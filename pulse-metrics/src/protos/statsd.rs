// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./statsd_test.rs"]
mod statsd_test;

use super::metric::{
  CounterType,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  ParseError,
  TagValue,
  default_timestamp,
};
use memchr::{memchr, memchr2, memmem, memrchr};
use std::vec;

fn parse_tags(input: bytes::Bytes) -> Result<Vec<TagValue>, ParseError> {
  if input.is_empty() {
    return Ok(vec![]);
  }

  let mut tags: Vec<TagValue> = Vec::new();
  let mut scan = input;
  loop {
    let tag_index_end = memchr(b',', scan.as_ref()).map_or_else(|| scan.len(), |i| i);
    let tag_scan = scan.slice(0 .. tag_index_end);
    match memchr(b':', tag_scan.as_ref()) {
      // Value-less tag, consume the name and continue
      None => tags.push(TagValue {
        tag: tag_scan,
        value: "".into(),
      }),
      Some(value_start) => tags.push(TagValue {
        tag: tag_scan.slice(0 .. value_start),
        value: tag_scan.slice(value_start + 1 ..),
      }),
    }
    if tag_index_end == scan.len() {
      return Ok(tags);
    }
    scan = scan.slice(tag_index_end + 1 ..);
  }
}

/// Parse an incoming single protocol unit and capture internal field
/// offsets for the positions and lengths of various protocol fields for
/// later access. No parsing or validation of values is done, so at a low
/// level this can be used to pass through unknown types and protocols.
pub fn parse(input: &bytes::Bytes, parse_lyft_tags: bool) -> Result<Metric, ParseError> {
  let length = input.len();

  // To support inner ':' symbols in a metric name (more common than you
  // think) we'll first find the index of the first type separator, and
  // then do a walk to find the last ':' symbol before that.
  let type_index = memchr(b'|', input.as_ref()).ok_or(ParseError::InvalidLine)? + 1;
  let value_index = memrchr(b':', &input[0 .. type_index]).ok_or(ParseError::InvalidType)? + 1;

  let mut type_index_end = length;
  let mut sample_rate_index: Option<(usize, usize)> = None;
  let mut tags_index: Option<(usize, usize)> = None;

  let mut scan_index = type_index;
  loop {
    let index = memchr(b'|', &input[scan_index ..]).map(|v| v + scan_index);
    match index {
      None => break,
      Some(x) if x + 2 >= length => break,
      Some(x) if x < type_index_end => type_index_end = x,
      _ => (),
    }
    match input[index.unwrap() + 1] {
      b'@' => {
        if sample_rate_index.is_some() {
          return Err(ParseError::RepeatedSampleRate);
        }
        sample_rate_index = index.map(|v| (v + 2, length));
        tags_index = tags_index.map(|(v, _l)| (v, index.unwrap()));
      },
      b'#' => {
        if tags_index.is_some() {
          return Err(ParseError::RepeatedTags);
        }
        tags_index = index.map(|v| (v + 2, length));
        sample_rate_index = sample_rate_index.map(|(v, _l)| (v, index.unwrap()));
      },
      _ => (),
    }
    scan_index = index.unwrap() + 1;
  }

  let mtype: Option<MetricType> = match (
    MetricType::from_statsd(&input[type_index .. type_index_end]).ok(),
    input[value_index],
  ) {
    (Some(MetricType::Gauge), b'-' | b'+') => Some(MetricType::DeltaGauge),
    (t, _) => t,
  };
  let sample_rate: Option<f64> = sample_rate_index
    .map(|(start, end)| {
      std::str::from_utf8(&input[start .. end])
        .map_err(|_| ParseError::InvalidValue)?
        .parse::<f64>()
        .map_err(|_| ParseError::InvalidValue)
    })
    .transpose()?;
  let mut tags = tags_index
    .map(|(start, end)| parse_tags(input.slice(start .. end)))
    .transpose()?
    .unwrap_or_default();
  let mut name = input.slice(0 .. value_index - 1);

  if parse_lyft_tags {
    // Search backwards in the name for ".__" and attempt to extract a tag from that, adjusting
    // the name when done.
    while let Some(tag_index) = memmem::rfind(&name, b".__") {
      let tag_index = tag_index + 3;
      if tag_index == name.len() {
        return Err(ParseError::InvalidTag);
      }

      let tag_slice = name.slice(tag_index ..);
      let mut need_name_truncate = true;

      // Unfortunately there are degenerate cases where the .__ tag is getting inserted in the
      // middle of the dot delimited name. The callers should be fixed but this is not possible.
      // So we need to handle this case. Thus, we need to check for both = and . in the tag.
      if let Some(equal_or_dot_index) = memchr2(b'=', b'.', &tag_slice) {
        if tag_slice[equal_or_dot_index] == b'=' {
          // Look for an embedded '.' index, otherwise default to the end of the tag.
          let extra_index = memchr(b'.', &tag_slice[equal_or_dot_index ..])
            .map_or(tag_slice.len(), |i| i + equal_or_dot_index);

          let tag = tag_slice.slice(.. equal_or_dot_index);
          let value = tag_slice.slice(equal_or_dot_index + 1 .. extra_index);
          tags.push(TagValue { tag, value });

          // In this case we need to reallocate name and copy the extra part after the extraction.
          if extra_index != tag_slice.len() {
            let new_size = name.len() - extra_index - 3;
            let mut new_name = Vec::with_capacity(new_size);
            new_name.extend_from_slice(&name[.. tag_index - 3]);
            new_name.extend_from_slice(&tag_slice[extra_index ..]);
            debug_assert_eq!(new_name.len(), new_size);
            name = new_name.into();
            need_name_truncate = false;
          }
        } else {
          // In this case the tag has an empty value but we still need to handle copying the
          // extra part after extracting the name.
          tags.push(TagValue {
            tag: tag_slice.slice(.. equal_or_dot_index),
            value: "".into(),
          });
          let new_size = name.len() - equal_or_dot_index - 3;
          let mut new_name = Vec::with_capacity(new_size);
          new_name.extend_from_slice(&name[.. tag_index - 3]);
          new_name.extend_from_slice(&tag_slice[equal_or_dot_index ..]);
          debug_assert_eq!(new_name.len(), new_size);
          name = new_name.into();
          need_name_truncate = false;
        }
      } else {
        tags.push(TagValue {
          tag: tag_slice,
          value: "".into(),
        });
      }
      if need_name_truncate {
        name.truncate(tag_index - 3);
      }
    }
  }

  let id = MetricId::new(name, mtype, tags, false)?;
  let value_str = std::str::from_utf8(&input[value_index .. type_index - 1])
    .map_err(|_| ParseError::InvalidValue)?;
  let value = value_str
    .parse::<f64>()
    .map_err(|_| ParseError::InvalidValue)?;
  // statsd doesn't support timestamps, so we just assign now.
  Ok(Metric::new(
    id,
    sample_rate,
    default_timestamp(),
    MetricValue::Simple(value),
  ))
}

