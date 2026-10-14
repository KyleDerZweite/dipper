# Build and run the raw desktop slice

The first app targets Fedora 43 KDE on Wayland. It uses a Rust core, a Tauri 2
shell, direct PipeWire capture, SQLite/RF64 storage, and one local Whisper
worker per dictation. This development build is not an installer or release.

Use Rust 1.95, a C/C++ compiler, GTK 3/WebKitGTK 4.1/AppIndicator development
packages, and the existing PipeWire runtime. Tests also require Python 3, FFmpeg,
and Node.js. Node executes the generated KWin-script ordering fixture and checks
frontend syntax. The local pilot also requires the
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
For a separate native checkout, use the exact revision and build Vulkan support.
Keep existing prototype checkouts and their data unchanged. This requires CMake,
Vulkan development headers, and the Vulkan shader compiler in the build environment.

```sh
git clone https://github.com/ggml-org/whisper.cpp.git /absolute/path/to/whisper.cpp
git -C /absolute/path/to/whisper.cpp checkout 0c7be696371069e0baa2515ed7aa08d5a5473aa6
cmake -S /absolute/path/to/whisper.cpp -B /absolute/path/to/whisper.cpp/build -DCMAKE_BUILD_TYPE=Release -DGGML_VULKAN=ON -DWHISPER_CURL=OFF
cmake --build /absolute/path/to/whisper.cpp/build --parallel 2
```

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
build or worker startup. It also clears inherited `NO_AT_BRIDGE` so this app
can expose its native accessibility tree. Default compilation uses two jobs. Cargo.lock owns
Rust dependency versions; the worker's native revision is checked separately.

## First use

Open Settings and Journal from the tray. Choose System default or an explicit
microphone. The default is resolved at every recording start; an active recording
stays attached to its resolved source. Set model and native-library paths, then
save. Configure Hold and confirm Dipper's own KDE dialog. The effective key is
shown after assignment. If startup reports Hold not configured, use Configure
Hold again; KDE can reuse an existing grant without another dialog. F2 is only
an initial preference. KDE Shortcuts owns
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
cargo test --workspace --locked -- --test-threads=1
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
```

Public-interface tests use disposable stores and process fixtures. Run tests with
one test thread so fresh executable-fixture writes cannot overlap another
test's fork and temporarily trigger Linux `ETXTBSY`. Explicitly concurrent
scenarios inside each test remain concurrent. Actual
PipeWire, Vulkan, portal, clipboard, T3, rendered UI, resource return, and pilot
latency need their own current-revision integration evidence. See the
[first-slice contract](app-design.md#first-slice-implementation-contract).
Cleanup, spoken self-corrections, explicit Corrections, reduced retention,
Replay/export, mobile, OTA, and full-pilot acceptance remain follow-up work.

GitHub CI checks the workspace with system PipeWire headers and an explicitly
unavailable native Whisper backend, so protocol and resampling fixtures need no
model download. It does not establish native Whisper ABI or GPU acceptance;
those require the local pinned-runtime build and integration evidence.

## Native verification, 2026-10-14

Code revision `79aa5db` was built and checked on Fedora KDE Wayland with the
pinned native Whisper runtime. The complete workspace suite passed 72 tests;
the three ignored child entry points were exercised by their parent fixtures.
Strict Clippy, formatting, frontend syntax, and shell syntax checks passed.
Independent Standards and Spec reviews resolved the reported target reservation,
qualification settlement, event ordering, persistence, and cleanup findings.

| Check | Evidence and limit |
| --- | --- |
| Complete raw chain | The rendered app's F2 Hold captured a synthetic German phrase through an isolated PipeWire source. Full large-v3 used RX 7700S Vulkan. Actual T3 composer text matched the raw transcript, and Undo restored the initially empty composer. Release to terminal Paste outcome was 3.614 seconds for this short case. |
| Focus loss | Leaving T3 and returning before release permanently suppressed insertion. The attempt retained text; no Paste or clipboard publication occurred. |
| Background controls | During recording, tray Settings kept T3 focused. Tray Cancel preserved available audio and the clipboard. Closing the window hid it while the process continued; tray Settings reopened it. |
| Rendered UI | Setup and Journal were inspected at 420- and 840-pixel window widths. Recording, canceled, complete, suppressed, and unverified outcomes were visible. Refresh devices, Save settings, explicit Copy, and recovery retry ran through native controls. |
| Worker ownership | Successful jobs exited and were reaped. Closing the real native worker control pipe during preparation and after Ready produced exit 125 and reaping in 3 and 164 milliseconds in separate probes. These are EOF probes, not a hard-system-crash test. |
| Storage | Default Ryzen capture retained 132,608 synchronized frames in an earlier native smoke test. Public fixtures cover actual file write failure, SQLite FULL, interrupted recovery, and exact preserved PCM. Hardware power-loss durability remains untested. |
| Resource return | No recognition worker remained after settlement. A three-second development-build idle sample measured about 282 MiB PSS across the app and its children and 0.65% of one CPU core. GPU VRAM samples include other running applications and are not attributable model budgets. |

Private evidence remains outside Git under the workstation's Dipper verification
store. No microphone audio, existing draft, or clipboard content is published.
The synthetic insertion was undone, all original clipboard MIME formats and bytes
were restored, and only owned verification resources were used. Existing
applications and services were preserved. A first-run check recreated Dipper's
own hidden development desktop entry before GTK initialization and registered
its portal identity successfully.

The HyperX USB microphone was disconnected, as confirmed by Kyle. Its actual
capture and spoken quality remain unexecuted. The short synthetic case establishes
integration, not the pilot's quality corpus, P95/P99 latency, resource budgets,
or owner acceptance. Cleanup, explicit Corrections, reduced retention, and the
complete comparison remain required follow-up scope.
