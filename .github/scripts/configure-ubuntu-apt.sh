#!/usr/bin/env bash
set -euo pipefail

apt_root="${1:-/etc/apt}"
sources="$apt_root/sources.list.d/ubuntu.sources"
test -f "$sources"

python3 "$(dirname "${BASH_SOURCE[0]}")/prepare-ubuntu-apt-https.py" "$apt_root"
