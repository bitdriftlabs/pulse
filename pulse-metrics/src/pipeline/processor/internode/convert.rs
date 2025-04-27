// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./convert_test.rs"]
mod convert_test;

use crate::protos::metric::{
  CounterType,
  DownstreamId,
  HistogramBucket,
  HistogramData,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  SummaryBucket,
  SummaryData,
  TagValue,
};
use protobuf::EnumOrUnknown;
use pulse_protobuf::protos::pulse::internode::v1::metric::downstream_id::Id_type;
use pulse_protobuf::protos::pulse::internode::v1::metric::histogram::Bucket;
use pulse_protobuf::protos::pulse::internode::v1::metric::metric::Value_type;
use pulse_protobuf::protos::pulse::internode::v1::metric::summary::Quantile;
use pulse_protobuf::protos::pulse::internode::v1::metric::{
  BulkTimer,
  DownstreamId as ProtoDownstreamId,
  Histogram,
  Metric as ProtoMetric,
  MetricId as ProtoMetricId,
  MetricSource as ProtoMetricSource,
  MetricType as ProtoMetricType,
  Summary,
  TagValue as ProtoTagValue,
  WireProtocol,
};
use std::net::IpAddr;
use std::time::Instant;

#[derive(thiserror::Error, Debug)]
pub enum ParseError {
  #[error("unspecified enum found")]
  UnspecifiedEnum,
  #[error("invalid enum value found")]
  InvalidEnum,
  #[error("downstream ID not found")]
  MissingDownstreamId,
  #[error("original line not found")]
  MissingOriginalLine,
  #[error("metric source field not found")]
  MissingMetricSource,
  #[error("metric id field not found")]
  MissingMetricId,
}

impl From<MetricType> for ProtoMetricType {
  fn from(t: MetricType) -> Self {
    match t {
      MetricType::Counter(CounterType::Delta) => Self::METRIC_TYPE_DELTA_COUNTER,
      MetricType::Counter(CounterType::Absolute) => Self::METRIC_TYPE_ABSOLUTE_COUNTER,
      MetricType::DeltaGauge => Self::METRIC_TYPE_DELTA_GAUGE,
      MetricType::DirectGauge => Self::METRIC_TYPE_DIRECT_GAUGE,
      MetricType::Gauge => Self::METRIC_TYPE_GAUGE,
      MetricType::Histogram => Self::METRIC_TYPE_HISTOGRAM,
      MetricType::Summary => Self::METRIC_TYPE_SUMMARY,
      MetricType::Timer => Self::METRIC_TYPE_TIMER,
      MetricType::BulkTimer => Self::METRIC_TYPE_BULK_TIMER,
    }
  }
}

impl TryFrom<EnumOrUnknown<ProtoMetricType>> for MetricType {
  type Error = ParseError;

  fn try_from(t: EnumOrUnknown<ProtoMetricType>) -> Result<Self, Self::Error> {
    match t.enum_value_or(ProtoMetricType::METRIC_TYPE_UNSPECIFIED) {
      ProtoMetricType::METRIC_TYPE_UNSPECIFIED => Err(ParseError::UnspecifiedEnum),
      ProtoMetricType::METRIC_TYPE_DELTA_COUNTER => Ok(Self::Counter(CounterType::Delta)),
      ProtoMetricType::METRIC_TYPE_ABSOLUTE_COUNTER => Ok(Self::Counter(CounterType::Absolute)),
      ProtoMetricType::METRIC_TYPE_DELTA_GAUGE => Ok(Self::DeltaGauge),
      ProtoMetricType::METRIC_TYPE_DIRECT_GAUGE => Ok(Self::DirectGauge),
      ProtoMetricType::METRIC_TYPE_GAUGE => Ok(Self::Gauge),
      ProtoMetricType::METRIC_TYPE_HISTOGRAM => Ok(Self::Histogram),
      ProtoMetricType::METRIC_TYPE_SUMMARY => Ok(Self::Summary),
      ProtoMetricType::METRIC_TYPE_TIMER => Ok(Self::Timer),
      ProtoMetricType::METRIC_TYPE_BULK_TIMER => Ok(Self::BulkTimer),
    }
  }
}

