// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::downstream_id;
use crate::protos::metric::DownstreamId;
use axum::extract::Request;
use prom_remote_write::prom_remote_write_server_config;
use prom_remote_write_server_config::downstream_id_source::Source_type;
use prom_remote_write_server_config::DownstreamIdSource;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;

#[test]
fn downstream_id_source() {
  {
    assert_eq!(
      DownstreamId::IpAddress("127.0.0.1".parse().unwrap()),
      downstream_id(None, "127.0.0.1".parse().unwrap(), &Request::new(().into()))
    );
  }

  {
    assert_eq!(
      DownstreamId::IpAddress("127.0.0.1".parse().unwrap()),
      downstream_id(
        Some(&DownstreamIdSource {
          source_type: Some(Source_type::RequestHeader("foo".into())),
          ..Default::default()
        }),
        "127.0.0.1".parse().unwrap(),
        &Request::new(().into())
      )
    );
  }

  {
    assert_eq!(
      DownstreamId::InflowProvided("bar".into()),
      downstream_id(
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
    );
  }
}
