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
use crate::admin::server::AdminHandlerHandle;
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{MetricId, ParsedMetric};
use ahash::RandomState;
use async_trait::async_trait;
use axum::response::Response;
use bd_time::TimeDurationExt;
use bytes::{BufMut, Bytes, BytesMut};
use cardinality_tracker::cardinality_tracker_config::top_k::Group_by;
use cardinality_tracker::cardinality_tracker_config::tracking_type::Type;
use cardinality_tracker::cardinality_tracker_config::{Count, TopK};
use cardinality_tracker::CardinalityTrackerConfig;
use comfy_table::presets::NOTHING;
use comfy_table::Table;
use futures::FutureExt;
use hyperloglogplus::{HyperLogLog, HyperLogLogPlus};
use itertools::Itertools;
use parking_lot::Mutex;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::cardinality_tracker;
use regex::bytes::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use time::ext::NumericalDuration;
use time::Duration;
use tokio::time::MissedTickBehavior;
use topk::FilteredSpaceSaving;

//
// Tracker
//

// Generic trait for all kinds of trackers.
trait Tracker: Send + Sync {
  fn track(&self, metric: &ParsedMetric);
  fn rotate(&self);
  fn to_admin_output(&self) -> String;
}

//
// CountTracker
//

// Tracker for counting the approximate cardinality of a group of metrics identified by name.
struct CountTracker {
  name_regex: Regex,
  hyperloglog: Mutex<HyperLogLogPlus<MetricId, RandomState>>,
  previous_cardinality: Mutex<Option<f64>>,
}

impl CountTracker {
  fn new(config: &Count) -> anyhow::Result<Self> {
    let name_regex = Regex::new(&config.name_regex)?;
    Ok(Self {
      name_regex,
      // Precision is 2^10 or about 1024 bytes.
      hyperloglog: Mutex::new(HyperLogLogPlus::new(10, RandomState::new()).unwrap()),
      previous_cardinality: Mutex::new(None),
    })
  }
}

impl Tracker for CountTracker {
  fn track(&self, metric: &ParsedMetric) {
    // Insert only if the name regex is a match.
    if self.name_regex.is_match(metric.metric().get_id().name()) {
      self.hyperloglog.lock().insert(metric.metric().get_id());
    }
  }

  fn rotate(&self) {
    // During rotation we make a new HLL and swap it with the old one. Then we take the count
    // of the old one and move it into the previous_cardinality field.
    let new_hyperloglog = HyperLogLogPlus::new(10, RandomState::new()).unwrap();
    let mut previous_hyperloglog =
      std::mem::replace(&mut *self.hyperloglog.lock(), new_hyperloglog);
    *self.previous_cardinality.lock() = Some(previous_hyperloglog.count());
  }

  fn to_admin_output(&self) -> String {
    self
      .previous_cardinality
      .lock()
      .map_or(" populating".to_string(), |c| format!(" {c}"))
  }
}

//
// TopKTracker
//

// Tracker for counting the approximate topk cardinality of a group of metrics identified either
// by a name regex or a set of tags. Per the proto docs we operate in 2 modes to avoid blowing
// out memory.
enum TopKState {
  // During the estimation mode we use filtered space saving to determine the approximate topk.
  EstimatingTopK {
    topk: FilteredSpaceSaving<Bytes>,
  },
  // During the finalizing mode we take the previous approximate topk and use HLL to get the
  // approximate cardinality of the topk.
  FinalizingTopK {
    topk: HashMap<Bytes, HyperLogLogPlus<MetricId, RandomState>, RandomState>,
  },
}
enum GroupByType {
  NameRegex(Option<Regex>),
  TagNames(Vec<String>),
}
struct TopKTracker {
  topk_state: Mutex<TopKState>,
  group_by: GroupByType,
  previous_topk: Mutex<Option<Vec<(Bytes, f64)>>>,
  topk: usize,
}

impl TopKTracker {
  fn new(config: TopK) -> anyhow::Result<Self> {
    let topk = config.top_k as usize;
    Ok(Self {
      topk_state: Mutex::new(TopKState::EstimatingTopK {
        topk: FilteredSpaceSaving::new(topk),
      }),
      group_by: match config.group_by.expect("pgv") {
        Group_by::NameRegex(name_regex) => GroupByType::NameRegex(
          if name_regex.is_empty() {
            None
          } else {
            Some(Regex::new(&name_regex)?)
          },
        ),
        Group_by::TagNames(tag_names) => GroupByType::TagNames(
          tag_names
            .tags
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        ),
      },
      previous_topk: Mutex::new(None),
      topk,
    })
  }

  fn insert(&self, key: Bytes, metric: &MetricId) {
    match &mut *self.topk_state.lock() {
      TopKState::EstimatingTopK { topk } => {
        topk.insert(key, 1);
      },
      TopKState::FinalizingTopK { topk } => {
        // We ignore any keys that are not already in the map from the previous round.
        if let Some(hyperloglog) = topk.get_mut(&key) {
          hyperloglog.insert(metric);
        }
      },
    }
  }
}

