// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./cm_quantile_test.rs"]
mod cm_quantile_test;

// This is a port of the code in:
// https://github.com/lyft/statsite/blob/master/src/cm_quantile.h and
// https://github.com/lyft/statsite/blob/master/src/cm_quantile.c
// To keep things "simple" I tried to replicate the data structures almost exactly. It's unclear
// if it would be better to just rewrite the algorithm from scratch or attempt to use one of the
// crates that implements this (none of them seemed popular and/or an exact match).

use adapter::SampleAdapter;
use intrusive_collections::linked_list::Cursor;
use intrusive_collections::{LinkedList, LinkedListLink};
use pulse_common::{LossyFloatToInt, LossyIntToFloat};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;
use std::ptr;

//
// Sample
//

// An individual sample, which implements Ord for insertion into the binary heap. This code assumes
// the value is never NaN.
struct Sample {
  value: f64,
  width: u64,
  delta: u64,
  link: LinkedListLink,
}

impl Sample {
  const fn new(value: f64) -> Self {
    Self {
      value,
      width: 0,
      delta: 0,
      link: LinkedListLink::new(),
    }
  }
}

impl PartialEq for Sample {
  fn eq(&self, other: &Self) -> bool {
    self.value == other.value
  }
}

impl Eq for Sample {}

#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for Sample {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    self.value.partial_cmp(&other.value)
  }
}

impl Ord for Sample {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.partial_cmp(other).unwrap()
  }
}

#[allow(clippy::expl_impl_clone_on_copy)]
mod adapter {
  use super::Sample;
  use intrusive_collections::{LinkedListLink, intrusive_adapter};

  intrusive_adapter!(pub(super) SampleAdapter = Box<Sample>: Sample { link: LinkedListLink });
}

//
// Quantile
//

// The implementation of the quantile approximation algorithm, the Cormode-Muthukrishnan algorithm
// for computation of biased quantiles over data streams from "Effective Computation of Biased
// Quantiles over Data Streams"
pub struct Quantile {
  epsilon: f64,
  quantiles: Vec<f64>,

  num_samples: u64,
  num_values: u64,

  samples: LinkedList<SampleAdapter>,
  buf_less: BinaryHeap<Reverse<Box<Sample>>>,
  buf_more: BinaryHeap<Reverse<Box<Sample>>>,

  // We have to use raw pointers here because I can't figure out a good way of storing a Cursor
  // or CursorMut from the library since they have lifetime dependencies.
  insert_cursor: *mut Sample,
  compress_cursor: *mut Sample,
  compress_min_rank: u64,
}

unsafe impl Send for Quantile {}
unsafe impl Sync for Quantile {}

impl Quantile {
  // Create a new quantile streamer.
  pub fn new(epsilon: f64, quantiles: Vec<f64>) -> Self {
    Self {
      epsilon,
      quantiles,
      num_samples: 0,
      num_values: 0,
      samples: LinkedList::new(SampleAdapter::new()),
      buf_less: BinaryHeap::new(),
      buf_more: BinaryHeap::new(),
      insert_cursor: ptr::null_mut(),
      compress_cursor: ptr::null_mut(),
      compress_min_rank: 0,
    }
  }

  // Add a sample to the quantile streamer.
  pub fn add_sample(&mut self, sample: f64) {
    self.add_to_buffer(sample);
    self.insert();
    self.compress();
  }

  // Adds a new sample to the buffer.
  fn add_to_buffer(&mut self, sample: f64) {
    let sample = Box::new(Sample::new(sample));

    // Check the cursor value. Only use buf_less if we have at least a single value.
    if self.num_values > 0
      && sample.value < Self::ptr_to_ref(&mut self.insert_cursor).map_or(0.0, |s| s.value)
    {
      self.buf_less.push(Reverse(sample));
    } else {
      self.buf_more.push(Reverse(sample));
    }
  }

  // Turn a pointer into a reference. A mutable reference to the pointer is passed to make sure
  // we have exclusive access. The pointer is assumed to be valid and in the samples list.
  fn ptr_to_ref(cursor: &mut *mut Sample) -> Option<&mut Sample> {
    if cursor.is_null() {
      None
    } else {
      unsafe { Some(&mut **cursor) }
    }
  }

