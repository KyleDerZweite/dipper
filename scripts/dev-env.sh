#!/usr/bin/env bash
# Source this file before Cargo commands for the local native development build.
export DIPPER_PIPEWIRE_PREFIX="${DIPPER_PIPEWIRE_PREFIX:-${XDG_CACHE_HOME:-$HOME/.cache}/dipper-build/sysroot}"
export DIPPER_WHISPER_DIR="${DIPPER_WHISPER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/dipper/prototype-tools/whisper.cpp}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
unset LD_LIBRARY_PATH APPDIR APPIMAGE NO_AT_BRIDGE
