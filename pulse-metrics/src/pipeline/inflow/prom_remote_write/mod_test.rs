// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::inflow::http_inflow::DownstreamIdProviderImpl;
use crate::protos::metric::{DownstreamId, DownstreamIdProvider, MetricId};
use crate::test::make_metric_id;
use axum::extract::Request;
use inflow_common::DownstreamIdSource;
use inflow_common::downstream_id_source::Source_type;
use protobuf::Message;
use pulse_protobuf::protos::pulse::config::inflow::v1::inflow_common;

#[test]
fn downstream_id_source() {
  {
    assert_eq!(
      DownstreamId::InflowProvided("127.0.0.1".into()),
      DownstreamIdProviderImpl::new(
        DownstreamIdSource::default_instance(),
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
        &DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        },
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
        &DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        },
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
        &DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          append_tags_to_downstream_id: true,
          ..Default::default()
        },
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