  // Turn a cursor into a mutable pointer.
  fn cursor_to_ptr(cursor: &Cursor<'_, SampleAdapter>) -> *mut Sample {
    cursor.get().map_or(ptr::null_mut(), |c| {
      std::ptr::from_ref::<Sample>(c).cast_mut()
    })
  }

  // Given a pointer to an item in the list, return the pointer of the next element (or null).
  #[allow(clippy::needless_pass_by_ref_mut)]
  fn ptr_next(list: &mut LinkedList<SampleAdapter>, cursor: &mut *mut Sample) -> *mut Sample {
    let mut cursor = unsafe { list.cursor_from_ptr(*cursor) };
    cursor.move_next();
    Self::cursor_to_ptr(&cursor)
  }

  // Given a pointer to an item in the list, return the pointer of the previous element (or null).
  #[allow(clippy::needless_pass_by_ref_mut)]
  fn ptr_prev(list: &mut LinkedList<SampleAdapter>, cursor: &mut *mut Sample) -> *mut Sample {
    let mut cursor = unsafe { list.cursor_from_ptr(*cursor) };
    cursor.move_prev();
    Self::cursor_to_ptr(&cursor)
  }

  // Incrementally processes inserts by moving data from the buffer to the samples using a cursor.
  fn insert(&mut self) {
    // Check if this is the first element.
    if self.samples.is_empty() {
      if let Some(Reverse(mut sample)) = self.buf_more.pop() {
        sample.width = 1;
        sample.delta = 0;
        self.samples.push_front(sample);
        self.num_values += 1;
        self.num_samples += 1;
        self.insert_cursor = Self::cursor_to_ptr(&self.samples.front());
      }
      return;
    }

    // Check if we need to initialize the cursor.
    if self.insert_cursor.is_null() {
      self.insert_cursor = Self::cursor_to_ptr(&self.samples.front());
    }

    // Handle adding values in the middle.
    let incr_size = self.cursor_increment();
    for _ in 0 .. incr_size {
      if self.insert_cursor.is_null() {
        break;
      }
      while let Some(sample) = self.buf_more.peek_mut() {
        let insert_sample = Self::ptr_to_ref(&mut self.insert_cursor).unwrap();
        if sample.0.value > insert_sample.value {
          break;
        }
        let Reverse(mut sample) = PeekMut::pop(sample);
        sample.width = 1;
        sample.delta = insert_sample.width + insert_sample.delta - 1;

        // Check if we need to update the compress cursor.
        if Self::ptr_to_ref(&mut self.compress_cursor)
          .is_some_and(|compress_sample| compress_sample.value >= sample.value)
        {
          self.compress_min_rank += 1;
        }

        unsafe { self.samples.cursor_mut_from_ptr(insert_sample) }.insert_before(sample);
        self.num_values += 1;
        self.num_samples += 1;
      }

      // Increment the cursor.
      self.insert_cursor = Self::ptr_next(&mut self.samples, &mut self.insert_cursor);
    }

    // Handle adding values at the end.
    if self.insert_cursor.is_null() {
      while let Some(sample) = self.buf_more.peek_mut() {
        if sample.0.value <= self.samples.back().get().unwrap().value {
          break;
        }
        let Reverse(mut sample) = PeekMut::pop(sample);
        sample.width = 1;
        sample.delta = 0;
        self.samples.push_back(sample);
        self.num_values += 1;
        self.num_samples += 1;
      }

      self.reset_insert_cursor();
    }
  }

  // Resets the insert cursor.
  fn reset_insert_cursor(&mut self) {
    // Swap the buffers, reset the cursor.
    std::mem::swap(&mut self.buf_less, &mut self.buf_more);
    self.insert_cursor = ptr::null_mut();
  }

  // Computes the number of items to process in one iteration.
  fn cursor_increment(&self) -> u64 {
    (self.num_samples.lossy_to_f64() * self.epsilon)
      .ceil()
      .lossy_to_u64()
  }

