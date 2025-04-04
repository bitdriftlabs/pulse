// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./retry_test.rs"]
mod retry_test;

use anyhow::bail;
use backoff::backoff::Backoff;
use futures::Future;
use parking_lot::Mutex;
use pulse_common::LossyIntoToFloat;
use pulse_protobuf::protos::pulse::config::common::v1::retry::RetryPolicy;
use std::sync::Arc;
use tokio::time::sleep;

const DEFAULT_BUDGET: f64 = 0.1;

#[derive(Default)]
struct LockedData {
  active_requests: u64,
  active_retries: u64,
}

//
// Retry
//

pub struct Retry {
  config: RetryPolicy,
  locked_data: Mutex<LockedData>,
}

impl Retry {
  pub fn new(config: &RetryPolicy) -> anyhow::Result<Arc<Self>> {
    let budget = config.budget.unwrap_or(DEFAULT_BUDGET);
    if !budget.is_finite() || budget <= 0.0 || budget > 1.0 {
      bail!("retry budget must be between > 0.0 and <= 1.0");
    }

    Ok(Arc::new(Self {
      config: config.clone(),
      locked_data: Mutex::default(),
    }))
  }

  async fn maybe_retry(
    &self,
    retry_count: &mut u32,
    backoff: &mut impl Backoff,
    n: &mut impl FnMut(),
  ) -> bool {
    *retry_count += 1;
    if self
      .config
      .max_retries
      .is_some_and(|max_retries| *retry_count > max_retries)
    {
      log::debug!("no further retries available (max retries)");
      return false;
    }

    let Some(backoff) = backoff.next_backoff() else {
      log::debug!("no further retries available (backoff exhausted)");
      return false;
    };

    if !{
      let mut locked_data = self.locked_data.lock();
      let active_requests = locked_data.active_requests.lossy_to_f64();
      let active_retries = locked_data.active_retries.lossy_to_f64();
      if active_retries >= (active_requests * self.config.budget.unwrap_or(0.1)) {
        log::debug!("retry budget not available");
        false
      } else {
        log::debug!("doing retry");
        locked_data.active_retries += 1;
        true
      }
    } {
      return false;
    }

    n();
    if !backoff.is_zero() {
      sleep(backoff).await;
    }
    log::debug!("retry sleep complete");
    true
  }

  #[allow(unused_assignments)] // This appears to be a compiler bug in this function.
  pub async fn retry_notify<T, E, F: Future<Output = Result<T, backoff::Error<E>>>>(
    &self,
    mut backoff: impl Backoff,
    mut f: impl FnMut() -> F,
    mut n: impl FnMut(),
  ) -> Result<T, E> {
    self.locked_data.lock().active_requests += 1;
    let mut doing_retry = false;
    let mut retry_count = 0;
    let result = loop {
      let result = f().await;
      if doing_retry {
        let mut locked_data = self.locked_data.lock();
        debug_assert!(locked_data.active_retries > 0);
        locked_data.active_retries -= 1;
        doing_retry = false;
      }

      match result {
        Ok(result) => break Ok(result),
        Err(backoff::Error::Permanent(e)) => break Err(e),
        Err(backoff::Error::Transient { err, .. }) => {
          if self
            .maybe_retry(&mut retry_count, &mut backoff, &mut n)
            .await
          {
            doing_retry = true;
          } else {
            break Err(err);
          }
        },
      }
    };

    {
      let mut locked_data = self.locked_data.lock();
      debug_assert!(locked_data.active_requests > 0);
      locked_data.active_requests -= 1;
    }
    result
  }
}