impl TryFrom<ProtoMetricSource> for MetricSource {
  type Error = ParseError;

  fn try_from(source: ProtoMetricSource) -> Result<Self, Self::Error> {
    match source
      .wire_protocol
      .enum_value_or(WireProtocol::WIRE_PROTOCOL_UNSPECIFIED)
    {
      WireProtocol::WIRE_PROTOCOL_UNSPECIFIED => Err(ParseError::UnspecifiedEnum),
      WireProtocol::WIRE_PROTOCOL_CARBON => source.original.map_or_else(
        || Err(ParseError::MissingOriginalLine),
        |o| Ok(Self::Carbon(o)),
      ),
      WireProtocol::WIRE_PROTOCOL_STATSD => source.original.map_or_else(
        || Err(ParseError::MissingOriginalLine),
        |o| Ok(Self::Statsd(o)),
      ),
      WireProtocol::WIRE_PROTOCOL_PROM => Ok(Self::PromRemoteWrite),
      WireProtocol::WIRE_PROTOCOL_OTLP => Ok(Self::Otlp),
    }
  }
}

impl From<&MetricSource> for ProtoMetricSource {
  fn from(source: &MetricSource) -> Self {
    match source {
      MetricSource::Carbon(original) => Self {
        wire_protocol: WireProtocol::WIRE_PROTOCOL_CARBON.into(),
        original: Some(original.clone()),
        ..Default::default()
      },
      MetricSource::Statsd(original) => Self {
        wire_protocol: WireProtocol::WIRE_PROTOCOL_STATSD.into(),
        original: Some(original.clone()),
        ..Default::default()
      },
      MetricSource::PromRemoteWrite => Self {
        wire_protocol: WireProtocol::WIRE_PROTOCOL_PROM.into(),
        original: None,
        ..Default::default()
      },
      MetricSource::Otlp => Self {
        wire_protocol: WireProtocol::WIRE_PROTOCOL_OTLP.into(),
        original: None,
        ..Default::default()
      },
      MetricSource::Aggregation { .. } => {
        // An aggregated metric should never be sent over internode.
        // TODO(mattklein123): Verify this in the configuration route check pass.
        unreachable!()
      },
    }
  }
}

fn downstream_id_to_proto_downstream_id(downstream_id: &DownstreamId) -> Id_type {
  match downstream_id {
    DownstreamId::LocalOrigin => Id_type::LocalOrigin(true),
    DownstreamId::UnixDomainSocket(name) => Id_type::UnixDomainSocket(name.clone()),
    DownstreamId::IpAddress(IpAddr::V4(addr)) => Id_type::Ipv4Address((*addr).into()),
    DownstreamId::IpAddress(IpAddr::V6(addr)) => {
      Id_type::Ipv6Address(addr.octets().to_vec().into())
    },
    DownstreamId::InflowProvided(inflow_provided) => {
      Id_type::InflowProvided(inflow_provided.clone())
    },
  }
}