  // Incrementally processes compression by using a cursor/
  fn compress(&mut self) {
    // Bail early if there is nothing to really compress.
    if self.num_samples < 3 {
      return;
    }

    // Check if we need to initialize the cursor.
    if self.compress_cursor.is_null() {
      let mut cursor = self.samples.back();
      cursor.move_prev();

      self.compress_min_rank = self.num_values - 1 - cursor.get().unwrap().width;
      cursor.move_prev();

      self.compress_cursor = Self::cursor_to_ptr(&cursor);
    }

    let incr_size = self.cursor_increment();
    for _ in 0 .. incr_size {
      if Self::cursor_to_ptr(&self.samples.front()) == self.compress_cursor {
        break;
      }

      let mut next = Self::ptr_next(&mut self.samples, &mut self.compress_cursor);
      let next_ref = Self::ptr_to_ref(&mut next).unwrap();
      let compressor_ref = Self::ptr_to_ref(&mut self.compress_cursor).unwrap();

      let max_rank = self.compress_min_rank + compressor_ref.width + compressor_ref.delta;
      self.compress_min_rank -= compressor_ref.width;
      let threshold = Self::threshold(
        &self.quantiles,
        self.epsilon,
        self.num_values,
        max_rank.lossy_to_f64(),
      );
      let test_val = compressor_ref.width + next_ref.width + next_ref.delta;
      if test_val <= threshold {
        // Combine the widths.
        next_ref.width += compressor_ref.width;

        // Make sure we don't stomp the insertion cursor.
        if self.insert_cursor == self.compress_cursor {
          self.insert_cursor = next;
        }

        // Remove the tuple.
        let mut cursor = unsafe { self.samples.cursor_mut_from_ptr(self.compress_cursor) };
        cursor.remove();
        cursor.move_prev();
        self.compress_cursor = Self::cursor_to_ptr(&cursor.as_cursor());

        // Reduce the sample count.
        self.num_samples -= 1;
      } else {
        self.compress_cursor = Self::ptr_prev(&mut self.samples, &mut self.compress_cursor);
      }
    }

    // Reset the cursor if we hit the start
    if Self::cursor_to_ptr(&self.samples.front()) == self.compress_cursor {
      self.compress_cursor = ptr::null_mut();
    }
  }

  fn threshold(quantiles: &[f64], epsilon: f64, num_values: u64, rank: f64) -> u64 {
    let mut min_val = f64::MAX;
    for quantile in quantiles {
      let quantile_min = if rank >= quantile * num_values.lossy_to_f64() {
        2.0 * epsilon * rank / quantile
      } else {
        2.0 * epsilon * (num_values.lossy_to_f64() - rank) / (1.0 - quantile)
      };
      if quantile_min < min_val {
        min_val = quantile_min;
      }
    }

    min_val.lossy_to_u64()
  }

  // Query for a particular quantile.
  pub fn query(&self, quantile: f64) -> f64 {
    let rank = (quantile * self.num_values.lossy_to_f64())
      .ceil()
      .lossy_to_u64();
    let mut min_rank = 0;
    let threshold = (Self::threshold(
      &self.quantiles,
      self.epsilon,
      self.num_values,
      rank.lossy_to_f64(),
    )
    .lossy_to_f64()
      / 2.0)
      .ceil()
      .lossy_to_u64();

    let mut prev_cursor = self.samples.front();
    let mut current_cursor = self.samples.front();
    while let Some(current) = current_cursor.get() {
      let max_rank = min_rank + current.width + current.delta;
      if max_rank > rank + threshold {
        break;
      }
      min_rank += current.width;
      prev_cursor = current_cursor.clone();
      current_cursor.move_next();
    }
    prev_cursor.get().map_or(0.0, |p| p.value)
  }

  // Flush to get the most accurate query results.
  pub fn flush(&mut self) {
    while !self.buf_less.is_empty() || !self.buf_more.is_empty() {
      if self.buf_more.is_empty() {
        self.reset_insert_cursor();
      }
      self.insert();
      self.compress();
    }
  }

  // Get the minimum sample value.
  pub fn min(&self) -> f64 {
    self.samples.front().get().map_or(0.0, |s| s.value)
  }

  // Get the maximum sample value.
  pub fn max(&self) -> f64 {
    self.samples.back().get().map_or(0.0, |s| s.value)
  }
}
