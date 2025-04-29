// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./stats_test.rs"]
mod stats_test;

use crate::protos::prom::{make_label, make_timeseries};
use bd_proto::protos::prometheus::prompb;
use bd_server_stats::stats::Collector;
use prometheus::proto::{Metric, MetricFamily, MetricType};
use prompb::remote::WriteRequest;
use prompb::types::MetricMetadata;
use protobuf::Chars;
use pulse_common::LossyIntToFloat;
use std::fmt::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const SEP: &str = ":";
const CARBON_SEP: &str = ".";

fn print_write_request(write_request: &WriteRequest) -> String {
  write_request
    .timeseries
    .iter()
    .fold(String::new(), |mut s, ts| {
      let labels: String = ts.labels.iter().fold(String::new(), |mut s, l| {
        write!(s, "[{}={}]", l.name, l.value).unwrap();
        s
      });
      writeln!(
        &mut s,
        "{}[value={}]",
        labels,
        ts.samples.first().map_or(f64::NAN, |s| s.value)
      )
      .unwrap();
      s
    })
}

#[derive(Clone)]
pub struct StatsProvider {
  collector: Collector,
  host: Option<Chars>,
  meta_tags: Vec<(Chars, Chars)>,
}

impl StatsProvider {
  #[must_use]
  pub fn new(host: Option<String>, meta_tags: Vec<(String, String)>) -> Self {
    Self {
      collector: Collector::default(),
      host: host.map(std::convert::Into::into),
      meta_tags: meta_tags
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect(),
    }
  }

