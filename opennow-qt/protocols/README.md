# Wayland protocol inputs

`color-management-v1.xml` is an unmodified copy from the wayland-protocols 1.45
release, commit `0091197f5c1b1f2c131f1410e99f9c95d50646be`.

- Upstream: <https://gitlab.freedesktop.org/wayland/wayland-protocols>
- Pinned source: <https://gitlab.freedesktop.org/wayland/wayland-protocols/-/raw/0091197f5c1b1f2c131f1410e99f9c95d50646be/staging/color-management/color-management-v1.xml>
- SHA-256: `6ab9082518639c7831899832789d1c5e7cde46ea470f46a4307514c679600f5b`

The complete upstream copyright and MIT permission notice remain in the XML.
`packaging/licenses/color-management-v1.txt` also carries the notice into binary
packages through the existing license-directory installation.

`cmake/PlatformHdr.cmake` generates the client bindings from this pinned input
using the build host's `wayland-scanner`. The host does not need a recent
wayland-protocols package for HDR observation; its version must not determine
whether release binaries contain the observer. Wayland client development
files and the scanner are still build dependencies, and the compositor must
advertise `wp_color_manager_v1` at runtime. Builds without the client or scanner
retain the fail-closed stub.
