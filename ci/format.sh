#!/bin/bash

set -euo pipefail

find pulse-protobuf/proto/ -name '*.proto'|xargs -n1 clang-format -i

# Check if git repository is dirty
if [[ -n $(git status --porcelain) ]]; then
  echo "Error: Git repository is dirty. Run tools/format.sh to format files."
  exit 1
fi
