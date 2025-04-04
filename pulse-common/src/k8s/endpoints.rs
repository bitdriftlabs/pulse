use super::watcher_base::{ResourceWatchCallbacks, WatcherBase};
use async_trait::async_trait;
use bd_shutdown::ComponentShutdown;
use k8s_openapi::api::core::v1::Endpoints;
use kube::Api;
use tokio::sync::watch;

// TODO(mattklein123): For right now for simplicity this uses Endpoints vs. EndpointSlice. We can
// improve this later if there is demand for using this for very large services that change
// often.
pub struct EndpointsWatcher {
  tx: watch::Sender<Option<Endpoints>>,
}

impl EndpointsWatcher {
  pub async fn create(
    namespace: &str,
    name: &str,
    shutdown: ComponentShutdown,
  ) -> anyhow::Result<watch::Receiver<Option<Endpoints>>> {
    let client = kube::Client::try_default().await?;
    let endpoints_api: Api<Endpoints> = kube::Api::namespaced(client, namespace);
    let field_selector = Some(format!("metadata.name={name}"));
    let (tx, rx) = watch::channel(None);
    WatcherBase::create(
      format!("endpoints {namespace}/{name}"),
      endpoints_api,
      field_selector,
      Self { tx },
      shutdown,
    )
    .await;

    Ok(rx)
  }
}

#[async_trait]
impl ResourceWatchCallbacks<Endpoints> for EndpointsWatcher {
  async fn apply(&mut self, resource: Endpoints) {
    self.tx.send(Some(resource)).unwrap();
  }

  async fn delete(&mut self, _resource: Endpoints) {
    self.tx.send(None).unwrap();
  }

  async fn init_apply(&mut self, resource: Endpoints) {
    self.tx.send(Some(resource)).unwrap();
  }

  async fn init_done(&mut self) {}
}
