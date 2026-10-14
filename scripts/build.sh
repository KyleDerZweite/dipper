#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root_dir/scripts/dev-env.sh"
"$root_dir/scripts/prepare-pipewire-headers.sh"
if [[ ! -f "$DIPPER_WHISPER_DIR/include/whisper.h" || ! -f "$DIPPER_WHISPER_DIR/build/bin/libwhisper.so" ]]; then
  printf '%s\n' 'Set DIPPER_WHISPER_DIR to the pinned whisper.cpp checkout with matching Vulkan libraries.' >&2
  exit 1
fi
revision="$(git -C "$DIPPER_WHISPER_DIR" rev-parse HEAD)"
if [[ "$revision" != 0c7be696371069e0baa2515ed7aa08d5a5473aa6 ]]; then
  printf '%s\n' 'The configured Whisper checkout does not match the pinned runtime.' >&2
  exit 1
fi
cd "$root_dir"
cargo build --locked --workspace "$@"
