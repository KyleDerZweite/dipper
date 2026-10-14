# Build and run the raw desktop slice

The first app targets Fedora 43 KDE on Wayland. It uses a Rust core, a Tauri 2
shell, direct PipeWire capture, SQLite/RF64 storage, and one local Whisper
worker per dictation. This development build is not an installer or release.

Use Rust 1.95, a C/C++ compiler, GTK 3/WebKitGTK 4.1/AppIndicator development
packages, and the existing PipeWire runtime. The local pilot also requires the
already running user ydotool daemon for Ctrl+V. Dipper does not install or
restart that daemon. Portal and KWin integration run in your desktop session.

## Native dependencies

`scripts/prepare-pipewire-headers.sh` downloads the matching Fedora development
RPM and extracts headers to a user build cache. It installs no system package
and restarts no service. Alternatively set `DIPPER_PIPEWIRE_PREFIX` to your
existing header prefix.

Set `DIPPER_WHISPER_DIR` to a whisper.cpp checkout at
`0c7be696371069e0baa2515ed7aa08d5a5473aa6`, with its matching Vulkan libraries in
`build/bin`. The development script can use the already available local native
build. It never modifies the preserved prototype or publishes its data.
Model weights remain external files. Set `DIPPER_MODEL_PATH` to the existing
`ggml-large-v3.bin`, or select it in Dipper Settings.

```sh
export DIPPER_WHISPER_DIR=/absolute/path/to/whisper.cpp
export DIPPER_MODEL_PATH=/absolute/path/to/ggml-large-v3.bin
./scripts/build.sh
./scripts/run.sh --show
```

Source `scripts/dev-env.sh` before direct Cargo commands. It removes inherited
AppImage library overrides that can otherwise select T3's libraries during
build or worker startup. Default compilation uses two jobs. Cargo.lock owns
Rust dependency versions; the worker's native revision is checked separately.

## First use

Open Settings and Journal from the tray. Choose System default or an explicit
microphone. The default is resolved at every recording start; an active recording
stays attached to its resolved source. Set model and native-library paths, then
save. Configure Hold and confirm Dipper's own KDE dialog. The effective key is
shown after assignment. F2 is only an initial preference. KDE Shortcuts owns
changes on GlobalShortcuts v1; Dipper does not remap Caps Lock.

Focus a T3 composer, then hold the configured key. Wait for Recording before
speaking. Use only that key during pilot dictation. Release to finalize audio,
recognize, and attempt Paste. Do not switch targets if you want automatic
insertion. Observed departure from the original T3 window permanently suppresses
that dictation's insertion, even if you return. The raw text remains in Journal
for explicit Copy. Dipper never sends Enter or pulls focus back.

Closing Settings hides the window and keeps the tray app running. Cancel
preserves available audio and blocks late insertion. Quit waits for capture
finalization and worker cleanup. If a physical release is missed, use Cancel;
the portal is not an independent physical-key tracker. Clipboard publication
and Paste are separate operations with an accepted residual focus race.

## Data and recovery

The app uses `~/.local/share/dipper/app` for its private database/audio/settings
and `~/.local/state/dipper/app` for diagnostics. These are separate from the
preserved experiment and evaluation artifacts. `DIPPER_DATA_DIR` and
`DIPPER_STATE_DIR` can select disposable stores for tests. Existing data is
never automatically purged. Initial comprehensive retention is this slice's
pilot policy, not a universal product default.

Only synchronized audio plus committed metadata acknowledges complete capture.
Interrupted work is recovered without automatic recognition or insertion.
Original interrupted audio is preserved. A hard system crash can lose an
unsynchronized tail; checkpoint cadence is not a measured maximum-loss promise.
Do not remove a store's lock/WAL files while Dipper is running.

## Verification

```sh
source scripts/dev-env.sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Public-interface tests use disposable stores and process fixtures. Actual
PipeWire, Vulkan, portal, clipboard, T3, rendered UI, resource return, and pilot
latency need their own current-revision integration evidence. See the
[first-slice contract](app-design.md#first-slice-implementation-contract).
Cleanup, spoken self-corrections, explicit Corrections, reduced retention,
Replay/export, mobile, OTA, and full-pilot acceptance remain follow-up work.
