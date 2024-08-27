#!/bin/bash

set -e

python3 ./ci/license_header.py

# Check if git repository is dirty
if [[ -n $(git status --porcelain) ]]; then
  echo "Error: Git repository is dirty. Run ci/license_headers.py to update license headers."
  exit 1
fi