  /// Generate and return a byte buffer containing a statsd formatted text
  /// output of the current contents of this collector.
  #[must_use]
  pub fn statsd_output(&self) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(65535);
    for mf in self.collector.gather() {
      for m in mf.get_metric() {
        let lines = self.format_statsd_lines(&mf, m);
        for line in lines {
          buffer.extend(line.as_bytes());
        }
      }
    }
    buffer
  }

  pub const fn collector(&self) -> &Collector {
    &self.collector
  }

  // Collect into a WriteRequest suitable for pushing via the Prometheus remote write API.
  #[must_use]
  pub fn prometheus_remote_write_output(&self) -> WriteRequest {
    let timestamp = i64::try_from(
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis(),
    )
    .unwrap();
    let mut timeseries = Vec::new();
    let mut metadata = Vec::new();
    let metrics = self.collector.gather();
    for metric_family in &metrics {
      let metric_name: Chars = metric_family.name().to_string().into();
      metadata.push(MetricMetadata {
        type_: match metric_family.get_field_type() {
          MetricType::COUNTER => prompb::types::metric_metadata::MetricType::COUNTER,
          MetricType::GAUGE => prompb::types::metric_metadata::MetricType::GAUGE,
          MetricType::HISTOGRAM => prompb::types::metric_metadata::MetricType::HISTOGRAM,
          _ => prompb::types::metric_metadata::MetricType::UNKNOWN,
        }
        .into(),
        metric_family_name: metric_name.clone(),
        ..Default::default()
      });

      for metric in metric_family.get_metric() {
        let mut labels: Vec<_> = metric
          .get_label()
          .iter()
          .map(|label| {
            make_label(
              label.name().to_string().into(),
              label.value().to_string().into(),
            )
          })
          .collect();
        for (name, value) in &self.meta_tags {
          labels.push(make_label(name.clone(), value.clone()));
        }
        if let Some(host) = &self.host {
          labels.push(make_label("source".into(), host.clone()));
        }

        match metric_family.get_field_type() {
          MetricType::COUNTER => {
            timeseries.push(make_timeseries(
              metric_name.clone(),
              labels,
              metric.get_counter().value(),
              timestamp,
              vec![],
            ));
          },
          MetricType::GAUGE => {
            timeseries.push(make_timeseries(
              metric_name.clone(),
              labels,
              metric.get_gauge().value(),
              timestamp,
              vec![],
            ));
          },
          MetricType::SUMMARY => log::warn!("summary not supported"),
          MetricType::UNTYPED => log::warn!("untyped not supported"),
          MetricType::HISTOGRAM => {
            let histogram = metric.get_histogram();
            for bucket in histogram.get_bucket() {
              timeseries.push(make_timeseries(
                format!("{}_bucket", metric_family.name()).into(),
                labels.clone(),
                bucket.cumulative_count().lossy_to_f64(),
                timestamp,
                vec![make_label(
                  "le".into(),
                  bucket.upper_bound().to_string().into(),
                )],
              ));
            }

            timeseries.push(make_timeseries(
              format!("{}_bucket", metric_family.name()).into(),
              labels.clone(),
              histogram.get_sample_count().lossy_to_f64(),
              timestamp,
              vec![make_label("le".into(), "+Inf".into())],
            ));
            timeseries.push(make_timeseries(
              format!("{}_count", metric_family.name()).into(),
              labels.clone(),
              histogram.get_sample_count().lossy_to_f64(),
              timestamp,
              vec![],
            ));
            timeseries.push(make_timeseries(
              format!("{}_sum", metric_family.name()).into(),
              labels.clone(),
              histogram.get_sample_sum(),
              timestamp,
              vec![],
            ));
          },
        }
      }
    }

    let write_request = WriteRequest {
      timeseries,
      metadata,
      ..Default::default()
    };
    log::debug!("{}", print_write_request(&write_request));
    write_request
  }

  fn format_statsd_lines(&self, mf: &MetricFamily, m: &Metric) -> Vec<String> {
    let metric_type = mf.get_field_type();

    // TODO(mattklein123): The following seems broken to me but in the interest of least change
    // leaving it for now.
    let host = self.host.as_ref().map_or("0", |h| &**h);
    let mut point_tags = format!("source:{host}");
    for l in m.get_label() {
      let key = l.name();
      let val = l.value();
      point_tags = format!("{point_tags},{key}:{val}");
    }
    for (key, val) in &self.meta_tags {
      point_tags = format!("{point_tags},{key}:{val}");
    }

    match metric_type {
      MetricType::COUNTER => {
        let value = m.get_counter().value().to_string();

        vec![Self::format_statsd_line(
          mf.name(),
          "count",
          value.as_str(),
          "c",
          point_tags.as_str(),
        )]
      },
      MetricType::GAUGE => {
        let value = m.get_gauge().value().to_string();
        vec![Self::format_statsd_line(
          mf.name(),
          "gauge",
          value.as_str(),
          "g",
          point_tags.as_str(),
        )]
      },
      MetricType::SUMMARY => todo!(),
      MetricType::UNTYPED => todo!(),
      MetricType::HISTOGRAM => todo!(),
    }
  }

  fn format_statsd_line(
    metric_name: &str,
    metric_name_suffix: &str,
    metric_value: &str,
    metric_type: &str,
    point_tags: &str,
  ) -> String {
    let name_with_suffix = format!("{metric_name}{SEP}{metric_name_suffix}");
    let name_with_suffix = name_with_suffix.replace(SEP, CARBON_SEP);
    format!("{name_with_suffix}:{metric_value}|{metric_type}#{point_tags}\n")
  }

  /// Generate and return a byte buffer containing a carbon formatted text
  /// output of the current contents of this collector.
  #[must_use]
  pub fn carbon_output(&self, timestamp: u64) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(65535);
    for mf in self.collector.gather() {
      for m in mf.get_metric() {
        let lines = self.format_carbon_lines(&mf, m, timestamp);
        for line in lines {
          buffer.extend(line.as_bytes());
        }
      }
    }
    buffer
  }

  fn format_carbon_lines(&self, mf: &MetricFamily, m: &Metric, timestamp: u64) -> Vec<String> {
    let metric_type = mf.get_field_type();

    // TODO(mattklein123): The following seems broken to me but in the interest of least change
    // leaving it for now.
    let host = self.host.as_ref().map_or("0", |h| &**h);
    let mut point_tags = format!("source=\"{host}\"");
    for l in m.get_label() {
      let key = l.name();
      let val = l.value();
      point_tags = format!("{point_tags} {key}=\"{val}\"");
    }
    for (key, val) in &self.meta_tags {
      point_tags = format!("{point_tags} {key}=\"{val}\"");
    }

    match metric_type {
      MetricType::COUNTER => {
        let value = m.get_counter().value().to_string();
        vec![Self::format_carbon_line(
          mf.name(),
          "count",
          value.as_str(),
          timestamp,
          point_tags.as_str(),
        )]
      },
      MetricType::GAUGE => {
        let value = m.get_gauge().value().to_string();
        vec![Self::format_carbon_line(
          mf.name(),
          "gauge",
          value.as_str(),
          timestamp,
          point_tags.as_str(),
        )]
      },
      MetricType::SUMMARY => todo!(),
      MetricType::UNTYPED => todo!(),
      MetricType::HISTOGRAM => {
        let histogram = m.get_histogram();
        let buckets = histogram.get_bucket();
        let mut lines = Vec::with_capacity(buckets.len() + 2);
        lines.push(Self::format_carbon_line(
          mf.name(),
          "count",
          histogram.get_sample_count().to_string().as_str(),
          timestamp,
          point_tags.as_str(),
        ));
        lines.push(Self::format_carbon_line(
          mf.name(),
          "sum",
          histogram.get_sample_sum().to_string().as_str(),
          timestamp,
          point_tags.as_str(),
        ));
        for bucket in buckets {
          let upper_bound = bucket.upper_bound();
          let value = bucket.cumulative_count().to_string();
          let point_tags = format!("{point_tags} le=\"{upper_bound}\"");
          lines.push(Self::format_carbon_line(
            mf.name(),
            "bucket",
            value.as_str(),
            timestamp,
            point_tags.as_str(),
          ));
        }
        // Add the implicit "+Inf" bucket
        let point_tags = format!("{point_tags} le=\"+Inf\"");
        lines.push(Self::format_carbon_line(
          mf.name(),
          "bucket",
          histogram.get_sample_count().to_string().as_str(),
          timestamp,
          point_tags.as_str(),
        ));
        lines
      },
    }
  }

  fn format_carbon_line(
    metric_name: &str,
    metric_name_suffix: &str,
    metric_value: &str,
    timestamp: u64,
    point_tags: &str,
  ) -> String {
    let name_with_suffix = format!("{metric_name}{SEP}{metric_name_suffix}");
    let name_with_suffix = name_with_suffix.replace(SEP, CARBON_SEP);
    format!("{name_with_suffix} {metric_value} {timestamp} {point_tags}\n")
  }
}
