// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::k8s::services::{MockServiceFetcher, ServiceCache};
use crate::k8s::test::make_object_meta;
use k8s_openapi::api::core::v1::Pod;
use time::ext::{NumericalDuration, NumericalStdDuration};
use vrl::btreemap;

#[tokio::test(start_paused = true)]
async fn test_services_call_failure() {
  let mut fetcher = MockServiceFetcher::new();
  fetcher
    .expect_services_for_namespace()
    .withf(|namespace| namespace == "default")
    .returning(|_| Err(anyhow::anyhow!("Service fetcher error")));
  let mut cache = ServiceCache::new(15.minutes(), Box::new(fetcher));
  assert!(
    cache
      .find_services(&Pod {
        metadata: make_object_meta("pod1", btreemap!(), btreemap!()),
        ..Default::default()
      })
      .await
      .is_empty()
  );
}

#[tokio::test(start_paused = true)]
async fn test_services_call_caching() {
  let mut fetcher = MockServiceFetcher::new();
  fetcher
    .expect_services_for_namespace()
    .withf(|namespace| namespace == "default")
    .times(2)
    .returning(|_| Ok(vec![]));
  let mut cache = ServiceCache::new(15.minutes(), Box::new(fetcher));
  assert!(
    cache
      .find_services(&Pod {
        metadata: make_object_meta("pod1", btreemap!(), btreemap!()),
        ..Default::default()
      })
      .await
      .is_empty()
  );
  assert!(
    cache
      .find_services(&Pod {
        metadata: make_object_meta("pod1", btreemap!(), btreemap!()),
        ..Default::default()
      })
      .await
      .is_empty()
  );

  tokio::time::sleep(15.std_minutes()).await;
  assert!(
    cache
      .find_services(&Pod {
        metadata: make_object_meta("pod1", btreemap!(), btreemap!()),
        ..Default::default()
      })
      .await
      .is_empty()
  );
}
