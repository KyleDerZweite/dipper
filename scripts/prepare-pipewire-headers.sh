#!/usr/bin/env bash
set -euo pipefail
root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root_dir/scripts/dev-env.sh"
if [[ -f "$DIPPER_PIPEWIRE_PREFIX/usr/include/pipewire-0.3/pipewire/pipewire.h" && -f "$DIPPER_PIPEWIRE_PREFIX/usr/include/spa-0.2/spa/param/audio/format-utils.h" ]]; then
  printf '%s\n' 'PipeWire development headers are available.'
  exit 0
fi
command -v dnf >/dev/null
command -v rpm2cpio >/dev/null
command -v cpio >/dev/null
package_version="$(rpm -q --qf '%{VERSION}-%{RELEASE}' pipewire-libs.x86_64)"
package_dir="${XDG_CACHE_HOME:-$HOME/.cache}/dipper-build/rpms"
mkdir -p "$package_dir" "$DIPPER_PIPEWIRE_PREFIX"
dnf download --arch=x86_64 --destdir="$package_dir" "pipewire-devel-$package_version"
for package_file in "$package_dir"/pipewire-devel-*.rpm; do
  rpm2cpio "$package_file" | cpio -idm --quiet --no-absolute-filenames -D "$DIPPER_PIPEWIRE_PREFIX"
done
printf '%s\n' 'Extracted matching headers locally. No system packages or services were changed.'
