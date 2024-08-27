#!/bin/bash

set -x
set -e

# Initialize proto submodule
git submodule update --init --recursive

if ! [[ -z "$RUNNER_TEMP" ]]; then
  sudo apt-get update
  sudo apt-get install lld
fi

# Install protoc
if ! protoc --version &> /dev/null; then
  if [[ -z "$RUNNER_TEMP" ]]; then
    echo "Not running in GHA. Install protoc in your path"
    exit 1
  fi

  if [[ "${ARCH}" == "linux/arm64" ]]; then
    PROTOC_ARCH=linux-aarch_64
  else
    PROTOC_ARCH=linux-x86_64
  fi

  pushd .
  PROTOC_VERSION=27.3
  cd "$RUNNER_TEMP"
  curl -Lfs -o protoc-"${PROTOC_VERSION}"-"${PROTOC_ARCH}".zip https://github.com/protocolbuffers/protobuf/releases/download/v"${PROTOC_VERSION}"/protoc-"${PROTOC_VERSION}"-"${PROTOC_ARCH}".zip
  sudo unzip protoc-"${PROTOC_VERSION}"-"${PROTOC_ARCH}".zip
  sudo mv bin/protoc /usr/local/bin/protoc
  popd
fi
