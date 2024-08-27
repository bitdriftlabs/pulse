// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::cell::RefCell;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

// Determine a next flush interval pegged against wall clock time.
pub fn next_flush_interval(time_provider: &dyn TimeProvider, flush_interval_s: i64) -> Duration {
  let now = time_provider.unix_now();
  let remainder = now % flush_interval_s;
  Duration::seconds(flush_interval_s - remainder)
}

//
// TimeProvider
//

pub trait TimeProvider: Send + Sync + 'static {
  fn now_utc(&self) -> OffsetDateTime;
  fn unix_now(&self) -> i64;
}

//
// RealTimeProvider
//

pub struct RealTimeProvider {}

impl TimeProvider for RealTimeProvider {
  fn now_utc(&self) -> OffsetDateTime {
    OffsetDateTime::now_utc()
  }

  fn unix_now(&self) -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
  }
}

//
// TestTimeProvider
//

#[derive(Default)]
pub struct TestTimeProvider {
  pub time: Arc<AtomicI64>,
}

impl TimeProvider for TestTimeProvider {
  fn now_utc(&self) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(self.time.load(Ordering::SeqCst)).unwrap()
  }

  fn unix_now(&self) -> i64 {
    self.time.load(Ordering::SeqCst)
  }
}

//
// DurationJitter
//

pub trait DurationJitter: Send + Sync {
  // Jitter from duration / 2 ..= duration
  fn half_jitter_duration(input: Duration) -> Duration;

  // jitter from 0 ..= duration
  fn full_jitter_duration(input: Duration) -> Duration;
}

//
// RealDurationJitter
//

pub struct RealDurationJitter {}

impl RealDurationJitter {
  fn jitter_worker(min_millis: u64, max_millis: u64) -> Duration {
    thread_local! {
      static RANDOM: RefCell<SmallRng> = RefCell::new(SmallRng::from_entropy());
    }

    let jittered_as_millis =
      RANDOM.with(|random| random.borrow_mut().gen_range(min_millis ..= max_millis));
    Duration::milliseconds(jittered_as_millis.try_into().unwrap())
  }
}

impl DurationJitter for RealDurationJitter {
  fn half_jitter_duration(input: Duration) -> Duration {
    let as_millis = input.whole_milliseconds() as u64;
    Self::jitter_worker(as_millis / 2, as_millis)
  }

  fn full_jitter_duration(input: Duration) -> Duration {
    let as_millis = input.whole_milliseconds() as u64;
    Self::jitter_worker(0, as_millis)
  }
}

//
// TestDurationJitter
//

pub struct TestDurationJitter {}

impl DurationJitter for TestDurationJitter {
  fn half_jitter_duration(input: Duration) -> Duration {
    input
  }

  fn full_jitter_duration(input: Duration) -> Duration {
    input
  }
}
