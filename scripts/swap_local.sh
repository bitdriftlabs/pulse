#!/bin/bash

# Run this to swap all of the shared-core deps to a local version for easy development.
for crate in bd-api bd-client-common bd-grpc bd-hyper-network bd-log bd-logger bd-panic bd-pgv \
             bd-proto bd-proto-util bd-runtime bd-runtime-config bd-shutdown bd-server-stats \
             bd-test-helpers bd-time; do
  /usr/bin/sed -i '' "s/\(${crate}\)[[:space:]]*=.*/\\1\.path = \"\.\.\/shared-core\/\\1\"/g" Cargo.toml
done
