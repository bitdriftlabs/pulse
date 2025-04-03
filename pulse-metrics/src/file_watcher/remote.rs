// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./remote_test.rs"]
mod remote_test;

use super::{FileWatcher, WatchError};
use crate::clients::make_tls_connector;
#[cfg(test)]
use crate::test::thread_synchronizer::ThreadSynchronizer;
use anyhow::bail;
use async_trait::async_trait;
use bd_shutdown::ComponentShutdown;
use bd_time::TimeDurationExt;
use bytes::Bytes;
use http::{Method, Request, StatusCode};
use http_body_util::{BodyExt, Empty};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use mockall::automock;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::common::bearer_token_config::Token_type;
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher::HttpFileSourceConfig;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use time::Duration;
use time::ext::NumericalDuration;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

/// A `RemoteFileWatcher` is used to track a remote resource, such as one from S3 or any HTTP
/// service. The remote source is continuously polled to check for modifications using the
/// resource's `ETag`.
struct SharedState {
  config: HttpFileSourceConfig,
  #[cfg(test)]
  thread_synchronizer: ThreadSynchronizer,
}
pub struct RemoteFileWatcher<T: RemoteFileWatcherClient + Send + Sync + 'static> {
  client: T,
  etag: String,
  file_cache_sender: Option<broadcast::Sender<(Bytes, String)>>,
  shared_state: Arc<SharedState>,
}

impl<T: RemoteFileWatcherClient + Send + Sync + 'static> RemoteFileWatcher<T> {
  // Create a new remote file watcher.
  pub async fn new(
    client: T,
    config: HttpFileSourceConfig,
    shutdown: ComponentShutdown,
  ) -> Result<(Self, Bytes), WatchError> {
    // Attempt an initial fetch. If this fails, attempt to load from cache if configured.
    // TODO(mattklein123): Depending on the size of the file, we may always want to attempt a cache
    // load first to get the server running as quickly as possible, and then do a background fetch
    // to update it.
    let (file, etag) = match client.get_file(None).await {
      Ok((file, etag)) => (file, etag),
      Err(e) => Self::maybe_load_cache_file(&config).await.ok_or(e)?,
    };
    log::debug!("loaded initial file with etag={etag}");

    // If caching is configured, create the cache task. A broadcast channel of size 1 is used
    // so if file writing gets behind we just drop older files, but only one file will be processed
    // at a time.
    let shared_state = Arc::new(SharedState {
      config,
      #[cfg(test)]
      thread_synchronizer: ThreadSynchronizer::default(),
    });
    let file_cache_sender = shared_state.config.cache_path.is_some().then(|| {
      let (sender, receiver) = broadcast::channel(1);
      let shared_state = shared_state.clone();
      tokio::spawn(async move {
        Self::write_file_to_cache_loop(receiver, shutdown, shared_state).await;
      });
      sender
    });
    // Send the initial load to the cache task if it exists. This may write back a file that was
    // just read but it's not worth handling this case.
    if let Some(file_cache_sender) = &file_cache_sender {
      log::debug!("sending initial load to cache writer");
      file_cache_sender
        .send((file.clone(), etag.clone()))
        .unwrap();
    }

    Ok((
      Self {
        client,
        etag,
        file_cache_sender,
        shared_state,
      },
      file,
    ))
  }

  // Wait to shutdown or write cache files.
  async fn write_file_to_cache_loop(
    mut receiver: broadcast::Receiver<(Bytes, String)>,
    mut shutdown: ComponentShutdown,
    shared_state: Arc<SharedState>,
  ) {
    let shutdown = shutdown.cancelled();
    tokio::pin!(shutdown);

    loop {
      #[cfg(test)]
      shared_state
        .thread_synchronizer
        .sync_point("write_file_to_cache_loop")
        .await;

      tokio::select! {
        () = &mut shutdown => {
          break;
        }
        result = receiver.recv() => {
          Self::on_recv_cache_write(&shared_state, result).await;
        }
      }
    }
    log::debug!("exiting write cache loop task");
  }

  // Called when the broadcast receiver receives a notification to write a cached file. Must handle
  // lagged errors if the writer gets behind.
  async fn on_recv_cache_write(
    shared_state: &SharedState,
    result: Result<(Bytes, String), RecvError>,
  ) {
    match result {
      Ok((file_bytes, etag)) => {
        let path = shared_state.config.cache_path.as_ref().unwrap();
        match Self::write_file_to_cache(path, &file_bytes, &etag).await {
          Ok(()) => log::info!("cached file to {path} with etag '{etag}'"),
          Err(e) => log::warn!("error caching file to {path}: {e}"),
        }
        #[cfg(test)]
        shared_state
          .thread_synchronizer
          .sync_point("wrote_file_to_cache")
          .await;
      },
      Err(RecvError::Lagged(_)) => log::debug!("ignoring lagged error"),
      Err(RecvError::Closed) => log::debug!("ignoring closed error"),
    }
  }

  // Attempt to load a file from cache.
  async fn maybe_load_cache_file(config: &HttpFileSourceConfig) -> Option<(Bytes, String)> {
    let cache_path = config.cache_path.as_ref()?;
    log::info!("initial remote fetch failed, attempting to load cached file from: {cache_path}");
    match Self::load_cache_file(cache_path).await {
      Ok((file, etag)) => Some((file, etag)),
      Err(e) => {
        log::info!("unable to load cache file: {e}");
        None
      },
    }
  }

  // Read a length prefixed vector from a file without wasted initialization.
  #[allow(clippy::uninit_vec, clippy::read_zero_byte_vec)]
  async fn read_vec(file: &mut File) -> anyhow::Result<Vec<u8>> {
    let size = file.read_u32().await?;
    let mut bytes = Vec::with_capacity(size as usize);
    unsafe {
      bytes.set_len(size as usize);
    }
    file.read_exact(&mut bytes).await?;
    Ok(bytes)
  }

  // Load a file/etag from cache, verifying the contents with a hash.
  // TODO(mattklein123): Per the below TODO, if we support memory mapping, we could do the memory
  // mapping directly here.
  async fn load_cache_file(path: &str) -> anyhow::Result<(Bytes, String)> {
    let mut file = File::open(path).await?;
    let etag = Self::read_vec(&mut file).await?;
    let etag = String::from_utf8(etag)?;
    let file_bytes = Self::read_vec(&mut file).await?.into();
    let hash = file.read_u64().await?;

    if Self::hash_file_contents(&etag, &file_bytes) != hash {
      bail!("hash mismatch for file '{path}");
    }

    Ok((file_bytes, etag))
  }

  // Hash the contents of the etag and file bytes.
  fn hash_file_contents(etag: &String, file_bytes: &Bytes) -> u64 {
    let mut hasher = xxhash_rust::xxh3::Xxh3Builder::new().build();
    etag.hash(&mut hasher);
    file_bytes.hash(&mut hasher);
    hasher.finish()
  }

  // Write the etag, file, and hash to a file.
  async fn write_file_to_cache(
    path: &str,
    file_bytes: &Bytes,
    etag: &String,
  ) -> anyhow::Result<()> {
    let mut file = File::create(path).await?;
    file.write_u32(u32::try_from(etag.len()).unwrap()).await?;
    file.write_all(etag.as_bytes()).await?;
    file
      .write_u32(u32::try_from(file_bytes.len()).unwrap())
      .await?;
    file.write_all(file_bytes).await?;
    file
      .write_u64(Self::hash_file_contents(etag, file_bytes))
      .await?;
    Ok(())
  }
}

