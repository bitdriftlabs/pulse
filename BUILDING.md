# Building Pulse

## Install dependencies

* [Install Rust](https://www.rust-lang.org/tools/install). The project does not currently have a
  Minimum Supported Rust Version (MSRV). We may use features from the latest Rust stable releases
  as soon as they are released. If you are seeing Rust build failures first perform `rustup update`.
* [Install Protobuf](https://github.com/protocolbuffers/protobuf/releases). Using the package
  manager of your choice should be fine if you like.
* Note that on Linux the cargo config assumes that `lld` is available as the linker in order to
  improve linking speed.

## Building

To build all binaries:

```
cargo build
```

To run all tests:

```
cargo test
```

To build just the proxy binary:

```
cargo build --bin pulse-proxy
```

To develop and run at the same time:

```
pulse> cargo run --bin pulse-proxy -- --help
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.37s
     Running `target/debug/pulse-proxy --help`
Usage: pulse-proxy [OPTIONS] --config <CONFIG>

Options:
  -c, --config <CONFIG>
      --config-check-and-exit
      --version
      --shutdown-delay <SHUTDOWN_DELAY>  [default: 0]
  -h, --help                             Print help
```

## Release binaries

Make sure to compile with the optimizing compiler as is done in the Docker builds.

```
cargo build --release
```
