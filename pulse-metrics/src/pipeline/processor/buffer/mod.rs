// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use bd_log::warn_every;
use bd_shutdown::ComponentShutdown;
use event_listener::{listener, Event};
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_protobuf::protos::pulse::config::processor::v1::buffer::BufferConfig;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use time::ext::NumericalDuration;

//
// Stats
//

struct Stats {
  dropped: IntCounter,
}

//
// BufferProcessor
//

// This is a basic buffer processor that accepts incoming metrics up to a limit before dropping.
// A fixed number of consumers pull metric batches and move them further along the pipeline.
// In the future we can add more sophisticated functionality such as spill to disk, a remote queue,
// etc. Note the current implementation is a LIFO queue. The oldest metrics will always be dropped
// if here is overflow.
pub(super) struct BufferProcessor {
  dispatcher: Arc<dyn PipelineDispatch>,
  stats: Stats,
  // TODO(mattklein123): I'm having a hard time finding a concurrent lock-free LIFO queue. Given
  // that we are operating unbounded, using Vec is probably good enough as over time it will
  // amortize to a steady state allocation and we will only be pushing and popping from the back.
  // If we are willing to switch to bounded a fixed size pre-allocated would be more efficient
  // along with a potentially lock-free implementation.
  lifo: Mutex<VecDeque<Vec<ParsedMetric>>>,
  event: Event,
  current_num_metrics: AtomicU64,
  max_num_metrics: u64,
}

impl BufferProcessor {
  pub(super) fn new(config: &BufferConfig, context: ProcessorFactoryContext) -> Arc<Self> {
    let processor = Arc::new(Self {
      dispatcher: context.dispatcher,
      stats: Stats {
        dropped: context.scope.counter("dropped"),
      },
      lifo: Mutex::default(),
      event: Event::new(),
      current_num_metrics: AtomicU64::default(),
      max_num_metrics: u64::from(config.max_buffered_metrics),
    });

    for _ in 0 .. config.num_consumers {
      let cloned_processor = processor.clone();
      let shutdown = context.shutdown_trigger_handle.make_shutdown();
      tokio::spawn(async move {
        cloned_processor.consume_metrics_loop(shutdown).await;
      });
    }

    processor
  }

  async fn recv(&self) -> Vec<ParsedMetric> {
    loop {
      if let Some(metrics) = self.lifo.lock().pop_back() {
        return metrics;
      }

      listener!(self.event => listener);

      if let Some(metrics) = self.lifo.lock().pop_back() {
        return metrics;
      }

      listener.await;
    }
  }

  async fn consume_metrics_loop(&self, mut shutdown: ComponentShutdown) {
    let shutdown = shutdown.cancelled();
    tokio::pin!(shutdown);

    loop {
      tokio::select! {
        () = &mut shutdown => break,
        samples = self.recv() => self.consume_metrics(samples).await,
      }
    }

    // Do a final drain at shutdown time.
    log::debug!("performing shutdown drain");
    while let Some(samples) = {
      let mut lifo = self.lifo.lock();
      lifo.pop_back()
    } {
      self.consume_metrics(samples).await;
    }

    log::debug!("shutdown drain complete");
  }

  async fn consume_metrics(&self, samples: Vec<ParsedMetric>) {
    log::debug!("forwarding {} sample(s)", samples.len());
    let num_samples = samples.len().try_into().unwrap();
    debug_assert!(self.current_num_metrics.load(Ordering::Relaxed) >= num_samples);
    self
      .current_num_metrics
      .fetch_sub(num_samples, Ordering::Relaxed);
    self.dispatcher.send(samples).await;
  }
}

#[async_trait]
impl PipelineProcessor for BufferProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let num_samples = samples.len().try_into().unwrap();
    let mut current_num_metrics = self
      .current_num_metrics
      .fetch_add(num_samples, Ordering::Relaxed)
      + num_samples;
    if current_num_metrics > self.max_num_metrics {
      warn_every!(
        1.minutes(),
        "{}",
        "buffer processor overflow. Dropping metrics"
      );

      // This loop is eventually consistent as multiple threads can race on overflow. It's not worth
      // making this accounting perfect.
      while current_num_metrics > self.max_num_metrics {
        if let Some(popped) = self.lifo.lock().pop_front() {
          log::debug!("dropping samples with size {}", popped.len());
          let popped_len = popped.len().try_into().unwrap();
          self.stats.dropped.inc_by(popped_len);
          current_num_metrics = self
            .current_num_metrics
            .fetch_sub(popped_len, Ordering::Relaxed)
            - popped_len;
        } else {
          break;
        }
      }
    }

    log::debug!("buffering {} sample(s)", samples.len());
    self.lifo.lock().push_back(samples);
    self.event.notify_additional(1);
  }

  async fn start(self: Arc<Self>) {}
}