impl Tracker for TopKTracker {
  fn track(&self, metric: &ParsedMetric) {
    match &self.group_by {
      GroupByType::NameRegex(name_regex) => {
        // If there is no regex, every metric is considered. Otherwise we perform the regex check
        // first.
        if name_regex
          .as_ref()
          .is_none_or(|regex| regex.is_match(metric.metric().get_id().name()))
        {
          self.insert(
            // TODO(mattklein123): In order to avoid storing a reference to the entire incoming
            // payload we make a full copy of the name here. In practice we could potentially use
            // MetricKey and some type of wrapper that hashes on just the name. We can consider
            // this in a follow up.
            metric.metric().get_id().name().to_vec().into(),
            metric.metric().get_id(),
          );
        }
      },
      GroupByType::TagNames(tag_names) => {
        // For this case we build a key from the tag values and insert that.
        let mut key = BytesMut::new();
        for (i, tag_name) in tag_names.iter().enumerate() {
          let tag_value = metric
            .metric()
            .get_id()
            .tag(tag_name)
            .map_or(b"unknown".as_slice(), |t| t.value.as_ref());
          if i > 0 {
            key.put_u8(b'_');
          }
          key.extend_from_slice(tag_value);
        }

        self.insert(key.freeze(), metric.metric().get_id());
      },
    }
  }

  fn rotate(&self) {
    let previous_state = {
      let mut topk_state = self.topk_state.lock();
      match &mut *topk_state {
        TopKState::EstimatingTopK { topk } => {
          // If we are in estimating mode we finalize the topk by creating a map of each key to
          // an HLL that will be used for the next phase cardinality count.
          let hyperloglog_map = topk
            .iter()
            .map(|(key, _)| {
              (
                key.clone(),
                HyperLogLogPlus::new(10, RandomState::new()).unwrap(),
              )
            })
            .collect();

          std::mem::replace(
            &mut *topk_state,
            TopKState::FinalizingTopK {
              topk: hyperloglog_map,
            },
          )
        },
        TopKState::FinalizingTopK { topk: _ } => {
          // If we are done finalizing we reset back to the beginning with a blank filtered
          // space saving structure.
          std::mem::replace(
            &mut *topk_state,
            TopKState::EstimatingTopK {
              topk: FilteredSpaceSaving::new(self.topk),
            },
          )
        },
      }
    };

    match previous_state {
      TopKState::EstimatingTopK { topk: _ } => {},
      TopKState::FinalizingTopK { topk } => {
        // In this case we pull out every key and approximate cardinality, and then sort it
        // first by count, and then by name.
        let mut previous_topk = Vec::with_capacity(topk.len());
        for (key, mut hyperloglog) in topk {
          previous_topk.push((key, hyperloglog.count()));
        }
        previous_topk.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        *self.previous_topk.lock() = Some(previous_topk);
      },
    }
  }

  fn to_admin_output(&self) -> String {
    let previous_topk = self.previous_topk.lock();
    (*previous_topk).as_ref().map_or_else(
      || " populating".to_string(),
      |previous_topk| {
        // This creates a nice table so everything is aligned in the output.
        let mut table = Table::new();
        table.load_preset(NOTHING);
        for (key, count) in previous_topk {
          table.add_row(vec![
            std::string::String::from_utf8_lossy(key).to_string(),
            count.to_string(),
          ]);
        }
        table.trim_fmt()
      },
    )
  }
}

//
// CardinalityTrackerProcessor
//

struct TrackerWrapper {
  tracker: Box<dyn Tracker>,
  name: String,
}
pub struct CardinalityTrackerProcessor {
  dispatcher: Arc<dyn PipelineDispatch>,
  trackers: Vec<TrackerWrapper>,
  admin_handler_handle: Mutex<Option<Box<dyn AdminHandlerHandle>>>,
}

impl CardinalityTrackerProcessor {
  pub fn new(
    config: CardinalityTrackerConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    let trackers = config
      .tracking_types
      .into_iter()
      .map(|tracking_type| {
        Ok::<_, anyhow::Error>(TrackerWrapper {
          tracker: match tracking_type.type_.expect("pgv") {
            Type::Count(count) => Box::new(CountTracker::new(&count)?) as Box<dyn Tracker>,
            Type::TopK(top_k) => Box::new(TopKTracker::new(top_k)?),
          },
          name: tracking_type.name.to_string(),
        })
      })
      .try_collect()?;

    let processor = Arc::new(Self {
      dispatcher: context.dispatcher,
      trackers,
      admin_handler_handle: Mutex::default(),
    });

    let cloned_processor = processor.clone();
    *processor.admin_handler_handle.lock() = Some(context.admin.register_handler(
      "/cardinality_tracker",
      Box::new(move |_| {
        let cloned_processor = cloned_processor.clone();
        async move { cloned_processor.admin_handler() }.boxed()
      }),
    )?);

    let cloned_processor = processor.clone();
    tokio::spawn(async move {
      cloned_processor
        .rotation_loop(config.rotate_after.unwrap_duration_or(5.minutes()))
        .await;
    });

    Ok(processor)
  }

  async fn rotation_loop(&self, duration: Duration) {
    let mut interval = duration.interval_at(MissedTickBehavior::Delay);
    loop {
      interval.tick().await;
      log::debug!("doing tracker rotation");
      for tracker in &self.trackers {
        tracker.tracker.rotate();
      }
      log::debug!("tracker rotation complete");
    }
  }

  fn admin_handler(&self) -> Response {
    let output = self
      .trackers
      .iter()
      .map(|tracker| {
        format!(
          "{} (approximate cardinality):\n{}",
          tracker.name,
          tracker.tracker.to_admin_output()
        )
      })
      .join("\n\n");
    Response::new(output.into())
  }
}

#[async_trait]
impl PipelineProcessor for CardinalityTrackerProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    for sample in &samples {
      for tracker in &self.trackers {
        tracker.tracker.track(sample);
      }
    }
    self.dispatcher.send(samples).await;
  }

  async fn start(self: Arc<Self>) {}
}