pub fn to_statsd_line(metric: &Metric) -> bytes::Bytes {
  // TODO(mattklein123): Histograms/summaries are blocked at the wire outflow level.
  let value = metric.value.to_simple();

  let mtype = metric
    .get_id()
    .mtype()
    .unwrap_or(MetricType::Counter(CounterType::Delta));
  let mut line = bytes::BytesMut::new();
  line.extend_from_slice(metric.get_id().name().as_ref());
  line.extend_from_slice(b":");
  if mtype == MetricType::DeltaGauge && value.is_sign_positive() {
    line.extend_from_slice(b"+");
  }
  line.extend_from_slice(value.to_string().as_bytes());
  line.extend_from_slice(b"|");
  line.extend_from_slice(mtype.to_statsd());
  if let Some(sample_rate) = metric.sample_rate {
    line.extend_from_slice(b"|@");
    line.extend_from_slice(sample_rate.to_string().as_bytes());
  }
  if !metric.get_id().tags().is_empty() {
    line.extend_from_slice(b"|#");
    let it = &mut metric.get_id().tags().iter().peekable();
    while let Some(tag) = it.next() {
      line.extend_from_slice(tag.tag.as_ref());
      if !tag.value.is_empty() {
        line.extend_from_slice(b":");
        line.extend_from_slice(tag.value.as_ref());
      }
      if it.peek().is_some() {
        line.extend_from_slice(b",");
      }
    }
  }
  line.freeze()
}
