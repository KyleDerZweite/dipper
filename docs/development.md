# Build and run the desktop development app

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

Model weights remain external files. New settings select the float16
`large_v3_turbo` profile and `models/ggml-large-v3-turbo.bin` inside the data
directory. `DIPPER_MODEL_PATH` overrides that path for new settings only.
Existing settings without a profile retain `large_v3` and their saved path.
To switch an existing app, select Whisper large-v3-turbo in Recognition setup,
set its matching float16 model path, and save while idle. The next dictation
uses both saved values. Filenames never choose a profile; a mismatched model
fails preparation explicitly and cannot produce an insertion.

```sh
export DIPPER_WHISPER_DIR=/absolute/path/to/whisper.cpp
export DIPPER_MODEL_PATH=/absolute/path/to/ggml-large-v3-turbo.bin
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

Focus a supported editable desktop text field, then hold the configured key. Wait for Recording before
speaking. Use only that key during pilot dictation. Release to finalize audio,
recognize, and attempt Paste. Do not switch targets if you want automatic
insertion. Observed departure from the original target window permanently suppresses
that dictation's insertion, even if you return. The raw text remains in Journal
for explicit Copy. Dipper never sends Enter or pulls focus back.

Recognition automatically detects the spoken language and transcribes without
translation. Each new attempt records `language: "auto"` in its configuration.
Older attempts keep their original configuration and transcript; saved German
settings remain valid for Replay. Auto detects one language per attempt, so
this does not establish quality for speech that switches languages mid-recording.

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
| Input loss | Removing only the owned virtual input during Hold failed explicitly with `input device disappeared`, retained all 50,688 available frames durably, reaped the worker, and left the clipboard unchanged with no insertion. The observed settlement took 237 milliseconds; physical HyperX removal remains untested. |
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

## Sidebar verification, 2026-10-14

The sidebar slice is based on raw-app revision `915a057`. The frontend now has
three local views and one shared appearance owner. Browser fixtures were served
outside the repository and are not production Tauri integration evidence.

Observed browser checks: Dictation/Journal/Settings navigation; preserved form
drafts; keyboard Skip to content retaining the Settings route; dark/light/system
cycling and persistence after navigation/reload; emulated OS preference changes;
busy Settings disabled with Cancel available; Journal load failure and Retry,
empty-state navigation, and long unbroken transcript wrapping. Inspected layouts
at 1280x800, 840x760, 420x560 and 360x640, plus 420x380 reflow and CSS zoom 200%.
No horizontal overflow was observed. True browser zoom and touch hardware were
not exercised. Dark muted text contrast is 7.96:1 and essential borders 3.72:1;
light muted text 5.89:1. Shared native inputs/buttons have 40 px controls and 44 px
sidebar links. The apparent rendering remains an owner judgment.

Real KDE/Wayland Tauri evidence: the embedded canonical logo renders in the
sidebar and KDE titlebar; the native tray PNG was inspected. Actual Refresh,
Save (persisted F2), Retry recovery, and Journal Copy commands succeeded. Copy
used a retained synthetic transcript and restored every original clipboard MIME
format byte-for-byte after the test. A virtual input with priority 0 was used for
a physical F2 Hold test; Settings controls were disabled while recording, Cancel
remained enabled, 44544 available frames were preserved, and no insertion or
clipboard mutation occurred. The OS default microphone was not changed.

An initial Hold attempt after relaunch produced no recording; explicitly
Configure Hold and retry succeeded. A repeated full synthetic Recognition/Paste
journey completed Recognition but safely suppressed Insertion because the current
long T3 thread exceeds the existing 650-node qualification bound. Original
clipboard formats were restored. This is not a successful repeated Paste check;
see the dedicated tracker issue. The earlier raw-app short-thread Paste evidence
remains historical evidence for that environment, not proof for long threads.
HyperX is now visible to PipeWire, but real owner speech and latency percentiles
were not evaluated during UI polish.

Current slice checks: JavaScript syntax, cargo fmt, clippy with warnings denied,
workspace tests (72 passed; 3 ignored child fixtures exercised by parent tests),
and native build. Independent review findings were resolved: contrast, skip-link
routing, failed input discovery preserving the selected microphone, and atomic
migration of generated desktop entries. Installed/user-edited entries remain
preserved; frontend logo is a symlink to root `Logo.png`.

## App-owned window controls, 2026-10-14

The owner replaced system decorations with app controls and selected dark-only
appearance. The switcher, light palette and appearance persistence code are
removed. `ui/window.js` delegates Minimize, Maximize/Restore, Close and edge
Resize to the existing Tauri host. Close still hides to the tray.

Native KDE/Wayland checks on the updated artifact passed: Maximize/Restore,
Minimize and restore from tray, titlebar move, edge resize from 840 to 900 px,
titlebar double-click to maximize, keyboard Enter to restore, and Close/reopen.
An unsaved Settings draft survived Close/reopen and was restored without Save.
Rendered frameless views were inspected at 840x760 and 420x480. Controls measured
44x41 CSS px. Browser fixtures checked keyboard focus, failed action recovery,
narrow geometry, and fixed dark rendering under an emulated light OS preference.
Earlier stored appearance choices have no effect. Physical touch was not tested.

The state-refresh race found by independent review is fixed: a resize received
while a host state query is pending invalidates its reply and triggers a trailing
query. `node --test tests/ui-window.cjs` covers that delayed-reply case and passes.
JavaScript syntax, native build, cargo fmt/clippy and focused independent review
passed. The prior 72-test domain suite remains evidence for unchanged domain code;
window interactions above establish this artifact's native evidence.

## Auto language and compact tray verification, 2026-10-14

New Recognition attempts use Auto, with translation disabled. A replay of a
private copy of the owner's retained English recording completed with detected
language `en` and English text; the retained German synthetic phrase completed
with detected language `de` and German text. Both workers were reaped. The
original Journal/audio was read-only and unchanged. Historical German attempts
remain preserved. Mixed-language speech and quality/latency percentiles still
need the representative pilot corpus.

The compact main window defaults to 680x540 logical pixels, with a 164 px sidebar,
smaller shared controls and a one-row Hold prompt. Main and tray share the last
transcript presenter and explicit Copy. The app-owned quick panel is 320x360.
Its Linux left-activation path uses the existing pinned tray-icon KSNI backend;
the smaller native context menu remains available as fallback.

Native KDE/Wayland evidence: real StatusNotifier Activate calls opened/toggled
and reopened the panel. Placement from top-tray hint 500,60 produced 200,72; a bottom
hint 4400,1400 placed it above the tray on the second output. Escape, outside focus
loss, Open Dipper and explicit Copy passed. Copy was scoped to the quick window
and restored all original clipboard MIME bytes after the check. Right-menu
DBusMenu labels/state and the now nonempty real tray tooltip were inspected.
A physical pointer click on the tray icon was not separately exercised.

During a real F2 recording on the owned silent virtual input, a native tray
activation preserved the active window and opened no panel. Cancel was enabled
while Settings were disabled; 52224 available frames were retained, no Insertion
occurred and clipboard bytes were unchanged. Configure Hold was used before this
check. An earlier post-relaunch attempt produced no recording; restored-shortcut
startup behavior still needs isolated investigation. The test source was removed;
the OS default and normal application store were restored.

The first native activation exposed a GTK-thread Tokio-context panic in the new
code, fixed by using the explicit Tauri runtime handle. Subsequent native runs
passed. Independent review also resolved file cleanup, setup deadlines and stale
placement generations. Superseded tasks cancel and complete cleanup before a new
placement, and popup focus is rechecked against admission. No owned placement
script files remained after the tests.

Workspace tests: 74 passed, plus 4 Node regression checks for stale window-state
replies, immutable Copy identity, owned placement and ambiguous windows.
JavaScript syntax, cargo fmt/clippy and native build passed on the current slice.
Rendered main/Settings/quick views, long content, error/busy states and narrow
420x420 layout were inspected. Native controls measured 38x34 in the titlebar,
with 34 px form controls and a compact quick-panel variant. Physical touch is untested.

Resource samples are not budgets or a controlled backend comparison: the old
AppIndicator parent was 138796 KiB RSS/40 threads; the new KSNI parent after creating
the second WebView was 236600 KiB/55 threads. This includes UI/runtime changes and
cannot isolate KSNI overhead. The quick WebView is lazy and remains cached after
first use; no Recognition worker remains warm after settlement. A matched idle
PSS/CPU comparison remains needed before selecting budgets.

## Composer qualification repair evidence, 2026-10-14

The bounded Collection query repair for #16 was independently reviewed against
main at de2a8b3. The native T3 process supported both queries; qualification
took approximately 26 ms, compared with approximately 390 ms for the earlier
per-node scan in a smaller rendered tree. These are local observations, not
latency percentiles or a resource budget. The old diagnostic still saturated
its 650-node bound in the actual conversation used for the repaired journey.

With Dipper's window hidden in the tray, a configured F2 Hold captured synthetic
German audio through an owned PipeWire input, completed Whisper large-v3 on
RX 7700S/Vulkan, and dispatched Paste. The actual T3 composer matched the raw
transcript exactly and retained Freitag. Release to terminal outcome was
approximately 2.408 seconds. Undo restored the empty composer; all clipboard
formats were restored and the worker was reaped. The Journal continues to label
OS dispatch unverified; external test observation does not change that policy.

The first assertion sampled the composer before asynchronous Paste handling
completed. A fresh observation confirmed the exact text and Undo succeeded.
The corrected harness waits for bounded target acknowledgement.

A separate restart issue remains: ListShortcuts can report F2 while no Hold
events arrive until Configure Hold binds it again. The native test explicitly
configured the binding before admission. Do not treat this evidence as automatic
shortcut restoration acceptance. The test used disposable Journal/audio and
owned virtual input resources; normal user data and the system default input
were preserved. Actual spoken corpus quality remains separate.

## Saved Hold restoration verification, 2026-10-14

The #18 repair supersedes the restart limitation above. A fresh Dipper process
started directly in the tray with its saved F2 assignment, without Configure Hold
or a permission dialog. Actual press/release captured synthetic audio and
completed native T3 Paste with exact transcript acknowledgement. Release to
terminal outcome was approximately 4.140 seconds. Undo restored the empty
composer, all original clipboard formats were restored, and the worker was reaped.
The configured F2 key was preserved without a preferred trigger during restoration.

A separate away-and-return run completed Recognition but kept insertion suppressed
at unchanged stage, with the composer and clipboard preserved. Selection tests
cover absent, empty, ambiguous and localized saved assignments. Native absence
and concurrent KDE configuration removal were not exercised; their behavior
remains fail-closed selection or the documented portal/backend race.

The workspace suite passed 77 tests with three child-entrypoint fixtures ignored
in the parent enumeration and exercised by their parent tests. The documented
serial command was used. An exploratory parallel desktop run failed an existing
helper-output assertion; its required serial run passed. Clippy with warnings
denied, formatting, the native build and four existing UI regressions passed.
Independent review found no blockers in the query or restoration repair.

## Explicit model-profile replay

The native check below adds an attempt to a disposable Journal containing an
existing recording. Use a private fixture copy, not the normal app store.
The example defaults to `large_v3` for compatibility. Turbo is explicit:

```sh
source scripts/dev-env.sh
export DIPPER_MODEL_PATH=/absolute/path/to/ggml-large-v3-turbo.bin
export DIPPER_MODEL_PROFILE=large_v3_turbo
cargo build -p dipper-worker --example native_job --bin dipper-worker --locked
cargo run -p dipper-worker --example native_job --locked -- /absolute/path/to/private-fixture
```

Use `large_v3` with its matching full float16 file for a baseline replay.
`DIPPER_MODEL_PROFILE` applies to this example only. The desktop app reads its
saved Settings profile. Existing attempts and their configuration remain intact.

## Turbo activation and shared control repair, 2026-10-14

The owner selected large-v3-turbo as the active default. Native replay using the
explicit Turbo profile completed on RX 7700S/Vulkan and reaped its worker. Both
mismatches, baseline profile with Turbo weights and Turbo profile with baseline
weights, failed with the expected profile error and reaped their workers. Model
digests were recorded. Legacy missing-profile configuration retains large-v3.

A fresh private native app used the Turbo profile, retained its restored F2 Hold,
saved profile and path through Settings, then hid in the tray. Actual PipeWire
Capture and Turbo Recognition dispatched Paste into the long T3 conversation.
The composer exactly matched the raw transcript; Friday meaning was preserved.
Release to terminal outcome was approximately 1.210 seconds for this short
synthetic case. Worker inference was 873 ms and preparation 3,792 ms, overlapping
Capture. Undo restored the empty composer and the worker was reaped. This is
not a percentile measurement or representative spoken-corpus evaluation.

The first test cleanup encountered a 32-format limit in the owned test restoration
helper on a 39-format image clipboard. The private snapshot remained intact.
The helper limit was extended; the repeated Paste journey restored its complete
text clipboard baseline. The original image's PNG bytes were separately restored
and verified. Exact restoration of all Qt image aliases was not established.
A later clipboard change was preserved rather than overwritten. No clipboard
restoration behavior was added to the application.

The shared control rule previously allocated 16 px of content inside a 34 px
input/select while inherited text line height was approximately 19.5 px. Zero
vertical padding and normal control line height now preserve the glyph box;
selects use a shared app-owned chevron while retaining native HTML semantics.
Buttons already grow to their text and were inspected alongside the fields.
Native [wide](evidence/controls-fixed-wide.png) and
[narrow](evidence/controls-fixed-narrow.png) Settings renders verified controls
at 680 by 540 and 420 by 420 logical pixels, including 1.25 display scaling.
Keyboard selection and Save persisted the correct Turbo profile/path. A first
keyboard probe assumed the wrong option order and missed a field after the
feedback banner moved it; the corrected observed coordinates confirmed Save.

The normal app's profile/path were switched atomically with its existing Settings
owner, then saved again through the native UI. Other input, Hold and runtime
settings and the normal Journal were retained. Only the owned virtual input and
private test app were stopped; the updated normal app runs in the tray.
Workspace tests passed 80 cases with three child-entrypoint fixtures exercised
by their parent tests, plus four UI regressions. Formatting, Clippy with warnings
denied, native worker/app build and native matching/mismatch checks passed.
Independent profile implementation review found no blockers.

The [cleanup research](recognition-cleanup-research.md) and
[public synthetic trial artifact](evidence/cleanup-models-20261014.json) record
separate small-model experiments. Cleanup and model management are not integrated
into the app by this slice. No comparison model remains intentionally resident.


## Cross-app verification, 2026-10-15

The bounded cross-app implementation at `3a1ddb5` removes the T3 app/class
allowlist and keeps KWin departure revocation and metadata-only field checks.
The live T3 provider qualified. A disposable GTK application qualified single-line
ENTRY and multiline TEXT fields, including two windows with one active target.
Password and read-only fields failed qualification. No existing user draft was
modified by these inspections.

An isolated tray-hidden app with an owned PipeWire input completed five actual
F2 Hold/capture/Turbo/Vulkan jobs. ENTRY and TEXT received the exact raw
transcript through Paste, respectively 1.108 s and 1.004 s after release. Their
owned fields were cleared after checking the exact test text. Password, read-only
and departure-and-return cases retained complete recognition results but suppressed
Paste and clipboard mutation. Every worker was reaped. These are short synthetic
integration cases, not microphone quality, production latency or P95/P99.

The native app build and workspace suite passed 84 tests. Three child fixture
entry points are ignored in the direct parent invocation and exercised by parent
scenarios. Both independent Standards and Spec reviews found no blockers. A
subsequent readability-only patch names the unchanged AT-SPI state bits; the 28
desktop tests were repeated successfully. Current-head formatting, Clippy and CI
results are recorded separately with the PR. Temporary native examples are not
part of the installed app.

The test clipboard was restored across all formats in the five completed runs.
An initial harness restoration attempt failed on KDE's `onlyReplaceEmpty` control
MIME, which cannot be blindly replayed over a nonempty test clipboard. Original
text bytes were restored after omitting that control hint; its control metadata
was not reproduced. The rerun used a fresh stable clipboard baseline and passed.
This is a verification-harness limitation, not a new product clipboard policy.
Private evidence stays under `~/.local/state/dipper/verification/cross-app-20261014`.
The folder date is the start of the work, which continued past midnight.

On 2026-10-15, a manual OS Paste test inserted a synthetic marker into the
owner-designated empty Vesktop composer and removed it without sending. The
empty composer was visually confirmed and both original clipboard payloads
were restored. Initial Paste before explicit field focus produced no insertion;
Undo did not remove the marker. Select All and Backspace removed the visually
confirmed synthetic-only contents.

Further native inspection exposed the renderer tree after root/frame
GetAttributes and frame GetRelationSet queries, without a client restart or
global settings changes. The complete tree had 1,830 nodes, one frame and one
focused editable ENTRY with the required field states. The frame still lacked
ACTIVE, even after a focus departure and return. The current Dipper guard
therefore still rejects this client. Automatic Discord dictation/Paste remains
unverified. See the [investigation](vesktop-insertion-research.md) for the source
explanation and a proposed field-aware compatibility path. No blind fallback
was added. Unsupported Collection providers and ambiguous/missing targets still
fail closed.

Original Journal data, normal settings, user applications and system defaults
are preserved. The temporary private app and owned input were removed after verification;
the owned test editor had already exited. The normal app is restarted with
its existing settings after the current build is verified. Installable desktop delivery and upgrade recovery remain issue #26.
