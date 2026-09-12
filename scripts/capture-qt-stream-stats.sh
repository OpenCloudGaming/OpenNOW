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
        runner=(xvfb-run -a)
    fi
fi

for mode in compact expanded degraded scaled toasts; do
    if ! "${runner[@]}" env "${platform[@]}" QSG_RHI_BACKEND=opengl "$app" \
        --smoke-test --allow-multiple-instances --desktop --route stream \
        --smoke-stream-stats-v2 --smoke-stats-"$mode" --reduced-motion \
        --smoke-width 1920 --smoke-height 1080 \
        --screenshot "$output/$mode.png" >"$output/$mode.log" 2>&1; then
        printf 'Capture failed; see %s\n' "$output/$mode.log" >&2
        exit 1
    fi
    test -s "$output/$mode.png"
    printf '%s\n' "$output/$mode.png"
done
