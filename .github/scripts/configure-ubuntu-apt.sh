#!/usr/bin/env bash
set -euo pipefail

apt_root="${1:-/etc/apt}"
sources="$apt_root/sources.list.d/ubuntu.sources"
test -f "$sources"

sed -i \
  -e 's|http://archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
  -e 's|http://us.archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
  -e 's|http://security.ubuntu.com/ubuntu|https://security.ubuntu.com/ubuntu|g' \
  "$sources"

if [[ -f "$apt_root/blacksmith-ubuntu-mirrors.txt" ]]; then
  printf '%s\n' 'https://archive.ubuntu.com/ubuntu' > "$apt_root/blacksmith-ubuntu-mirrors.txt"
fi
