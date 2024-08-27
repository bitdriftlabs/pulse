// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::aggregation::test::HelperBuilder;
use crate::protos::metric::{HistogramBucket, HistogramData, MetricType, MetricValue};
use crate::protos::prom::prom_stale_marker;
use bd_time::TimeDurationExt;
use time::ext::NumericalDuration;

#[tokio::test(start_paused = true)]
async fn histograms() {
  let mut helper = HelperBuilder::default()
    .emit_absolute_counters_as_delta_rate()
    .enable_emit_prometheus_stale_markers()
    .build()
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Histogram,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 10.0,
            le: 0.05,
          },
          HistogramBucket {
            count: 20.0,
            le: 0.1,
          },
        ],
        sample_count: 40.0,
        sample_sum: 150.0,
      }),
    )])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Histogram,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 15.0,
            le: 0.05,
          },
          HistogramBucket {
            count: 30.0,
            le: 0.1,
          },
        ],
        sample_count: 60.0,
        sample_sum: 175.0,
      }),
    )])
    .await;
  let expected = vec![helper.expected_metric(
    "hello",
    MetricValue::Histogram(HistogramData {
      buckets: vec![
        HistogramBucket {
          count: 0.083_333_333_333_333_33,
          le: 0.05,
        },
        HistogramBucket {
          count: 0.166_666_666_666_666_66,
          le: 0.1,
        },
      ],
      sample_count: 0.333_333_333_333_333_3,
      sample_sum: 0.416_666_666_666_666_7,
    }),
    MetricType::Histogram,
  )];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  let expected = vec![helper.expected_metric(
    "hello",
    MetricValue::Histogram(HistogramData {
      buckets: vec![
        HistogramBucket {
          count: prom_stale_marker(),
          le: 0.05,
        },
        HistogramBucket {
          count: prom_stale_marker(),
          le: 0.1,
        },
      ],
      sample_count: prom_stale_marker(),
      sample_sum: prom_stale_marker(),
    }),
    MetricType::Histogram,
  )];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn histogram_bucket_mismatch() {
  let mut helper = HelperBuilder::default().build().await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Histogram,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 10.0,
            le: 0.05,
          },
          HistogramBucket {
            count: 20.0,
            le: 0.1,
          },
        ],
        sample_count: 40.0,
        sample_sum: 150.0,
      }),
    )])
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Histogram,
      MetricValue::Histogram(HistogramData {
        buckets: vec![HistogramBucket {
          count: 5.0,
          le: 0.05,
        }],
        sample_count: 20.0,
        sample_sum: 125.0,
      }),
    )])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Histogram,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 11.0,
            le: 0.05,
          },
          HistogramBucket {
            count: 21.0,
            le: 0.1,
          },
        ],
        sample_count: 41.0,
        sample_sum: 175.0,
      }),
    )])
    .await;
  let expected = vec![helper.expected_metric(
    "hello",
    MetricValue::Histogram(HistogramData {
      buckets: vec![
        HistogramBucket {
          count: 1.0,
          le: 0.05,
        },
        HistogramBucket {
          count: 1.0,
          le: 0.1,
        },
      ],
      sample_count: 1.0,
      sample_sum: 25.0,
    }),
    MetricType::Histogram,
  )];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}
