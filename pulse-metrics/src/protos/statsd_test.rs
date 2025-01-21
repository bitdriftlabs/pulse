// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::test::make_counter;
use anyhow::bail;
use nom::AsBytes;
use quickcheck_macros::quickcheck;

const fn clean(mut metric: Metric) -> Metric {
  metric.timestamp = 0;
  metric
}

#[test]
fn parse_statsd() {
  let valid: Vec<bytes::Bytes> = vec![
    "foo.bar:3|c".into(),
    "car:bar:3|c".into(),
    "hello.bar:4.0|ms|#tags".into(),
    "hello.bar:4.0|ms|@1.0|#tags".into(),
  ];
  for buf in valid {
    parse(&buf, false).unwrap();
  }
}

#[test]
fn lyft_tags() {
  let parsed = clean(
    parse(
      &"tests.service.Users.GetUser.__downstream_cluster=XYZ.stream.send.ms:3.0|c".into(),
      true,
    )
    .unwrap(),
  );
  assert_eq!(
    &parsed,
    make_counter(
      "tests.service.Users.GetUser.stream.send.ms",
      &[("downstream_cluster", "XYZ")],
      0,
      3.0
    )
    .metric()
  );

  let parsed = clean(
    parse(
      &"tests.service.Users.GetUser.__downstream_cluster=XYZ.codes.__hello=world.OK:3.0|c".into(),
      true,
    )
    .unwrap(),
  );
  assert_eq!(
    &parsed,
    make_counter(
      "tests.service.Users.GetUser.codes.OK",
      &[("downstream_cluster", "XYZ"), ("hello", "world")],
      0,
      3.0
    )
    .metric()
  );

  let parsed = clean(
    parse(
      &"tests.service.Users.GetUser.__downstream_cluster.stream.send.ms:3.0|c".into(),
      true,
    )
    .unwrap(),
  );
  assert_eq!(
    &parsed,
    make_counter(
      "tests.service.Users.GetUser.stream.send.ms",
      &[("downstream_cluster", "")],
      0,
      3.0
    )
    .metric()
  );

  let parsed = clean(
    parse(
      &"tests.service.Users.GetUser.__downstream_cluster=.stream.send.ms:3.0|c".into(),
      true,
    )
    .unwrap(),
  );
  assert_eq!(
    &parsed,
    make_counter(
      "tests.service.Users.GetUser.stream.send.ms",
      &[("downstream_cluster", "")],
      0,
      3.0
    )
    .metric()
  );

  let parsed = clean(parse(&"foo.car:bar.__hello=world:3.0|c".into(), true).unwrap());
  assert_eq!(
    &parsed,
    make_counter("foo.car:bar", &[("hello", "world")], 0, 3.0).metric()
  );

  let parsed = clean(parse(&"foo.car:bar.__hello=world.__bar=baz:3.0|c".into(), true).unwrap());
  assert_eq!(
    &parsed,
    make_counter("foo.car:bar", &[("bar", "baz"), ("hello", "world")], 0, 3.0).metric()
  );

  let parsed = clean(parse(&"foo.car:bar.__bar:3.0|c".into(), true).unwrap());
  assert_eq!(
    &parsed,
    make_counter("foo.car:bar", &[("bar", "")], 0, 3.0).metric()
  );

  let parsed = clean(parse(&"foo.car:bar.__bar=:3.0|c".into(), true).unwrap());
  assert_eq!(
    &parsed,
    make_counter("foo.car:bar", &[("bar", "")], 0, 3.0).metric()
  );

  assert_eq!(
    Err(ParseError::InvalidTag),
    parse(&"foo.car:bar.__:3.0|c".into(), true)
  );
}

#[test]
fn simple_line() {
  let parsed = parse(&"foo.car:bar:3.0|c".into(), false).unwrap();
  assert_eq!(parsed.get_id().name(), "foo.car:bar");
  assert_eq!(parsed.value, MetricValue::Simple(3.));
  assert_eq!(
    parsed.get_id().mtype(),
    Some(MetricType::Counter(CounterType::Delta))
  );
  assert_eq!(parsed.sample_rate, None);
}