#[async_trait]
impl<T: RemoteFileWatcherClient + Send + Sync + 'static> FileWatcher for RemoteFileWatcher<T> {
  async fn wait_until_modified(&mut self) -> Result<Bytes, WatchError> {
    loop {
      self
        .shared_state
        .config
        .interval
        .unwrap_duration_or(1.minutes())
        .sleep()
        .await;
      match self.client.get_file(Some(self.etag.clone())).await {
        Ok((new_file, new_etag)) => {
          log::debug!("new file: etag={new_etag}");
          if let Some(file_cache_sender) = &self.file_cache_sender {
            log::debug!("sending file to cache writer");
            file_cache_sender
              .send((new_file.clone(), new_etag.clone()))
              .unwrap();
          }

          self.etag = new_etag;
          return Ok(new_file);
        },
        Err(WatchError::ResourceNotModified) => {
          log::debug!("not modified");
        },
        Err(err) => {
          return Err(err);
        },
      }
    }
  }
}

#[automock]
#[async_trait]
pub trait RemoteFileWatcherClient {
  /// Fetches the specified resource. Returns [WatchError::ResourceNotModified] if the specified
  /// resource matches the given etag.
  async fn get_file(&self, etag: Option<String>) -> Result<(Bytes, String), WatchError>;
}

pub struct HttpRemoteFileWatcherClient {
  source: HttpFileSourceConfig,
  client: Client<HttpsConnector<HttpConnector>, Empty<Bytes>>,
  request_timeout: Duration,
}

impl HttpRemoteFileWatcherClient {
  #[must_use]
  pub fn new(source: HttpFileSourceConfig) -> Self {
    // TODO(mattklein123): Make connect timeout configurable.
    let client =
      Client::builder(TokioExecutor::new()).build(make_tls_connector(250.milliseconds()));
    let request_timeout = source.request_timeout.unwrap_duration_or(15.seconds());
    Self {
      source,
      client,
      request_timeout,
    }
  }
}

#[async_trait]
impl RemoteFileWatcherClient for HttpRemoteFileWatcherClient {
  async fn get_file(&self, etag: Option<String>) -> Result<(Bytes, String), WatchError> {
    let mut request = Request::builder()
      .method(Method::GET)
      .uri(self.source.url.as_str());

    if let Some(etag) = etag {
      request = request.header("If-None-Match", etag);
    }

    if let Some(bearer_token) = self.source.auth_bearer_token.clone().into_option() {
      let token = match bearer_token.token_type.expect("pgv") {
        Token_type::Token(token) => token.to_string(),
        Token_type::FilePath(file_path) => std::fs::read_to_string(file_path)?,
      };
      request = request.header("x-bitdrift-api-key", token);
    }

    let response = match self
      .request_timeout
      .timeout(self.client.request(request.body(Empty::new()).unwrap()))
      .await
    {
      Err(_) => return Err(WatchError::Timeout),
      Ok(status) => match status {
        Err(e) => return Err(e.into()),
        Ok(r) => {
          log::debug!("response with status: {}", r.status());
          if r.status() == StatusCode::NOT_MODIFIED {
            return Err(WatchError::ResourceNotModified);
          } else if !r.status().is_success() {
            return Err(WatchError::Http(r.status()));
          }
          r
        },
      },
    };

    let etag: String = response
      .headers()
      .get("ETag")
      .and_then(|etag| etag.to_str().ok().map(std::string::ToString::to_string))
      .ok_or(WatchError::ResourceMissingEtag)?;

    // TODO(mattklein123): This code used to write to a temp file, only to immediately read it back
    // into memory for use in either regex or FST building, which makes little sense. If we end up
    // with very large files, we could potentially write this to disk, and then return a memory
    // mapped file to that data.
    Ok((response.into_body().collect().await?.to_bytes(), etag))
  }
}
