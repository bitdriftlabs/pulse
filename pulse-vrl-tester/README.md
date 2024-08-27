# VRL tester

The VRL tester binary (build via `cargo build --bin pulse-vrl-tester`) is used to run self
contained tests.

The configuration for the tests is defined in Protobuf and passed to the tool as YAML. See here for
the [configuration definition](../pulse-protobuf/proto/pulse/vrl_tester/v1/vrl_tester.proto). The
binary is invoked with the following options:

```
Usage: pulse-vrl-tester [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>
      --proxy-config <PROXY_CONFIG>
  -h, --help                         Print help
```

The `-c` option is based the test config. Optionally `--proxy-config` can be used to pass a proxy
configuration to load mutate processor programs from. This makes it easier to keep the real
configuration and the test cases in sync.

An example test file look as follows:

```yaml
test_cases:
  - program: |
      if !match_any(.name, [
        r'^staging:infra:k8s:.+_staging:clustercapacity:cpu_request:namespace:sum',
        r'^staging:infra:k8s:.+_staging:clustercapacity:memory_request:namespace:sum',
      ]) {
        abort
      }
    transforms:
    - metric:
        input: staging:infra:k8s:compliance_staging:clustercapacity:cpu_request:namespace:sum:1|c
        output: staging:infra:k8s:compliance_staging:clustercapacity:cpu_request:namespace:sum:1|c
    - metric:
        input: foo:1|c
        output: abort
```

Currently all input and output metrics are specified in DogStatsD format, even though internally
Prometheus style metrics will work just fine. In the future we will support both formats in tests.
