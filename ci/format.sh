#!/bin/bash

set -euo pipefail

buf lint
buf format -w pulse-protobuf/proto/

# Check if git repository is dirty
if [[ -n $(git status --porcelain) ]]; then
  echo "Error: Git repository is dirty. Run tools/format.sh to format files."
  exit 1
fi
