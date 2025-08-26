# Drop tester

The drop tester binary (build via `cargo build --bin pulse-drop-tester`) is used to run self
contained tests.

The configuration for the tests is defined in Protobuf and passed to the tool as YAML. See here for
the [configuration definition](../pulse-protobuf/proto/pulse/drop_tester/v1/drop_tester.proto). The
binary is invoked with the following options:

```
Usage: pulse-drop-tester [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>
      --proxy-config <PROXY_CONFIG>
  -h, --help                         Print help
```

The `-c` option is based the test config. Optionally `--proxy-config` can be used to pass a proxy
configuration to load drop processor configs from. This makes it easier to keep the real
configuration and the test cases in sync.

An example test file look as follows:

```yaml
test_cases:
- config:
    rules:
    - name: foo
      conditions:
      - metric_name:
          exact: bar
  metrics:
  - input: bar:1|c
    dropped_by: foo
  - input: baz:1|g
```

The `dropped_by` test case specifies which rule name should drop the metric. If the metric should
not be dropped just leave it empty/missing.

Currently all input and output metrics are specified in DogStatsD format, even though internally
Prometheus style metrics will work just fine. In the future we will support both formats in tests.
