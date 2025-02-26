// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::inflow::prom_remote_write::DownstreamIdProviderImpl;
use crate::protos::metric::{DownstreamId, DownstreamIdProvider, MetricId};
use crate::test::make_metric_id;
use axum::extract::Request;
use prom_remote_write::prom_remote_write_server_config;
use prom_remote_write_server_config::downstream_id_source::Source_type;
use prom_remote_write_server_config::DownstreamIdSource;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use snap::raw::Encoder;
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use protobuf::Message;

#[test]
fn downstream_id_source() {
  {
    assert_eq!(
      DownstreamId::InflowProvided("127.0.0.1".into()),
      DownstreamIdProviderImpl::new(
        false,
        None,
        "127.0.0.1".parse().unwrap(),
        &Request::new(().into())
      )
      .downstream_id(&MetricId::new("".into(), None, vec![], true).unwrap())
    );
  }

  {
    assert_eq!(
      DownstreamId::InflowProvided("127.0.0.1".into()),
      DownstreamIdProviderImpl::new(
        false,
        Some(&DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        }),
        "127.0.0.1".parse().unwrap(),
        &Request::new(().into())
      )
      .downstream_id(&MetricId::new("".into(), None, vec![], true).unwrap())
    );
  }

  {
    assert_eq!(
      DownstreamId::InflowProvided("bar".into()),
      DownstreamIdProviderImpl::new(
        false,
        Some(&DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        }),
        "127.0.0.1".parse().unwrap(),
        &Request::builder()
          .header("foo", "bar")
          .body(().into())
          .unwrap()
      )
      .downstream_id(&make_metric_id("blah", None, &[("foo", "bar")]))
    );
  }

  {
    assert_eq!(
      DownstreamId::InflowProvided("bar:foo=bar".into()),
      DownstreamIdProviderImpl::new(
        true,
        Some(&DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        }),
        "127.0.0.1".parse().unwrap(),
        &Request::builder()
          .header("foo", "bar")
          .body(().into())
          .unwrap()
      )
      .downstream_id(&make_metric_id("blah", None, &[("foo", "bar")]))
    );
  }
}

// Debug By Kai
// {
//   "timeseries": [
//     {
//       "labels": [
//         {"name": "__name__", "value": "http_requests_total"},
//         {"name": "method", "value": "post"},
//         {"name": "code", "value": "200"}
//       ],
//       "samples": [
//         {"value": 1027, "timestamp": 1609459200000},
//         {"value": 1028, "timestamp": 1609459260000}
//       ]
//     }
//   ]
// }
#[test]
// fn test_decode_body_into_write_request() {
//     // Create a sample WriteRequest
//     let mut write_request = WriteRequest::new();
//     let mut timeseries = write_request.mut_timeseries().push_default();
//     let mut label = timeseries.mut_labels().push_default();
//     label.set_name("__name__".to_string());
//     label.set_value("http_requests_total".to_string());
//     let mut sample = timeseries.mut_samples().push_default();
//     sample.set_value(1027.0);
//     sample.set_timestamp(1609459200000);

//     // Serialize the WriteRequest to bytes
//     let serialized_request = write_request.write_to_bytes().unwrap();

//     // Compress the serialized bytes using Snappy
//     let compressed_request = Encoder::new().compress_vec(&serialized_request).unwrap();

//     // Decode the compressed request
//     let decoded_request = super::decode_body_into_write_request(&compressed_request).unwrap();

//     // Assert that the decoded request matches the original WriteRequest
//     assert_eq!(write_request, decoded_request);
// }
