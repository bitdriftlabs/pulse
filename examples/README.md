# Examples

The examples in this folder demonstrate various elements of a more complicated daemonset and
aggregation tier deployment.

* bootstrap_ds.yaml: This is the bootstrap configuration for the daemonset. It writes meta stats
  to the agg tier, and identifies itself via the Kubernetes node and pod names (delivered via the
  upward API). The actual configuration is loaded from a config map which can be hot reloaded.
* bootstrap_agg.yaml: This is the bootstrap configuration for the aggregation tier. It writes
  meta stats to an APS workspace, using default AWS auth. The actual configuration is loaded from
  a config map which can be hot reloaded.

Both bootstrap configurations expose the admin API on port 9999.

* config_ds.yaml: This configuration demonstrates a pipeline that scrapes Prometheus metrics from
  various sources. It does various mutations before sending them off to the aggregation tier.
* config_agg.yaml: This configuration demonstrates a pipeline that interoperates with a control
  plane to send metric samples and receive remote controlled block lists. It also performs
  aggregation on incoming metrics from the daemonsets. Finally the metrics are sent off to an APS
  workspace via default AWS auth. The aggregation configuration is tuned for Prometheus, and a
  decision has been made to convert absolute counters to delta counters (pre-rate) and to send
  stale markers. This makes the entire setup work better with zero elision and blocking.
