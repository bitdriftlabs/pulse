# Configuration overview

Pulse configuration is defined [canonically using Protobuf](pulse-protobuf/proto/pulse/config/).
The proxy loads the configuration in YAML which is internally converted to Protobuf using the
canonical proto3 to YAML mappings.

The root of the configuration is defined in the [bootstrap
config](pulse-protobuf/proto/pulse/config/bootstrap/v1/bootstrap.proto). The bootstrap contains
global configuration for things like the admin endpoint, meta metrics for the proxy, global
Kubernetes configuration, etc. The bootstrap is primarily a wrapper for a *pipeline* which is
a description of how metrics flow through the proxy. The pipeline has three major components:

* Inflows: Inflows receive metrics, either over the network, via scraping, etc.
* Processors: Processors transform metrics.
* Outflows: Outflows send metrics to a remote destination.

The pipeline is a graph in which inflows can send to any number of processors or outflows and
processors can send to any number of processors or outflows.

The proxy pipeline can either be specified inline within the bootstrap via the `pipeline` field,
or can be loaded via a watched filesystem location (typically a Kubernetes config map) via the
`fs_watched_pipeline` field. In the case of a filesystem watched pipeline, if a change is observed
on the filesystem the pipeline will be reloaded in a hitless fashion, thus allowing for seamless
in place configuration changes without restarts.

For further information see the Protobuf documentation and the [examples/](examples/).
