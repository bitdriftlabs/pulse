// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::make_tls_connector;
use async_trait::async_trait;
use aws_config::identity::IdentityCache;
use aws_config::sts::AssumeRoleProvider;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sigv4::http_request::{
  SignableBody,
  SignableRequest,
  SigningInstructions,
  SigningSettings,
  sign,
};
use aws_sigv4::sign::v4::SigningParams;
use aws_smithy_async::rt::sleep::default_async_sleep;
use aws_smithy_async::time::SystemTimeSource;
use aws_smithy_runtime_api::client::identity::{
  ResolveCachedIdentity,
  SharedIdentityCache,
  SharedIdentityResolver,
};
use aws_smithy_runtime_api::client::runtime_components::{
  RuntimeComponents,
  RuntimeComponentsBuilder,
};
use aws_smithy_types::config_bag::ConfigBag;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use bd_time::TimeDurationExt;
use bytes::Bytes;
use http::Method;
use http_body_util::BodyExt;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use outflow_common::http_remote_write_auth_config::{Auth_type, aws_auth_config};
use outflow_common::request_header::Value_type;
use outflow_common::{HttpRemoteWriteAuthConfig, RequestHeader};
use pulse_common::proto::{CONTENT_TYPE_PROTOBUF, env_or_inline_to_string};
use pulse_protobuf::protos::pulse::config::common::v1::common::bearer_token_config::Token_type;
use pulse_protobuf::protos::pulse::config::outflow::v1::outflow_common;
use std::time::{SystemTime, UNIX_EPOCH};
use time::Duration;
use time::ext::NumericalDuration;

pub const PROM_REMOTE_WRITE_HEADERS: &[(&str, &str)] = &[
  ("content-type", CONTENT_TYPE_PROTOBUF),
  ("content-encoding", "snappy"),
  ("X-Prometheus-Remote-Write-Version", "0.1.0"),
];

#[derive(thiserror::Error, Debug)]
pub enum HttpRemoteWriteError {
  #[error("AWS error: {0}")]
  Aws(String),
  #[error("hyper client error: {0}")]
  HyperClient(#[from] hyper_util::client::legacy::Error),
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),
  #[error("response error: {0}: {1}")]
  Response(StatusCode, String),
  #[error("request timeout")]
  Timeout,
}

pub type Result<T> = std::result::Result<T, HttpRemoteWriteError>;

struct AwsAuthInner {
  sdk_config: SdkConfig,
  identity_resolver: SharedIdentityResolver,
  identity_cache: SharedIdentityCache,
  runtime_components: RuntimeComponents,
  config_bag: ConfigBag,
}

enum Auth {
  Bearer(String),
  Aws(Box<AwsAuthInner>),
}

/// A thin client wrapper used for mocking in tests
#[allow(clippy::ref_option_ref)] // Spurious
#[mockall::automock]
#[async_trait]
pub trait HttpRemoteWriteClient: Send + Sync {
  async fn send_write_request<'a>(
    &self,
    compressed_write_request: Bytes,
    extra_headers: Option<&'a HeaderMap>,
  ) -> Result<()>;
}

pub struct HyperHttpRemoteWriteClient {
  inner: Client<HttpsConnector<HttpConnector>, Body>,
  endpoint: String,
  timeout: Duration,
  auth: Option<Auth>,
  core_request_headers: Vec<(String, String)>,
  config_request_headers: Vec<RequestHeader>,
}

impl HyperHttpRemoteWriteClient {
  async fn create_auth(auth_config: Option<HttpRemoteWriteAuthConfig>) -> Result<Option<Auth>> {
    let Some(auth_config) = auth_config else {
      return Ok(None);
    };

    Ok(Some(match auth_config.auth_type.expect("pgv") {
      Auth_type::BearerToken(bearer_token) => {
        Auth::Bearer(match bearer_token.token_type.as_ref().expect("pgv") {
          Token_type::Token(token) => token.to_string(),
          Token_type::FilePath(file_path) => std::fs::read_to_string(file_path)?,
        })
      },
      Auth_type::Aws(aws_auth_type) => match aws_auth_type.auth_type.expect("pgv") {
        aws_auth_config::Auth_type::Default(config) => {
          let sdk_config = aws_config::load_defaults(BehaviorVersion::v2025_08_07()).await;
          let credentials_provider =
            sdk_config
              .credentials_provider()
              .ok_or(HttpRemoteWriteError::Aws(
                "no credentials provider configured".to_string(),
              ))?;
          let credentials_provider = if let Some(assume_role) = config.assume_role {
            SharedCredentialsProvider::new(
              AssumeRoleProvider::builder(assume_role)
                .build_from_provider(credentials_provider)
                .await,
            )
          } else {
            credentials_provider
          };

          let identity_cache = IdentityCache::lazy().build();
          // TODO(mattklein123): This is an awful hack due to the fact that AWS decided to entangle
          // the cache with the runtime. See:
          // - https://github.com/awslabs/aws-sdk-rust/discussions/923
          // - https://github.com/awslabs/aws-sdk-rust/issues/948
          let runtime_components = RuntimeComponentsBuilder::for_tests()
            .with_sleep_impl(default_async_sleep())
            .with_time_source(Some(SystemTimeSource::new()))
            .build()
            .unwrap();

          Auth::Aws(Box::new(AwsAuthInner {
            sdk_config,
            identity_resolver: SharedIdentityResolver::new(credentials_provider),
            identity_cache,
            runtime_components,
            config_bag: ConfigBag::base(),
          }))
        },
      },
    }))
  }