impl From<&ParsedMetric> for ProtoMetric {
  fn from(m: &ParsedMetric) -> Self {
    Self {
      id: Some(ProtoMetricId {
        name: m.metric().get_id().name().clone(),
        metric_type: m
          .metric()
          .get_id()
          .mtype()
          .map(|t| ProtoMetricType::from(t).into()),
        tag_values: m
          .metric()
          .get_id()
          .tags()
          .iter()
          .map(|t| ProtoTagValue {
            tag: t.tag.clone(),
            value: t.value.clone(),
            ..Default::default()
          })
          .collect(),
        ..Default::default()
      })
      .into(),
      sample_rate: m.metric().sample_rate,
      timestamp: m.metric().timestamp,
      value_type: Some(match &m.metric().value {
        MetricValue::Simple(value) => Value_type::SimpleValue(*value),
        MetricValue::Histogram(histogram) => Value_type::Histogram(Histogram {
          buckets: histogram
            .buckets
            .iter()
            .map(|bucket| Bucket {
              count: bucket.count,
              le: bucket.le,
              ..Default::default()
            })
            .collect(),
          sample_count: histogram.sample_count,
          sample_sum: histogram.sample_sum,
          ..Default::default()
        }),
        MetricValue::Summary(summary) => Value_type::Summary(Summary {
          quantiles: summary
            .quantiles
            .iter()
            .map(|bucket| Quantile {
              quantile: bucket.quantile,
              value: bucket.value,
              ..Default::default()
            })
            .collect(),
          sample_count: summary.sample_count,
          sample_sum: summary.sample_sum,
          ..Default::default()
        }),
        MetricValue::BulkTimer(values) => Value_type::BulkTimer(BulkTimer {
          values: values.clone(),
          ..Default::default()
        }),
      }),
      metric_source: Some(m.source().into()).into(),
      received_at: 0, // TODO(mattklein123): Support received_at.
      downstream_id: Some(ProtoDownstreamId {
        id_type: Some(downstream_id_to_proto_downstream_id(m.downstream_id())),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    }
  }
}

impl TryFrom<ProtoMetricId> for MetricId {
  type Error = ParseError;

  fn try_from(id: ProtoMetricId) -> Result<Self, Self::Error> {
    let mtype: Option<MetricType> = match id.metric_type {
      Some(t) => Some(MetricType::try_from(t)?),
      None => None,
    };
    Ok(
      Self::new(
        id.name,
        mtype,
        id.tag_values
          .into_iter()
          .map(|tv| TagValue {
            tag: tv.tag,
            value: tv.value,
          })
          .collect(),
        true, // Assume sorted already in the other process.
      )
      .expect("validated by other node"),
    )
  }
}

pub fn proto_metric_to_parsed_metric(m: ProtoMetric) -> Result<ParsedMetric, ParseError> {
  let source: MetricSource = m.metric_source.into_option().map_or(
    Err(ParseError::MissingMetricSource),
    std::convert::TryInto::try_into,
  )?;

  let id: MetricId = m.id.into_option().map_or(
    Err(ParseError::MissingMetricId),
    std::convert::TryInto::try_into,
  )?;

  Ok(ParsedMetric::new(
    Metric::new(
      id,
      m.sample_rate,
      m.timestamp,
      match m.value_type.ok_or(ParseError::UnspecifiedEnum)? {
        Value_type::SimpleValue(value) => MetricValue::Simple(value),
        Value_type::Histogram(histogram) => MetricValue::Histogram(HistogramData {
          buckets: histogram
            .buckets
            .into_iter()
            .map(|b| HistogramBucket {
              le: b.le,
              count: b.count,
            })
            .collect(),
          sample_count: histogram.sample_count,
          sample_sum: histogram.sample_sum,
        }),
        Value_type::Summary(summary) => MetricValue::Summary(SummaryData {
          quantiles: summary
            .quantiles
            .into_iter()
            .map(|q| SummaryBucket {
              quantile: q.quantile,
              value: q.value,
            })
            .collect(),
          sample_count: summary.sample_count,
          sample_sum: summary.sample_sum,
        }),
        Value_type::BulkTimer(bulk_timer) => MetricValue::BulkTimer(bulk_timer.values),
      },
    ),
    source,
    Instant::now(), // TODO(mattklein123): Support received_at.
    match m
      .downstream_id
      .into_option()
      .ok_or(ParseError::MissingDownstreamId)?
      .id_type
      .ok_or(ParseError::UnspecifiedEnum)?
    {
      Id_type::LocalOrigin(_) => DownstreamId::LocalOrigin,
      Id_type::UnixDomainSocket(name) => DownstreamId::UnixDomainSocket(name),
      Id_type::Ipv4Address(address) => DownstreamId::IpAddress(IpAddr::V4(address.into())),
      Id_type::Ipv6Address(address) => {
        let address: &[u8; 16] = address
          .get(0 .. 16)
          .ok_or(ParseError::MissingDownstreamId)?
          .try_into()
          .unwrap();
        DownstreamId::IpAddress(IpAddr::V6((*address).into()))
      },
      Id_type::InflowProvided(inflow_provided) => DownstreamId::InflowProvided(inflow_provided),
    },
  ))
}
