// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use anyhow::bail;
use matches::assert_matches;
use nom::AsBytes;
use quickcheck_macros::quickcheck;
use time::OffsetDateTime;

/// Parse an escaped string, returning the actual parsed state
fn parse_escaped_string(s: &[u8], double_quoted: bool) -> IResult<&[u8], &[u8]> {
  escaped_string(double_quoted).parse(s)
}

/// Parse a string, returning the actual parsed state
fn parse_string(s: &[u8]) -> IResult<&[u8], &[u8]> {
  string().parse(s)
}

/// Parse a named tag, consisting of a "a"="b" set, where a and b are escaped string capable.
fn parse_named_tag(s: &'static [u8]) -> IResult<&'static [u8], TagValue> {
  map_res(
    (string(), tag(&b"="[..]), string(), opt(tag(&b" "[..]))),
    #[allow(clippy::type_complexity)]
    |t: (&[u8], &[u8], &[u8], Option<&[u8]>)| -> Result<TagValue, Infallible> {
      Ok(TagValue {
        tag: t.0.into(),
        value: t.2.into(),
      })
    },
  )
  .parse(s)
}

#[test]
fn check_doubled_quoted_esc() {
  assert_eq!(
    parse_escaped_string(br#""hello world""#, true),
    Ok((b"".as_ref(), b"hello world".as_ref()))
  );
  assert_eq!(
    parse_escaped_string(br#""hello \"world""#, true),
    Ok((b"".as_ref(), b"hello \\\"world".as_ref()))
  );
  assert_matches!(parse_escaped_string(br#""hello \"world'"#, true), Err(_));
}

#[test]
fn check_single_quoted_esc() {
  assert_eq!(
    parse_escaped_string(br"'hello world'", false),
    Ok((b"".as_ref(), b"hello world".as_ref()))
  );
  assert_eq!(
    parse_escaped_string(br"'hello \'world'", false),
    Ok((b"".as_ref(), b"hello \\\'world".as_ref()))
  );
  assert_matches!(parse_escaped_string(br#"'hello \'world""#, false), Err(_));
}

#[test]
fn test_string() {
  assert_eq!(
    parse_string(br#""hello world""#),
    Ok((b"".as_ref(), b"hello world".as_ref()))
  );
  assert_eq!(
    parse_string(br#""hello \"world""#),
    Ok((b"".as_ref(), b"hello \\\"world".as_ref()))
  );
  assert_eq!(
    parse_string(br"'hello world'"),
    Ok((b"".as_ref(), b"hello world".as_ref()))
  );
  assert_eq!(
    parse_string(br"'hello \'world'"),
    Ok((b"".as_ref(), b"hello \\\'world".as_ref()))
  );
  assert_eq!(
    parse_string(b"hello"),
    Ok((b"".as_ref(), b"hello".as_ref()))
  );
}

#[test]
fn check_named_tag() {
  let tag1 = br#""name"="value""#;
  let result = TagValue {
    tag: "name".into(),
    value: "value".into(),
  };
  assert_eq!(parse_named_tag(tag1), Ok((b"".as_ref(), result.clone(),)));
  let tag2 = br#"name="value""#;
  assert_eq!(parse_named_tag(tag2), Ok((b"".as_ref(), result,)));
}

#[test]
fn test_parse_carbon() {
  let line: bytes::Bytes = "hello.world 4.3 1234 source=a".into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "hello.world".into(),
      None,
      vec![TagValue {
        tag: "source".into(),
        value: "a".into(),
      }],
      false,
    )
    .unwrap(),
    None,
    1234,
    MetricValue::Simple(4.3),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_no_timestamp() {
  let line: bytes::Bytes = "hello.world 4.3  source=a".into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "hello.world".into(),
      None,
      vec![TagValue {
        tag: "source".into(),
        value: "a".into(),
      }],
      false,
    )
    .unwrap(),
    None,
    result.1.timestamp,
    MetricValue::Simple(4.3),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_special_characters() {
  let line: bytes::Bytes = r#"new-york.power.usage 42422 123456 source=local-_-host.edu data-center_.="dc1 and dc2!@#$/%^&*()""#.into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "new-york.power.usage".into(),
      None,
      vec![
        TagValue {
          tag: "data-center_.".into(),
          value: "dc1 and dc2!@#$/%^&*()".into(),
        },
        TagValue {
          tag: "source".into(),
          value: "local-_-host.edu".into(),
        },
      ],
      false,
    )
    .unwrap(),
    None,
    123_456,
    MetricValue::Simple(42422.0),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_negative_value() {
  let line: bytes::Bytes = "hello.world -50.12 123 source=server-0".into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "hello.world".into(),
      None,
      vec![TagValue {
        tag: "source".into(),
        value: "server-0".into(),
      }],
      false,
    )
    .unwrap(),
    None,
    123,
    MetricValue::Simple(-50.12),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_negative_scientific_notation() {
  let line: bytes::Bytes = "cpu0.loadavg.1m 1E-2 123 source=hello".into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "cpu0.loadavg.1m".into(),
      None,
      vec![TagValue {
        tag: "source".into(),
        value: "hello".into(),
      }],
      false,
    )
    .unwrap(),
    None,
    123,
    MetricValue::Simple(0.01),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_single_quotes() {
  let line: bytes::Bytes = "\"sca.logstash.pipelines.sca.plugins.outputs.solas.events.out\" \
                            208140 1660557239 host='s-2m210901pw.sys.az1.eng.pdx.wd' \
                            cname='s-2m210901pw.sys.az1.eng.pdx.wd'"
    .into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "sca.logstash.pipelines.sca.plugins.outputs.solas.events.out".into(),
      None,
      vec![
        TagValue {
          tag: "cname".into(),
          value: "s-2m210901pw.sys.az1.eng.pdx.wd".into(),
        },
        TagValue {
          tag: "host".into(),
          value: "s-2m210901pw.sys.az1.eng.pdx.wd".into(),
        },
      ],
      false,
    )
    .unwrap(),
    None,
    1_660_557_239,
    MetricValue::Simple(208_140.0),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn test_parse_carbon_missing_value() {
  let line = "system.cpu.loadavg source=server-0";
  parse_carbon_line(&line.into()).unwrap_err();
}

#[test]
fn test_parse_carbon_invalid_character() {
  let line = r"system.cpu.load\# 0.03 source=server-0";
  parse_carbon_line(&line.into()).unwrap_err();
}

#[test]
fn test_complex_lines() {
  let line: bytes::Bytes = r#""oxs.datasync.subscription.changesetclient.maxsentforconsumetxnidvalue" 3.8684168E7 1638906732 source="s-mxq70102gq.sys.az1.cust.atl.wd" "servicetype"="ots" "wd_env_status"="active" "instance"="wd2az1sbpvwots1032b" "_additional_source"="proxy::dmz0041.svc.nprd.atl.wd" "origin"="prometheus_oxs" "cname"="sbpvwots1032.impl.env.az1.cust.atl.wd" "env"="impl" "metric_protocol"="prometheus" "tenant"="REDACTED""#.into();
  let result = parse_carbon_line(&line).expect("no error");
  let metric = Metric::new(
    MetricId::new(
      "oxs.datasync.subscription.changesetclient.maxsentforconsumetxnidvalue".into(),
      None,
      vec![
        TagValue {
          tag: "_additional_source".into(),
          value: "proxy::dmz0041.svc.nprd.atl.wd".into(),
        },
        TagValue {
          tag: "cname".into(),
          value: "sbpvwots1032.impl.env.az1.cust.atl.wd".into(),
        },
        TagValue {
          tag: "env".into(),
          value: "impl".into(),
        },
        TagValue {
          tag: "instance".into(),
          value: "wd2az1sbpvwots1032b".into(),
        },
        TagValue {
          tag: "metric_protocol".into(),
          value: "prometheus".into(),
        },
        TagValue {
          tag: "origin".into(),
          value: "prometheus_oxs".into(),
        },
        TagValue {
          tag: "servicetype".into(),
          value: "ots".into(),
        },
        TagValue {
          tag: "source".into(),
          value: "s-mxq70102gq.sys.az1.cust.atl.wd".into(),
        },
        TagValue {
          tag: "tenant".into(),
          value: "REDACTED".into(),
        },
        TagValue {
          tag: "wd_env_status".into(),
          value: "active".into(),
        },
      ],
      false,
    )
    .unwrap(),
    None,
    1_638_906_732,
    MetricValue::Simple(3.868_416_8E7),
  );
  assert_eq!(result, (b"".as_ref(), metric));
}

#[test]
fn format_metric() {
  let carbon = "hello 1.2 1 a=b";
  let result = parse(&carbon.into()).expect("valid carbon line");
  let f = result.to_string();
  assert_eq!("hello([a=b])[VALUE=1.2][TIMESTAMP=1]", f);
}

#[test]
fn format_metric_nots() {
  let carbon = "hello 1.2 a=b";
  let result = parse(&carbon.into()).expect("valid carbon line");
  let f = result.to_string();
  let expected = format!("hello([a=b])[VALUE=1.2][TIMESTAMP={}]", result.timestamp);
  assert_eq!(expected, f);
}

#[test]
fn metric_to_invaliddatetime() {
  let carbon = "hello 1.2 446744073709551614 a=b";
  let result = parse(&carbon.into()).expect("valid carbon line");
  let dt = result.to_datetime();
  assert!(dt.is_none());
}

#[test]
fn metric_to_datetime() {
  let carbon = "hello 1.2 1655870400 a=b";
  let result = parse(&carbon.into()).expect("valid carbon line");
  let dt = result.to_datetime();
  assert!(dt.is_some());
  let ntd = OffsetDateTime::from_unix_timestamp(1_655_870_400).unwrap();
  assert_eq!(Some(ntd), dt);
}

#[test]
fn metric_to_datetime_ms() {
  let carbon = "hello 1.2 1655870400000 a=b";
  let result = parse(&carbon.into()).expect("valid carbon line");
  let dt = result.to_datetime();
  assert!(dt.is_some());
  let ntd = OffsetDateTime::from_unix_timestamp(1_655_870_400).unwrap();
  assert_eq!(Some(ntd), dt);
}

#[quickcheck]
fn metric_roundtrip_carbon_line(mut input: Metric) -> anyhow::Result<()> {
  // Carbon wire format does not support metric types or sample rates, so we omit them.
  let id = MetricId::new(
    input.get_id().name().clone(),
    None,
    input.get_id().tags().to_vec(),
    true,
  )
  .unwrap();
  input.set_id(id);

  input.sample_rate = None;
  let carbon_line = to_carbon_line(&input);
  let output = parse(&carbon_line)?;
  if input != output {
    bail!(
      "input != output: input {:?}, output {:?}, line {:?}",
      input,
      output,
      String::from_utf8(carbon_line.to_vec()).unwrap(),
    );
  }
  Ok(())
}

#[test]
fn to_carbon_line_simple() {
  let metric = Metric::new(
    MetricId::new("foo.bar".into(), None, vec![], false).unwrap(),
    None,
    123,
    MetricValue::Simple(5.1),
  );
  let result = to_carbon_line(&metric);
  assert_eq!(result.as_bytes(), b"\"foo.bar\" 5.1 123");
}

#[test]
fn to_carbon_line_timestamp() {
  let metric = Metric::new(
    MetricId::new("foo.bar".into(), None, vec![], false).unwrap(),
    Some(0.1),
    5000,
    MetricValue::Simple(5.1),
  );
  let result = to_carbon_line(&metric);
  assert_eq!(result.as_bytes(), b"\"foo.bar\" 5.1 5000");
}

#[test]
fn to_carbon_line_tags() {
  let metric = Metric::new(
    MetricId::new(
      "foo.bar".into(),
      None,
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
          tag: "source".into(),
          value: "mysource".into(),
        },
        TagValue {
          tag: "host".into(),
          value: "myhost".into(),
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
    1234,
    MetricValue::Simple(5.1),
  );
  let result = to_carbon_line(&metric);
  assert_eq!(
            result.as_bytes(),
            b"\"foo.bar\" 5.1 1234 \"source\"=\"mysource\" \"host\"=\"myhost\" \"tag1\"=\"value1\" \"tag2\"=\"\" \"tag3\"=\"value3\""
        );
}

#[test]
fn to_carbon_line_tags_complex() {
  let metric = Metric::new(
    MetricId::new(
      "foo.bar".into(),
      None,
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
          tag: "source".into(),
          value: "mysource".into(),
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
    5000,
    MetricValue::Simple(5.1),
  );
  let result = to_carbon_line(&metric);
  assert_eq!(
            result.as_bytes(),
            b"\"foo.bar\" 5.1 5000 \"source\"=\"mysource\" \"tag1\"=\"value1\" \"tag2\"=\"\" \"tag3\"=\"value3\""
        );
}