  pub async fn new(
    endpoint: String,
    timeout: Duration,
    auth_config: Option<HttpRemoteWriteAuthConfig>,
    core_request_headers: &[(&str, &str)],
    config_request_headers: Vec<RequestHeader>,
    pool_idle_timeout: Option<Duration>,
  ) -> Result<Self> {
    Ok(Self {
      // TODO(mattklein123): Make connect timeout configurable.
      inner: Client::builder(TokioExecutor::new())
        .pool_idle_timeout(
          pool_idle_timeout
            .unwrap_or_else(|| 90.seconds())
            .unsigned_abs(),
        )
        .build(make_tls_connector(250.milliseconds())),
      endpoint,
      timeout,
      auth: Self::create_auth(auth_config).await?,
      core_request_headers: core_request_headers
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect(),
      config_request_headers,
    })
  }

  // TODO(mattklein123): The version inside the SDK only supports http 0.x, so this code is copied.
  // Also, the query param code was removed as there is a type mismatch with Uri and we don't use
  // it. Replace this with a supported SDK function when available.
  fn apply_signature_to_request<B>(
    signing_instructions: SigningInstructions,
    request: &mut http::Request<B>,
  ) {
    let (new_headers, new_query) = signing_instructions.into_parts();
    for header in new_headers {
      let mut value = http::HeaderValue::from_str(header.value()).unwrap();
      value.set_sensitive(header.sensitive());
      request.headers_mut().insert(header.name(), value);
    }

    debug_assert!(new_query.is_empty());
  }
}

#[async_trait]
impl HttpRemoteWriteClient for HyperHttpRemoteWriteClient {
  async fn send_write_request<'a>(
    &self,
    compressed_write_request: Bytes,
    extra_headers: Option<&'a HeaderMap>,
  ) -> Result<()> {
    let mut request = Request::builder().method(Method::POST).uri(&self.endpoint);
    if let Some(Auth::Bearer(bearer_token)) = &self.auth {
      request = request.header("x-bitdrift-api-key", bearer_token);
    }
    for request_header in &self.config_request_headers {
      // TODO(mattklein123): Verify valid header names/values before we get here.
      request = match request_header.value_type.as_ref().expect("pgv") {
        Value_type::Value(value) => request.header(
          request_header.name.as_str(),
          env_or_inline_to_string(value).unwrap_or_default(),
        ),
        Value_type::Timestamp(_) => request.header(
          request_header.name.as_str(),
          SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string(),
        ),
      };
    }
    for (header_name, header_value) in &self.core_request_headers {
      request = request.header(header_name, header_value);
    }
    if let Some(extra_headers) = extra_headers {
      request.headers_mut().unwrap().extend(extra_headers.clone());
    }

    let mut request = request.body(compressed_write_request.clone()).unwrap();
    if let Some(Auth::Aws(aws_auth)) = &self.auth {
      let signing_settings = SigningSettings::default();
      let credentials = aws_auth
        .identity_cache
        .resolve_cached_identity(
          aws_auth.identity_resolver.clone(),
          &aws_auth.runtime_components,
          &aws_auth.config_bag,
        )
        .await
        .map_err(|e| HttpRemoteWriteError::Aws(format!("cannot fetch credentials: {e}")))?;
      let signing_params = SigningParams::builder()
        .identity(&credentials)
        .region(
          aws_auth
            .sdk_config
            .region()
            .ok_or(HttpRemoteWriteError::Aws(
              "no configured region".to_string(),
            ))?
            .as_ref(),
        )
        .name("aps")
        .time(SystemTime::now())
        .settings(signing_settings)
        .build()
        .unwrap();
      let signable_request = SignableRequest::new(
        request.method().as_str(),
        &self.endpoint,
        std::iter::empty(),
        SignableBody::Bytes(&compressed_write_request),
      )
      .unwrap();
      let (signing_instructions, _signature) = sign(signable_request, &signing_params.into())
        .map_err(|e| HttpRemoteWriteError::Aws(format!("cannot sign request: {e}")))?
        .into_parts();
      Self::apply_signature_to_request(signing_instructions, &mut request);
    }

    let (parts, body) = request.into_parts();
    let Ok(result) = self
      .timeout
      .timeout(self.inner.request(Request::from_parts(parts, body.into())))
      .await
    else {
      return Err(HttpRemoteWriteError::Timeout);
    };
    match result {
      Ok(r) => {
        if r.status().is_success() {
          return Ok(());
        }

        let (parts, body) = r.into_parts();
        let body = body
          .collect()
          .await
          .ok()
          .and_then(|body| String::from_utf8(body.to_bytes().to_vec()).ok())
          .unwrap_or_else(|| "unreadable body".to_string());
        Err(HttpRemoteWriteError::Response(parts.status, body))
      },
      Err(e) => Err(HttpRemoteWriteError::HyperClient(e)),
    }
  }
}

#[must_use]
pub fn should_retry(e: &HttpRemoteWriteError) -> bool {
  match e {
    HttpRemoteWriteError::Response(status, _) => {
      status.is_server_error() || *status == StatusCode::TOO_MANY_REQUESTS
    },
    // This is imperfect and will catch some Hyper errors that likely cannot ever succeed. Still,
    // it seems safer to just always retry these cases. If nothing is going to succeed (bad host,
    // whatever) the buffers will just overflow anyway.
    _ => true,
  }
}
