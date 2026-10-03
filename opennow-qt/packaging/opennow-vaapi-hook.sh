#!/bin/sh

case "$(uname -m)" in
    x86_64) opennow_multiarch=x86_64-linux-gnu opennow_ld_abi=x86-64 ;;
    aarch64) opennow_multiarch=aarch64-linux-gnu opennow_ld_abi=AArch64 ;;
    *) opennow_multiarch='' opennow_ld_abi='' ;;
esac

opennow_host_libva() {
    [ -n "$opennow_ld_abi" ] || return 1
    opennow_ld_cache=$(PATH="$PATH:/sbin:/usr/sbin" ldconfig -p 2>/dev/null) || return 1
    for opennow_library in libva.so.2 libva-drm.so.2; do
        printf '%s\n' "$opennow_ld_cache" \
            | grep -q "^[[:space:]]*$opennow_library (libc6,$opennow_ld_abi)" || return 1
    done
}

if ! opennow_host_libva; then
    opennow_libva_fallback="${this_dir:?}/usr/lib/libva-fallback"
    case ":${LD_LIBRARY_PATH-}:" in
        *":$opennow_libva_fallback:"*) ;;
        *) LD_LIBRARY_PATH="$opennow_libva_fallback${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" ;;
    esac
    export LD_LIBRARY_PATH
    if [ "${LIBVA_DRIVERS_PATH+x}" != x ]; then
        LIBVA_DRIVERS_PATH=/usr/lib/dri:/usr/lib64/dri
        if [ -n "$opennow_multiarch" ]; then
            LIBVA_DRIVERS_PATH="$LIBVA_DRIVERS_PATH:/usr/lib/$opennow_multiarch/dri"
        fi
        export LIBVA_DRIVERS_PATH
    fi
fi
