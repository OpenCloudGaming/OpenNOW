#!/usr/bin/env bash
set -euo pipefail

deb=$(realpath "${1:?Usage: verify_bundled_deb.sh PACKAGE.deb}")
verification=$(cd "$(dirname "$0")" && pwd)
test -f "$deb"
docker run --rm -i \
  --mount "type=bind,source=$(dirname "$deb"),target=/packages,readonly" \
  --mount "type=bind,source=$verification,target=/verification,readonly" \
  -e "OPENNOW_DEB=/packages/$(basename "$deb")" \
  ubuntu:24.04 bash -s <<'VERIFY'
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
export LC_ALL=C.UTF-8
printf 'path-include=/usr/share/doc/opennow/*\n' > /etc/dpkg/dpkg.cfg.d/zz-opennow-test
apt-get update
apt-get install -y --no-install-recommends "$OPENNOW_DEB"
test "$(dpkg-query -W -f='${db:Status-Status}' opennow)" = installed
if dpkg-query -W -f='${binary:Package}\t${db:Status-Status}\n' \
    | grep -E '^(libqt6|qt6-|qml6-|libsdl3).*installed$'; then
  echo 'The bundled DEB must not install distribution Qt or SDL3 packages' >&2
  exit 1
fi
test "$(dpkg-query -S /opt/opennow/usr/bin/opennow-qt)" = 'opennow: /opt/opennow/usr/bin/opennow-qt'
test -f /usr/share/applications/io.github.opencloudgaming.OpenNOW.desktop
test -f /usr/share/icons/hicolor/scalable/apps/io.github.opencloudgaming.OpenNOW.svg
test -f /usr/share/metainfo/io.github.opencloudgaming.OpenNOW.metainfo.xml
test -f /usr/share/doc/opennow/THIRD_PARTY_NOTICES
while IFS= read -r -d '' binary; do
  dependencies=$(ldd "$binary" 2>&1) || {
    if [[ "$dependencies" == *'statically linked'* ]]; then
      continue
    fi
    printf '%s\n%s\n' "$binary" "$dependencies" >&2
    exit 1
  }
  if [[ "$dependencies" == *'not found'* ]]; then
    printf '%s\n%s\n' "$binary" "$dependencies" >&2
    exit 1
  fi
done < <(find /opt/opennow/usr/bin /opt/opennow/usr/lib /opt/opennow/usr/plugins /opt/opennow/usr/qml \
    -type f \( -name '*.so*' -o -name 'opennow-*' \) -print0)
useradd --create-home opennow-test
install -d -m 700 -o opennow-test -g opennow-test /tmp/opennow-runtime
runuser -u opennow-test -- env QT_QPA_PLATFORM=offscreen XDG_RUNTIME_DIR=/tmp/opennow-runtime \
  timeout 60 /usr/bin/opennow-qt --smoke-test --allow-multiple-instances --route home --reduced-motion
apt-get install -y --no-install-recommends python3 binutils xvfb xauth mesa-vulkan-drivers
runuser -u opennow-test -- python3 /verification/verify_linux_package.py /opt/opennow/usr/bin
runuser -u opennow-test -- env QT_QPA_PLATFORM=xcb XDG_RUNTIME_DIR=/tmp/opennow-runtime \
  timeout 60 xvfb-run -a /usr/bin/opennow-qt --smoke-test --allow-multiple-instances --route home --reduced-motion
apt-get install -y --reinstall --no-install-recommends "$OPENNOW_DEB"
apt-get purge -y opennow
test ! -e /usr/bin/opennow-qt
test ! -e /opt/opennow
test ! -e /usr/share/applications/io.github.opencloudgaming.OpenNOW.desktop
echo 'Bundled DEB install, dependency, offscreen/X11 smoke, capability, reinstall and removal checks passed'
VERIFY