#[test]
fn metric_types() -> anyhow::Result<()> {
  let type_checks: Vec<(bytes::Bytes, MetricType)> = vec![
    (
      "foo.bar:3|c".into(),
      MetricType::Counter(CounterType::Delta),
    ),
    ("car:bar:3|g".into(), MetricType::Gauge),
    ("car:bar:3|G".into(), MetricType::DirectGauge),
    ("hello.bar:4.0|ms|#tags".into(), MetricType::Timer),
  ];
  for (buf, expected_metric_type) in type_checks {
    println!("{}", String::from_utf8(buf.to_vec())?);
    let res = parse(&buf, false)?;
    assert_eq!(res.get_id().mtype(), Some(expected_metric_type));
  }
  Ok(())
}

#[test]
fn tagged_line() {
  let parsed = parse(&"foo.bar:3|c|@1.0|#tags".into(), false).unwrap();
  assert_eq!(parsed.get_id().name(), "foo.bar");
  assert_eq!(parsed.value, MetricValue::Simple(3.));
  assert_eq!(
    parsed.get_id().mtype(),
    Some(MetricType::Counter(CounterType::Delta))
  );
  assert_eq!(parsed.sample_rate, Some(1.));
  assert_eq!(parsed.get_id().tags()[0].tag, "tags");
  assert_eq!(parsed.get_id().tags()[0].value, "");
}

#[test]
fn tagged_line_reverse() {
  let parsed = parse(&"foo.bar:3|c|#tags|@1.0".into(), false).unwrap();
  assert_eq!(parsed.get_id().name(), "foo.bar");
  assert_eq!(parsed.value, MetricValue::Simple(3.));
  assert_eq!(
    parsed.get_id().mtype(),
    Some(MetricType::Counter(CounterType::Delta))
  );
  assert_eq!(parsed.sample_rate, Some(1.));
  assert_eq!(parsed.get_id().tags()[0].tag, "tags");
  assert_eq!(parsed.get_id().tags()[0].value, "");
}

#[test]
fn invalid_value() {
  let result = parse(&"foo.car:bar:3.x0|c".into(), false);
  assert_eq!(result.err().unwrap(), ParseError::InvalidValue);
}

#[test]
fn invalid_line() {
  let result = parse(&"foo.car:bar:3".into(), false);
  assert_eq!(result.err().unwrap(), ParseError::InvalidLine);
}

#[test]
fn test_parse_tag() {
  let tag_v: bytes::Bytes = "name:value".into();
  let r = parse_tags(tag_v).unwrap();
  assert!(r.len() == 1);
  assert_eq!(r[0].tag, "name");
  assert_eq!(r[0].value, "value");
}

#[test]
fn test_parse_tag_naked_single() {
  let tag_v: bytes::Bytes = "name".into();
  let r = parse_tags(tag_v).unwrap();
  assert_eq!(r[0].tag, "name");
  assert_eq!(r[0].value, "");
}

#[test]
fn test_parse_tag_complex_name() {
  let tag_v: bytes::Bytes = "name:value:value:value,name2:value2:value2:value2".into();
  let r = parse_tags(tag_v).unwrap();
  assert!(r.len() == 2);
  assert_eq!(r[0].tag, "name");
  assert_eq!(r[0].value, "value:value:value");
  assert_eq!(r[1].tag, "name2");
  assert_eq!(r[1].value, "value2:value2:value2");
}

#[test]
fn test_parse_tag_none() {
  let tag_v: bytes::Bytes = "".into();
  let r = parse_tags(tag_v).unwrap();
  assert!(r.is_empty());
}

#[test]
fn test_parse_tag_multiple() {
  let tag_v: bytes::Bytes = "name:value,name2:value2,name3:value3".into();
  let r = parse_tags(tag_v).unwrap();
  assert!(r.len() == 3);
  assert_eq!(r[0].tag, "name");
  assert_eq!(r[0].value, "value");
  assert_eq!(r[1].tag, "name2");
  assert_eq!(r[1].value, "value2");
  assert_eq!(r[2].tag, "name3");
  assert_eq!(r[2].value, "value3");
}

#[test]
fn test_parse_tag_multiple_short() {
  let tag_v: bytes::Bytes = "name:value,name2,name3:value3".into();
  let r = parse_tags(tag_v).unwrap();
  assert!(r.len() == 3);
  assert_eq!(r[0].tag, "name");
  assert_eq!(r[0].value, "value");
  assert_eq!(r[1].tag, "name2");
  assert_eq!(r[1].value, "");
  assert_eq!(r[2].tag, "name3");
  assert_eq!(r[2].value, "value3");
}

