FROM ubuntu:jammy as base

RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends ca-certificates curl dumb-init \
    && apt-get autoremove -y && apt-get clean \
    && rm -rf /tmp/* /var/tmp/* \
    && rm -rf /var/lib/apt/lists/*

USER root

COPY target/release/pulse-proxy /usr/local/bin/pulse-proxy
COPY target/release/pulse-vrl-tester /usr/local/bin/pulse-vrl-tester

CMD ["/usr/local/bin/pulse-proxy"]
