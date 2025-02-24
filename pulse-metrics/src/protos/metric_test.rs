// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::protos::prom::{MetadataType, make_label};
use crate::test::make_tag;
use anyhow::bail;
use bd_proto::protos::prometheus::prompb;
use pretty_assertions::assert_eq;
use prompb::remote::WriteRequest;
use prompb::types::metric_metadata::MetricType as PromMetricType;
use prompb::types::{MetricMetadata, Sample, TimeSeries};
use quickcheck_macros::quickcheck;

// TODO(mattklein123): Add support for histograms.
pub mod arbitraries {
  use super::*;
  use crate::test::make_tag;

  impl quickcheck::Arbitrary for MetricType {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
      match u8::arbitrary(g) {
        0 ..= 50 => Self::Counter(CounterType::Delta),
        51 ..= 101 => Self::DirectGauge,
        102 ..= 152 => Self::Gauge,
        153 ..= 255 => Self::Timer,
      }
    }
  }

  impl quickcheck::Arbitrary for MetricId {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
      let mtype = Option::<MetricType>::arbitrary(g);

      // One metric name per type to ensure unqiue metric/type pairs.
      let name: bytes::Bytes = match mtype {
        Some(MetricType::Counter(CounterType::Delta)) => "foo.bar.c",
        Some(MetricType::DeltaGauge) => "foo.bar.baz.g",
        Some(MetricType::DirectGauge) => "foo.bar.G",
        Some(MetricType::Gauge) => "foo.bar.g",
        Some(
          MetricType::Histogram
          | MetricType::Summary
          | MetricType::Counter(CounterType::Absolute)
          | MetricType::BulkTimer,
        ) => unreachable!(),
        Some(MetricType::Timer) => "foo.bar.ms",
        None => "foo.bar",
      }
      .into();

      let potential_tags = vec![
        make_tag("name", "value"),
        make_tag("name2", "value2"),
        make_tag("name3", "value:value:value"),
        make_tag("name4", "value2:value2:value2"),
        make_tag("atag", "avalue:withanextracolon"),
        make_tag("tags.extra.name", "value=iscool"),
      ];
      let n_tags = usize::arbitrary(g) % (potential_tags.len() + 1);
      let tags = potential_tags[0 .. n_tags].to_vec();

      Self::new(name, mtype, tags, false).unwrap()
    }
  }

  impl quickcheck::Arbitrary for Metric {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
      let id = MetricId::arbitrary(g);
      let sample_rate = match u8::arbitrary(g) {
        0 ..= 127 => None,
        128 ..= 255 => Some(1. - f64::from(u8::arbitrary(g)) / 256.),
      };
      // timestamp should be a positive integer
      let timestamp = u64::from(u32::arbitrary(g)) + 1;
      let value = match id.mtype {
        // Gauge is non-negative. Otherwise, it is a DeltaGauge.
        Some(MetricType::Gauge) => f64::from(u16::arbitrary(g)) * 1.25,
        _ => f64::from(i16::arbitrary(g)) * 1.25,
      };

      Self {
        id,
        sample_rate,
        timestamp,
        value: MetricValue::Simple(value), //
      }
    }
  }

  impl quickcheck::Arbitrary for ParsedMetric {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
      Self {
        metric: Metric::arbitrary(g),
        source: MetricSource::PromRemoteWrite,
        received_at: Instant::now(),
        cached_metric: CachedMetric::Overflow,
        downstream_id: DownstreamId::LocalOrigin,
        metadata: None,
      }
    }
  }
}

