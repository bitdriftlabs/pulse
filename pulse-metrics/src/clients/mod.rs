// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod client;
pub mod client_pool;
pub mod http;
pub mod retry;

use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use time::Duration;

#[must_use]
pub fn make_tls_connector(connect_timeout: Duration) -> HttpsConnector<HttpConnector> {
  let mut connector = HttpConnector::new();
  connector.set_connect_timeout(Some(connect_timeout.unsigned_abs()));
  connector.enforce_http(false);

  // TODO(mattklein123): When talking to AMP we were seeing errors like:
  // hyper error: http2 error: connection error received: not a result of an error
  // The assumption is this is GOAWAY but it's unclear how to get more information. After switching
  // to HTTP/1 it's clear the issue was remote connection idle timeouts which are fixed with
  // retries. Consider turning HTTP/2 back on.
  HttpsConnectorBuilder::new()
    .with_native_roots()
    .unwrap()
    .https_or_http()
    .enable_http1()
    .wrap_connector(connector)
}
