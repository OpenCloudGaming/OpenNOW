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

capture() {
    local name=$1
    shift
    if ! "${runner[@]}" env "${platform[@]}" QSG_RHI_BACKEND=opengl "$app" \
        --smoke-test --allow-multiple-instances --desktop --route home \
        --smoke-onboarding --reduced-motion "$@" \
        --screenshot "$output/$name.png" >"$output/$name.log" 2>&1; then
        printf 'Capture failed; see %s\n' "$output/$name.log" >&2
        return 1
    fi
    test -s "$output/$name.png"
    printf '%s\n' "$output/$name.png"
}

for step in {0..5}; do
    capture "desktop-$step" --smoke-width 1440 --smoke-height 900 --onboarding-step "$step"
    capture "compact-$step" --smoke-width 960 --smoke-height 540 --onboarding-ui-scale 1.25 --onboarding-step "$step"
    capture "scrolled-$step" --smoke-width 960 --smoke-height 540 --onboarding-ui-scale 1.25 --onboarding-step "$step" --onboarding-scroll-check
    capture "light-$step" --smoke-width 1440 --smoke-height 900 --smoke-light-theme --onboarding-step "$step"
done
capture login --smoke-width 1440 --smoke-height 900 --onboarding-login
capture login-compact --smoke-width 960 --smoke-height 540 --onboarding-login
capture login-scrolled --smoke-width 960 --smoke-height 540 --onboarding-login --onboarding-scroll-check