#[quickcheck]
#[allow(clippy::needless_pass_by_value)]
fn metrics_roundtrip_write_request(input: Vec<ParsedMetric>) -> anyhow::Result<()> {
  let write_request = ParsedMetric::to_write_request(
    input.clone(),
    &ToWriteRequestOptions {
      metadata: MetadataType::Normal,
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );
  // TODO(mattklein123): Make this test work both with and without the summary hack as well as the
  // counter hack.
  let (output, errors) = from_write_request(
    write_request,
    &ParseConfig {
      summary_as_timer: true,
      counter_as_delta: true,
      ..Default::default()
    },
  );
  assert!(errors.is_empty());
  if input.len() != output.len() {
    bail!(
      "mismatched lengths: input {:?}, output: {:?}",
      input,
      output
    );
  }
  for input_metric in input.clone() {
    let matching = output
      .iter()
      .find(|output_metric| &input_metric.metric == *output_metric);
    if Some(&input_metric.metric.clone()) != matching {
      bail!(
        "input missing from output: input {:?}, output: {:?}",
        input_metric,
        output
      );
    }
  }
  for output_metric in output {
    let matching = input
      .iter()
      .find(|input_metric| input_metric.metric == output_metric)
      .map(|metric| &metric.metric);
    if Some(&output_metric.clone()) != matching {
      bail!(
        "output missing from input: output {:?}, input: {:?}",
        output_metric,
        input
      );
    }
  }
  Ok(())
}

#[test]
fn metrics_from_write_request() {
  // TODO(mattklein123): Make this test work both with and without the summary hack as well as the
  // counter hack.
  let metric_types = vec![
    (
      PromMetricType::COUNTER,
      Some(MetricType::Counter(CounterType::Delta)),
    ),
    (PromMetricType::GAUGE, Some(MetricType::Gauge)),
    (PromMetricType::SUMMARY, Some(MetricType::Timer)),
    (PromMetricType::UNKNOWN, None),
  ];

  for (prom_metric_type, expected_metric_type) in metric_types {
    let write_request: WriteRequest = WriteRequest {
      timeseries: vec![TimeSeries {
        labels: vec![
          make_label("tag1".into(), "value1".into()),
          make_label("tag2".into(), "value2".into()),
          make_label("__name__".into(), "metric_name".into()),
        ],
        samples: vec![
          Sample {
            value: 5.,
            timestamp: 1000,
            sample_rate: 0.,
            ..Default::default()
          },
          Sample {
            value: 10.,
            timestamp: 2500,
            sample_rate: 0.5,
            ..Default::default()
          },
        ],
        exemplars: vec![],
        ..Default::default()
      }],
      metadata: vec![MetricMetadata {
        metric_family_name: "metric_name".into(),
        type_: prom_metric_type.into(),
        ..Default::default()
      }],
      failthrough_statsd_lines: vec![],
      ..Default::default()
    };

    // TODO(mattklein123): Make this test work both with and without the summary hack as well as the
    // counter hack.
    let (metrics, errors) = from_write_request(
      write_request,
      &ParseConfig {
        summary_as_timer: true,
        counter_as_delta: true,
        ..Default::default()
      },
    );
    assert!(errors.is_empty());
    let expected_metric_id = MetricId {
      name: "metric_name".into(),
      mtype: expected_metric_type,
      tags: vec![make_tag("tag1", "value1"), make_tag("tag2", "value2")],
    };

    assert_eq!(
      metrics,
      vec![
        Metric {
          id: expected_metric_id.clone(),
          sample_rate: None,
          timestamp: 1,
          value: MetricValue::Simple(5.),
        },
        Metric {
          id: expected_metric_id.clone(),
          sample_rate: Some(0.5),
          timestamp: 2,
          value: MetricValue::Simple(10.),
        }
      ]
    );
  }
}

#[quickcheck]
fn metrics_to_write_request_metadata_only(input: ParsedMetric) -> anyhow::Result<()> {
  let write_request = ParsedMetric::to_write_request(
    vec![input],
    &ToWriteRequestOptions {
      metadata: MetadataType::Only,
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );
  assert_eq!(write_request.timeseries.len(), 1);
  assert!(write_request.timeseries[0].samples.is_empty());
  Ok(())
}
