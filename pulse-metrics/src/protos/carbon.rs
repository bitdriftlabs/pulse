// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./carbon_test.rs"]
mod carbon_test;

use super::metric::{unwrap_timestamp, Metric, MetricId, MetricValue, TagValue};
use crate::protos::metric;
use nom::branch::alt;
use nom::bytes::complete::{escaped, is_not, tag, take_while1};
use nom::character::complete::digit1;
use nom::character::is_space;
use nom::combinator::{map_res, opt};
use nom::error::ParseError;
use nom::multi::many1;
use nom::number::complete::double;
use nom::sequence::{delimited, pair, tuple};
use nom::IResult;
use std::convert::Infallible;
use std::str::Utf8Error;
use thiserror;

/// Return a parser which:
/// Captures an escaped string, single (or double) quoted, with any character including an escaped
/// \' (or \"), until the final ' (or ") Contents of the string are not escape-decoded, keeping the
/// literal \' (or \") character inside the string.
fn escaped_string<'a, E: ParseError<&'a [u8]>>(
  double_quoted: bool,
) -> impl FnMut(&'a [u8]) -> IResult<&[u8], &[u8], E> {
  let (quote, not_chars) = if double_quoted {
    (b"\"", "\\\"")
  } else {
    (b"'", "\\'")
  };
  delimited(
    tag(quote),
    escaped(is_not(not_chars), '\\', tag(quote)),
    tag(quote),
  )
}

fn non_escaped_string<'a, E: ParseError<&'a [u8]>>(
) -> impl FnMut(&'a [u8]) -> IResult<&[u8], &[u8], E> {
  take_while1(|c: u8| {
    c.is_ascii_lowercase()
      || c.is_ascii_uppercase()
      || c.is_ascii_digit()
      || c == b'-'
      || c == b'_'
      || c == b'.'
  })
}

fn string<'a, E: ParseError<&'a [u8]>>() -> impl FnMut(&'a [u8]) -> IResult<&[u8], &[u8], E> {
  alt((
    escaped_string(false),
    escaped_string(true),
    non_escaped_string(),
  ))
}

#[allow(clippy::type_complexity)]
fn named_tag<
  'a,
  E: ParseError<&'a [u8]> + nom::error::FromExternalError<&'a [u8], std::convert::Infallible>,
>(
  input: &'a bytes::Bytes,
) -> impl FnMut(&'a [u8]) -> IResult<&[u8], TagValue, E> {
  map_res(
    tuple((string(), tag(b"="), string(), opt(take_while1(is_space)))),
    #[allow(clippy::type_complexity)]
    |t: (&[u8], &[u8], &[u8], Option<&[u8]>)| -> Result<TagValue, Infallible> {
      Ok(TagValue {
        tag: input.slice_ref(t.0),
        value: input.slice_ref(t.2),
      })
    },
  )
}

#[derive(thiserror::Error, Debug)]
enum TimestampError {
  #[error("Invalid character")]
  InvalidUtf8(Utf8Error),
  #[error("Invalid digits")]
  InvalidNumber(std::num::ParseIntError),
}

fn parse_timestamp(input: &[u8]) -> Result<u64, TimestampError> {
  let s = std::str::from_utf8(input).map_err(TimestampError::InvalidUtf8)?;
  s.parse().map_err(TimestampError::InvalidNumber)
}

/// Parse a complete carbon line, returning a structure with all the fields
/// populated, including tags, timestamp and underlying value
fn parse_carbon_line(input: &bytes::Bytes) -> Result<(&[u8], Metric), metric::ParseError> {
  let bytes: &[u8] = input;
  let (bytes, (metric, _, value, timestamp, tags)) = tuple((
    string::<nom::error::Error<_>>(),
    take_while1(is_space),
    double,
    opt(pair(
      take_while1(is_space),
      map_res(digit1, parse_timestamp), // Timestamp is technically optional
    )),
    opt(pair(take_while1(is_space), many1(named_tag(input)))),
  ))(bytes)
  .map_err(|_| metric::ParseError::Generic)?;
  let tags = tags.map(|(_, tags)| tags).unwrap_or_default();
  let id = MetricId::new(input.slice_ref(metric), None, tags, false)?;

  Ok((
    bytes,
    Metric::new(
      id,
      None,
      unwrap_timestamp(timestamp.map(|c| c.1)),
      MetricValue::Simple(value),
    ),
  ))
}

pub fn parse(input: &bytes::Bytes) -> Result<Metric, metric::ParseError> {
  let (_, metric) = parse_carbon_line(input)?;
  Ok(metric)
}

fn write_carbon_tag(line: &mut bytes::BytesMut, tag: &TagValue) {
  line.extend_from_slice(b" \"");
  line.extend_from_slice(tag.tag.as_ref());
  line.extend_from_slice(b"\"=\"");
  line.extend_from_slice(tag.value.as_ref());
  line.extend_from_slice(b"\"");
}

pub fn to_carbon_line(metric: &Metric) -> bytes::Bytes {
  // TODO(mattklein123): Histograms/summaries are blocked at the wire outflow level.
  let value = metric.value.to_simple();

  let mut line = bytes::BytesMut::new();
  line.extend_from_slice(b"\"");
  line.extend_from_slice(metric.get_id().name().as_ref());
  line.extend_from_slice(b"\" ");
  line.extend_from_slice(value.to_string().as_bytes());
  line.extend_from_slice(b" ");
  line.extend_from_slice(metric.timestamp.to_string().as_bytes());
  // "source" (or "host") tag must come first.
  if let Some(tag) = metric
    .get_id()
    .tags()
    .iter()
    .find(|tag| tag.tag.as_ref() == b"source")
  {
    write_carbon_tag(&mut line, tag);
  }
  if let Some(tag) = metric
    .get_id()
    .tags()
    .iter()
    .find(|tag| tag.tag.as_ref() == b"host")
  {
    write_carbon_tag(&mut line, tag);
  }
  for tag in metric.get_id().tags() {
    if tag.tag.as_ref() != b"host" && tag.tag.as_ref() != b"source" {
      write_carbon_tag(&mut line, tag);
    }
  }
  line.freeze()
}