#[test]
fn parsed_simple() {
  let parsed = parse(&"foo.bar:3|c|#tags:value|@0.25".into(), false).unwrap();
  assert_eq!(parsed.get_id().name(), "foo.bar");
  assert_eq!(parsed.value, MetricValue::Simple(3.0));
  assert_eq!(
    parsed.get_id().mtype(),
    Some(MetricType::Counter(CounterType::Delta))
  );
  assert_eq!(parsed.sample_rate, Some(0.25));
  assert_eq!(
    parsed.get_id().tags()[0],
    TagValue {
      tag: "tags".into(),
      value: "value".into(),
    }
  );
}

#[test]
fn parsed_tags_complex() {
  let parsed = parse(&"foo.bar:3|c|#tags|tagpt2:value|@1.0".into(), false).unwrap();
  assert_eq!(parsed.get_id().name(), "foo.bar");
  assert_eq!(parsed.value, MetricValue::Simple(3.0));
  assert_eq!(
    parsed.get_id().mtype(),
    Some(MetricType::Counter(CounterType::Delta))
  );
  assert_eq!(parsed.sample_rate, Some(1.));
  assert_eq!(
    parsed.get_id().tags()[0],
    TagValue {
      tag: "tags|tagpt2".into(),
      value: "value".into(),
    }
  );
}

#[quickcheck]
fn metric_roundtrip_statsd_line(mut input: Metric) -> anyhow::Result<()> {
  let statsd_line = to_statsd_line(&input);
  let mut output = parse(&statsd_line, false)?;
  // Statsd requires a metric type. We default to counter if none is provided.
  if input.get_id().mtype().is_none() {
    let id = MetricId::new(
      input.get_id().name().clone(),
      Some(MetricType::Counter(CounterType::Delta)),
      input.get_id().tags().to_vec(),
      true,
    )
    .unwrap();
    input.set_id(id);
  }
  // Statsd wire format does not support timestamp so we omit it.
  output.timestamp = input.timestamp;
  if input != output {
    bail!(
      "input != output: input {:?}, output {:?}, line {:?}",
      input,
      output,
      String::from_utf8(statsd_line.to_vec()).unwrap(),
    );
  }
  Ok(())
}

#[test]
fn to_statsd_line_simple() {
  let metric = Metric::new(
    MetricId::new("foo.bar".into(), Some(MetricType::Timer), vec![], false).unwrap(),
    None,
    888,
    MetricValue::Simple(5.1),
  );
  let result = to_statsd_line(&metric);
  assert_eq!(result.as_bytes(), b"foo.bar:5.1|ms");
}

#[test]
fn to_statsd_line_sample_rate() {
  let metric = Metric::new(
    MetricId::new(
      "foo.bar".into(),
      Some(MetricType::Counter(CounterType::Delta)),
      vec![],
      false,
    )
    .unwrap(),
    Some(0.1),
    999,
    MetricValue::Simple(5.1),
  );
  let result = to_statsd_line(&metric);
  assert_eq!(result.as_bytes(), b"foo.bar:5.1|c|@0.1");
}

#[test]
fn to_statd_line_tags() {
  let metric = Metric::new(
    MetricId::new(
      "foo.bar".into(),
      Some(MetricType::Gauge),
      vec![
        TagValue {
          tag: "tag1".into(),
          value: "value1".into(),
        },
        TagValue {
          tag: "tag2".into(),
          value: "".into(),
        },
        TagValue {
          tag: "tag3".into(),
          value: "value3".into(),
        },
      ],
      false,
    )
    .unwrap(),
    None,
    1111,
    MetricValue::Simple(5.1),
  );
  let result = to_statsd_line(&metric);
  assert_eq!(
    result.as_bytes(),
    b"foo.bar:5.1|g|#tag1:value1,tag2,tag3:value3"
  );
}

#[test]
fn to_statd_line_tags_complex() {
  let metric = Metric::new(
    MetricId::new(
      "foo.bar".into(),
      Some(MetricType::Counter(CounterType::Delta)),
      vec![
        TagValue {
          tag: "tags|tag1".into(),
          value: "value1".into(),
        },
        TagValue {
          tag: "tag2".into(),
          value: "".into(),
        },
        TagValue {
          tag: "tag3".into(),
          value: "value3".into(),
        },
      ],
      false,
    )
    .unwrap(),
    Some(0.95),
    2222,
    MetricValue::Simple(5.1),
  );
  let result = to_statsd_line(&metric);
  assert_eq!(
    result.as_bytes(),
    b"foo.bar:5.1|c|@0.95|#tag2,tag3:value3,tags|tag1:value1"
  );
}
