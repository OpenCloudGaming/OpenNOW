#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    printf 'Usage: bash %s <built-opennow-qt> <output-directory>\n' "$0" >&2
    exit 2
fi

app=$(realpath "$1")
mkdir -p "$2"
output=$(realpath "$2")
runner=()
platform=()
if [[ $(uname -s) == Linux ]]; then
    platform=(QT_QPA_PLATFORM=xcb)
    if [[ -z ${DISPLAY:-} ]]; then
        command -v xvfb-run >/dev/null
        runner=(xvfb-run -a -s '-screen 0 1920x3840x24')
    fi
fi

capture() {
    local name=$1
    shift
    if ! "${runner[@]}" env "${platform[@]}" QSG_RHI_BACKEND=opengl "$app" \
        --smoke-test --allow-multiple-instances --desktop --route settings \
        --smoke-paper-design --reduced-motion --smoke-settings-layout \
        --screenshot "$output/$name.png" "$@" >"$output/$name.log" 2>&1; then
        printf 'Capture failed; see %s\n' "$output/$name.log" >&2
        exit 1
    fi
    test -s "$output/$name.png"
    printf '%s\n' "$output/$name.png"
}

for page in stream network audio controls recording appearance console account about; do
    capture "$page" --smoke-settings-page "$page" --smoke-width 1440 \
        --smoke-height 900 --smoke-settings-full-page
    capture "$page-compact" --smoke-settings-page "$page" --smoke-width 960 \
        --smoke-height 720
    capture "$page-scaled" --smoke-settings-page "$page" --smoke-width 1440 \
        --smoke-height 900 --smoke-settings-scale 1.25
done

for page in stream network recording appearance about; do
    capture "$page-details" --smoke-settings-page "$page" --smoke-width 1440 \
        --smoke-height 900 --smoke-settings-details --smoke-settings-full-page
done

capture stream-details-bottom --smoke-settings-page stream --smoke-width 1440 \
    --smoke-height 1000 --smoke-settings-details --smoke-settings-scroll 1
capture statistics --smoke-settings-page stream --smoke-settings-panel stats \
    --smoke-width 1440 --smoke-height 900 --smoke-settings-details --smoke-settings-full-page
capture shortcuts --smoke-settings-page controls --smoke-settings-panel shortcuts \
    --smoke-width 1440 --smoke-height 900 --smoke-settings-full-page
