# Local desktop delivery and update policy

Status: local installation authorized by Kyle on 2026-10-15. Public GitHub
Releases and automatic updates are proposed distribution steps, not publication
in this slice. This follows the existing Rust/Tauri/worker/Journal boundaries.

## Local installation contract

Build optimized release binaries for the current Fedora 43 x86_64 system.
Package the app, sibling worker and clipboard helper, pinned Whisper/Vulkan
shared libraries, icon, source/runtime provenance and SHA-256 file manifest.
Models, settings, recordings and diagnostics never enter the distributable.
System WebKitGTK/GTK, PipeWire, Vulkan driver, KWin, portal and ydotool remain
host prerequisites; do not bundle drivers or restart system services.

Install for the current user under `~/.local/opt/dipper/releases/<release-id>`.
A stable `current` link selects the app; `previous` retains the last installation.
The start menu and `~/.local/bin/dipper` use the installed launcher. No checkout,
Cargo, development wrapper or prototype runtime directory is required at launch.
The installer verifies and stages immutable release files before activation. It
holds the same session lock as the app throughout every installation mutation
and refuses a running Dipper. Never stop an active dictation to upgrade.
A private, durable transaction records prior settings, launchers, desktop entries
and links before changes. Handled failures restore them. After interruption,
recover that transaction under the lock before another operation. Switch current
atomically; reject release-ID collisions with different content. Test injected
failure and interrupted-transaction recovery, not only successful installation.

Keep existing private data paths and model choice. Back up settings before
changing only `native_lib_dir` to the stable installed bundled-runtime path.
The same path follows `current` on later upgrades and rollback. New setups get
the bundled runtime through the launch environment. The model is not copied,
downloaded or changed. The UI retains explicit runtime configuration.
Desktop entries are backed up before replacement. No login startup is enabled.

Packaging/install scripts own files and links. The desktop host retains app
identity, native window, portal and single-instance ownership. Core Settings,
Capture, Recognition and Journal remain unchanged. The worker build gains a
release-relative library search path, keeping the development path in debug
builds only. No Journal migration is introduced. The installer supports an
explicit rollback to a verified previous release. Same-schema binary rollback
is not permission to open a future incompatible Journal schema.

## Acceptance

Build and verify a complete local release. Test manifest corruption rejection,
install/update/rollback with disposable prefixes and stores, running-app lock
refusal, preservation of unrelated settings and data, and launcher independence
from the source checkout. Install while the normal app is idle, retain a private
metadata backup, start the installed app and verify tray/UI and actual synthetic
Hold/Turbo/Paste. Keep target drafts/clipboard intact and remove owned fixtures.
Record installed source revision and exact native libraries actually loaded.
Previously verified crash/storage-full behavior remains core evidence; do not
claim a fresh installed recovery exercise unless it was executed.

## Proposed release cycle

Use semantic pre-release versions while core workflow and delivery mature.
`main` should contain reviewed, passing slices. Cut an `alpha` pre-release after
a useful tested group of changes, rather than publishing every commit. A weekly
candidate is a useful ceiling for routine batches; urgent fixes can ship sooner.
Only promote a candidate after target-machine installation and recovery checks.

A future GitHub Release should contain the platform-specific artifact, checksums,
source/runtime manifest, release notes and compatibility/rollback limits. Start
with manual download and explicit installation. Keep models independent from
app releases. Build Linux artifacts on a declared compatibility baseline; this
Fedora 43 build does not establish support for older distributions.

Before automatic updates, choose RPM versus AppImage as the public channel,
settle signing-key custody and the supported updater version, and test interrupted
downloads, signature rejection and rollback. Signed updates should be offered to
the user and applied only while idle. Do not use updater configuration or pretend
signatures without real protected signing keys. Mobile updates remain separate.

## Build and installation commands

On the declared Fedora build host, build from a clean committed checkout:

```sh
scripts/build.sh --release
python3 scripts/package-release.py
```

Extract the generated `target/dist/dipper-*.tar.gz`, verify its adjacent checksum,
quit Dipper while idle, then run the bundle's installer:

```sh
python3 /absolute/path/to/extracted-bundle/install.py
~/.local/bin/dipper --version
~/.local/bin/dipper --show
```

The installer records the exact source revision and private metadata backup path.
It does not require sudo. Subsequent bundles use the same installer. To undo the
last same-schema upgrade, quit Dipper while idle and run:

```sh
python3 ~/.local/opt/dipper/current/install.py --rollback
```

After an interrupted installer, recover its private transaction before launching:

```sh
python3 ~/.local/opt/dipper/current/install.py --recover
```

For a first installation interrupted before current exists, use `install.py` from
the extracted bundle with `--recover` instead. Checksums detect corruption; they
do not authenticate a download. This local channel has no unattended downloader
or signing key. The [release research](desktop-delivery-research.md) records
current Tauri RPM support and its privilege/dependency-resolution tradeoffs.
