// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::PipelineRoute;
use anyhow::anyhow;
use protobuf::Chars;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::PipelineConfig;
use std::fmt;
use time::Duration;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum PipelineComponentType {
  Inflow,
  Processor,
  Outflow,
}

impl From<&PipelineComponentType> for &str {
  fn from(t: &PipelineComponentType) -> Self {
    match t {
      PipelineComponentType::Inflow => "inflow",
      PipelineComponentType::Processor => "processor",
      PipelineComponentType::Outflow => "outflow",
    }
  }
}

impl fmt::Display for PipelineComponentType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s: &str = self.into();
    write!(f, "{s}")
  }
}

impl From<PipelineRouteType> for PipelineComponentType {
  fn from(route_type: PipelineRouteType) -> Self {
    match route_type {
      PipelineRouteType::Processor => Self::Processor,
      PipelineRouteType::Outflow => Self::Outflow,
    }
  }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PipelineComponent {
  pub name: String,
  pub component_type: PipelineComponentType,
}

impl From<PipelineRoute> for PipelineComponent {
  fn from(route: PipelineRoute) -> Self {
    Self {
      component_type: route.inner.route_type.into(),
      name: route.inner.route_to.clone(),
    }
  }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PipelineRouteType {
  Outflow,
  Processor,
}

impl TryFrom<&str> for PipelineRouteType {
  type Error = anyhow::Error;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    match value {
      "outflow" => Ok(Self::Outflow),
      "processor" => Ok(Self::Processor),
      _ => Err(anyhow!("unknown route type: {value}")),
    }
  }
}

impl From<&PipelineRouteType> for &str {
  fn from(t: &PipelineRouteType) -> Self {
    match t {
      PipelineRouteType::Outflow => "outflow",
      PipelineRouteType::Processor => "processor",
    }
  }
}

impl fmt::Display for PipelineRouteType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let s: &str = self.into();
    write!(f, "{s}")
  }
}

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::seconds(10);

#[must_use]
pub const fn default_max_in_flight() -> u64 {
  // TODO(mattklein123): Consider pulling the default from worker/system concurrency instead.
  16
}

fn check_route(config: &PipelineConfig, route: &Chars) -> anyhow::Result<()> {
  let route = PipelineRoute::from_config_string(route)?;
  match route.inner.route_type {
    PipelineRouteType::Outflow => config
      .outflows
      .get(route.inner.route_to.as_str())
      .ok_or_else(|| anyhow!("invalid routing destination {route}"))
      .map(|_| ()),
    PipelineRouteType::Processor => config
      .processors
      .get(route.inner.route_to.as_str())
      .ok_or_else(|| anyhow!("invalid routing destination {route}"))
      .map(|_| ()),
  }
}

// TODO(mattklein123): Check for cycles.
pub fn check_routes(config: &PipelineConfig) -> anyhow::Result<()> {
  config.inflows.iter().try_for_each(|(_, inflow)| {
    inflow
      .routes
      .iter()
      .try_for_each(|r| check_route(config, r))
  })?;
  config.processors.iter().try_for_each(|(_, proc)| {
    proc
      .routes
      .iter()
      .try_for_each(|r| check_route(config, r))?;
    proc
      .alt_routes
      .iter()
      .try_for_each(|r| check_route(config, r))
  })
}
