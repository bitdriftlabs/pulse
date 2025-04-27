# Pulse

Pulse is an observability proxy built for very large metrics infrastructures. It derives ideas
from previous projects in this space including [statsite](https://github.com/statsite/statsite)
and [statsrelay](https://github.com/lyft/statsrelay), while offering a modern API driven
configuration and hitless configuration reloading similar to that offered by
[Envoy](https://github.com/envoyproxy/envoy).

While [OTel Collector](https://github.com/open-telemetry/opentelemetry-collector),
[Fluent Bit](https://github.com/fluent/fluent-bit]), and
[Vector](https://github.com/vectordotdev/vector) are all excellent projects that offer some level of
metrics support, they are lacking when it comes to scaling very large metrics infrastructures,
primarily around:

  * Aggregation (e.g., dropping a pod label to derive service level aggregate metrics)
  * Clustering (consistent hashing and routing at the aggregation tier)
  * Automated blocking/elision based on control-plane driven configuration

This project fills those gaps. Pulse has also been heavily optimized for performance. It is deployed
in production in clusters processing hundreds of millions of metrics per second.

## High level features

* Protocols
  * Prometheus Remote Write (inflow and outflow). Outflow supports AWS IAM authentication in order
    to interoperate with Amazon Managed Prometheus.
  * OTLP (inflow and outflow). Currently only OTLP over HTTP+protobuf is supported. No compression
    or snappy is supported.
  * Prometheus K8s scraping (inflow)
  * StatsD and DogStatsD (inflow and outflow)
  * Carbon (inflow and outflow)
  * Any protocol can be converted to any other protocol during processing.
* Dynamic configuration
  * Configuration is defined using Protobuf and loaded via YAML
  * Configuration can be "hot" reloaded from K8s config maps, allowing for hitless reloads of all
    configuration elements. (Control plane driven configuration similar to what is offered by
    Envoy's xDS is not currently implemented but would not be difficult to add depending on
    interest.)
* Internode clustering
  * Multiple Pulse proxies can be clustered in a consistent hash ring. This allows the same
    metric to always be routed to the same proxy node, where it can be consistently mutated,
    aggregated, etc.
* Support for scripting using [VRL](https://vector.dev/docs/reference/vrl/). Pulse embeds a lightly
  modified version of VRL that can be accessed at various points in the pipeline.
* Kubernetes integration. Pulse is capable of interfacing with the Kubernetes API to fetch pod
  and service information that can be used for Prometheus scraping as well as enriching the VRL
  context passed to metric processors. This allows, for example, metrics to be altered based on a
  pod's name, namespace, etc.
* Processors / transformers
  * [Aggregation](pulse-protobuf/proto/pulse/config/processor/v1/aggregation.proto): Similar in
    spirit to the aggregation functionality offered by
    [statsite](https://github.com/statsite/statsite), this processor also supports aggregating
    Prometheus metrics. Depending on the infrastructure, aggregation should be the first priority
    when seeking to reduce points per second. Depending on the environment it is easily possible
    to drop overall volume by 1-2 orders of magnitude while at the same time increasing query speed
    for the vast majority of queries performed.
  * [Buffer](pulse-protobuf/proto/pulse/config/processor/v1/buffer.proto): This is a simple
    buffer that can absorb metric bursts from previous portions of the pipeline. This is useful
    when developing a pipeline that aggregates metrics once per minute, since the majority of work
    is done at the top of the minute.
  * [Cardinality Limiter](pulse-protobuf/proto/pulse/config/processor/v1/cardinality_limiter.proto):
    This processor uses a Cuckoo Filter to keep track of metric cardinality over a window of time.
    Cardinality limits can be global or per Kubernetes pod, with the limits optionally determined
    via a VRL program. Metrics that exceed the limit are dropped.
  * [Cardinality Tracker](pulse-protobuf/proto/pulse/config/processor/v1/cardinality_tracker.proto):
    The cardinality tracker is useful for understanding top users of metrics. It can compute counts
    based on both streaming HyperLogLog as well as streaming TopK (filtered space saving) to easily
    understand where metrics are coming from based on provided configuration.
  * [Elision](pulse-protobuf/proto/pulse/config/processor/v1/elision.proto): The elision processor
    is capable of dropping metrics via control plane provided [FST
    files](https://github.com/BurntSushi/fst). It is also capable of dropping repeated zeros ("zero
    elision") which can lead to a very large amount of savings if a vendor charges based on points
    per second, given how frequent repeated zeros are. See below for more information on the
    control plane driven aspects of Pulse.
  * [Internode](pulse-protobuf/proto/pulse/config/processor/v1/internode.proto): This processor
    provides the internode/consistent hashing functionality described above. In order to correctly
    perform aggregation and most other functionality within an aggregation tier, the same metric
    must arrive on the same node. This processor offers self contained sharding and routing if
    external sharding and routing is not provided some other way.
  * [Mutate](pulse-protobuf/proto/pulse/config/processor/v1/mutate.proto): The mutation processor
    allows a [VRL](https://vector.dev/docs/reference/vrl/) program to be run against all metrics.
    This offers a massive amount of flexibility as different VRL processors can be run at different
    points in the pipeline. For example, pre-sharding, the pod label might be dropped, forcing all
    metrics without the label to get routed to the same node via internode, and then aggregated. The
    mutate filter can also be used for routing, by sending metrics into multiple filters
    simultaneously and aborting metrics that should not continue further. In Kubernetes deployments,
    extra metadata for pods and services are made available to the VRL context.
  * [Populate cache](pulse-protobuf/proto/pulse/config/processor/v1/populate_cache.proto): As
    written above, performance is a primary design goal of Pulse. Some processors require
    persisted state, which is stored in a high performance LRU cache. In order to avoid repeated
    cache lookups, populating the metric in the cache and loading its state is an explicit action
    that must be configured within a pipeline.
  * [Regex](pulse-protobuf/proto/pulse/config/processor/v1/regex.proto): This processor provides a
    simple regex based allow/deny filter. While anything that this processor does could be
    implemented using the mutate/VRL processor, this processor is provided for simplicity and
    performance.
  * [Sampler](pulse-protobuf/proto/pulse/config/processor/v1/sampler.proto): The sampler sends
    metrics samples to a control plane in Prometheus Remote Write format. Why this is useful is
    described more below.

## Deployment types

Many different deployment types are possible with Pulse. Some of these are described below to give
an idea of the possibilities:

* Sidecar/Daemonset: As the first stage of processing, Pulse is capable of either receiving push
  metrics directly or scraping Prometheus targets. Arbitrary processing can happen at this stage
  including transformations, cardinality limiting, etc. In StatsD style infrastructures, this
  tier can handle initial aggregation of counters, gauges, and timers prior to sending further
  in the pipeline.
* Aggregation tier: A clustered aggregation tier is where the majority of metrics reduction will
  take place. As described above, consistent hashing can be used to route metrics to appropriate
  processing nodes. On these nodes, metrics can be aggregated, and blocking/elision can take place.
* Use of a control plane: Pulse can both receiving configuration from a control plane (including
  FST block files), as well as send samples of metrics that pass through the proxy. If a control
  plane can see which metrics are read (for example by intercepting queries), and knows which
  metrics are written, it becomes possible to build an automated system that blocks metrics that are
  written but never read (which in large infrastructures is usually a huge portion of metrics).
  Pulse was built to interoperate with control planes to provide this type of functionality, all
  with hitless reloads.

## Getting started

See [examples/](examples/) for various configuration examples. These should provide a good base of
understanding around what Pulse can provide when coupled with the canonical Protobuf configuration
specification.

## Documentation

The project is currently very light on documentation which is something we would like to rectify
moving forward. The best documentation sources are:

* This README
* The high level [configuration README](CONFIGURATION.md)
* The [configuration Protobufs](pulse-protobuf/proto/pulse/config/)
* The [examples/](examples/)
* In a worst case scenario the proxy integration tests cover most features and show various
  configuration examples. They can be found [here](pulse-proxy/src/test/integration/).

## VRL tester

VRL programs are used in various parts of the proxy, in particular inside the mutate processor. We
provide a binary that can be used for testing VRL programs. See [here](pulse-vrl-tester/) for more
information.

## Admin endpoint

The proxy supports a local admin endpoint that exposes the following endpoints:
* /healthcheck: can be used for liveness/readiness checks if desired.
* /log_filter: dynamically change the log level via RUST_LOG.
* /metrics: available for prometheus scraping of meta stats if desired.
* /profile_*: enable/disable memory profiling.

Depending on configuration the following additional endpoints are available:
* /dump_poll_filter: Dump information about loaded allow/block lists.
* /last_aggregation: Dump information about the last aggregated batch of metrics.
* /cardinality_tracker: Dump information about discovered cardinality.
* /last_elided: Dump information about the elision status of a particular metric.

## Docker images

We do not currently provide numbered releases. This may change in the future. We build x64/arm64
multi-arch images for each commit which are published to [public
ECR](https://gallery.ecr.aws/bitdrift/pulse). The docker images contain both the pulse-proxy and
pulse-vrl-tester binaries.

## License

The source code is licensed using [PolyForm Shield](LICENSE). If you are an end user, broadly you
can do whatever you want with this code. See the [License FAQ](LICENSE_FAQ.md) for more information.

## Support

For questions and support feel free to [join bitdrift
Slack](https://communityinviter.com/apps/bitdriftpublic/bitdrifters) and ask questions in the #pulse
room. For commercial support or to discuss options for a managed control plane which will handle
automatic metric discovery and blocking, contact us at [info@bitdrift.io](mailto:info@bitdrift.io)
to discuss.
