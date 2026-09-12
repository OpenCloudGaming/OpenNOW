#!/usr/bin/env bash
set -euo pipefail

apt_root="${1:-/etc/apt}"
sources="$apt_root/sources.list.d/ubuntu.sources"
test -f "$sources"

sed -E -i \
  -e 's#https?://(archive|us.archive|security)\.ubuntu\.com/ubuntu#https://mirrors.edge.kernel.org/ubuntu#g' \
  "$sources"

if [[ -f "$apt_root/blacksmith-ubuntu-mirrors.txt" ]]; then
  printf '%s\n' 'https://mirrors.edge.kernel.org/ubuntu' > "$apt_root/blacksmith-ubuntu-mirrors.txt"
fi
