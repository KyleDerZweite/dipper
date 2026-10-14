#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root_dir/scripts/dev-env.sh"
cd "$root_dir"
exec "${CARGO_TARGET_DIR:-target}/debug/dipper" "$@"
