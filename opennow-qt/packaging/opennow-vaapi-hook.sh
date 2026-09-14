#!/bin/sh

if [ "${LIBVA_DRIVERS_PATH+x}" != x ]; then
    LIBVA_DRIVERS_PATH=/usr/lib/dri:/usr/lib64/dri
    case "$(uname -m)" in
        x86_64) LIBVA_DRIVERS_PATH="$LIBVA_DRIVERS_PATH:/usr/lib/x86_64-linux-gnu/dri" ;;
        aarch64) LIBVA_DRIVERS_PATH="$LIBVA_DRIVERS_PATH:/usr/lib/aarch64-linux-gnu/dri" ;;
    esac
    export LIBVA_DRIVERS_PATH
fi
