# App foundation and module design

Status: first-slice implementation contract technically reviewed, 2026-10-14.
The subsequent [cross-app insertion contract](cross-app-insertion.md) supersedes
T3-only target admission, composer-class checks and Accessibility field
admission. Kyle selected original-window validation on 2026-10-15; historical
password/read-only rejection rules below no longer describe current insertion.
Its conservative focus, clipboard, cancellation and Journal contracts remain
compatible with this foundation. Historical T3-specific contracts below record
the first slice and must not be treated as a current app allowlist requirement.
The [preparation plan](preparation.md) records the accepted permission to start
app work from provisional evidence. Issue #4 owns foundation and module design;
issue #5 owns implementation breakdown. The owner selected an independent app in [ADR-0001](adr/0001-own-dipper-app.md);
Rust core and Tauri desktop are accepted in [ADR-0002](adr/0002-rust-core-tauri-desktop.md).
The implementation contract below consolidates accepted scope and technical choices. Independent Worker, Capture/Journal, and Desktop reviews found no remaining contract blockers. The useful raw implementation and dated integration evidence are recorded in the
[development guide](development.md). Full-pilot acceptance remains separate.

## Evidence and limits

The preserved local experiment at `prototype/native-recorder`, revision
`3436eb18`, demonstrated capture from the actual USB microphone and local
recognition with Whisper/whisper.cpp/Vulkan and Qwen3-ASR on CPU. Its README,
TRANSCRIPTION, and EVALUATION records own exact results and limitations.
The prototype is not a production foundation and remains unpublished.

Global KDE Wayland hold-to-record, real T3 insertion, production journal
consistency and recovery, and the release-to-insertion gate remain unverified.
App work may address these gaps while additional recordings accumulate.

## Accepted first slice

The owner selected the first usable chain on 2026-10-13: global hold-to-record,
complete capture, recognition, and insertion of the raw transcript into T3,
with retained recordings and separate recognition attempts. This combines
capture and voice-to-T3 delivery rather than shipping capture-only first.
Light cleanup and spoken self-corrections remain pilot requirements for the
following slice; raw output is not final pilot acceptance.

Dipper runs as a background/tray app. UI polishing is deferred. The first slice
still needs clear recording, processing, cancellation, and error feedback;
no polished overlay design has been selected.

The owner wants desktop interaction to feel as native as possible. Use that
goal to evaluate KDE integration, shortcut configuration, status feedback,
theme behavior, and focus handling. It does not replace the accepted Rust/Tauri
foundation or select a new UI toolkit.

By default, Dipper uses the system's default input device. It must list available
input devices and allow an explicit choice. Resolve the system default at each recording start and keep that input fixed
for the recording. A changed default applies to the next recording. If the
active device disappears, stop with a failure outcome, preserve available audio,
and do not automatically insert text. Input-selection UX may be refined later.

Whisper large-v3 through whisper.cpp/Vulkan on the RX 7700S was the first
live baseline. The reviewed Turbo profile contract below records the subsequent
owner decision for live recognition. Qwen ASR was excluded from active candidates by owner decision:
the observed CPU startup-inclusive latency exceeded the tolerable wait, despite
useful experimental quality. Preserve its historical evidence and installed
artifacts; do not delete or reinterpret them as a failed capture.

If the user switches away from T3 before insertion, do not insert automatically
or pull focus back. Retain the transcript for explicit copying or later insertion.
An unclear insertion outcome must not trigger an automatic duplicate retry.

Use a Rust core and Tauri 2 desktop shell. Keep core behavior behind interfaces
that do not depend on Tauri or desktop controls. Mobile UI and the frontend
framework remain deferred choices. The reviewed first-slice contract below defines insertion verification and Journal
responsibilities. SQLite with separate
lossless audio and interrupted-recording recovery are accepted in ADR-0003. Long-term update requirements include both
desktop whole-app delivery without manual downloads and compatible mobile
UI/JS updates without a new store release. Native-code changes and model-weight
updates require separate versioning and delivery; update implementation is not
added to the first raw-chain slice by this requirement. The first-slice implementation contract is reviewed; runtime acceptance is tracked separately.

## Accepted persistence and initial insertion

Use SQLite for journal metadata and separate lossless audio files, as recorded
in [ADR-0003](adr/0003-journal-storage.md). Write audio during capture and preserve
available data on interruption. Recovery marks unfinished recordings as
interrupted and performs no automatic insertion after restart. The reviewed contract below defines the initial schema, RF64 audio, and commit
checkpoints. Numerical crash-loss bounds remain unmeasured.

On 2026-10-14, the owner accepted acknowledging completed capture only after
successful audio synchronization and its corresponding metadata commit.
A hard system crash may lose the unsynchronized tail. Preserve available data,
record the last successful synchronization point, and measure observed loss
before choosing a numerical maximum. A fixed crash-loss number is not an
implementation-start prerequisite.

For the first slice, insertion may use Clipboard plus Paste while the intended
T3 target remains valid. Leave the transcript in the clipboard after insertion;
do not automatically restore its former content. If focus changes away from T3,
do not automatically insert or overwrite the clipboard merely to make the
transcript available. Keep it in the journal for explicit use instead.
Clipboard transfer and input-event delivery do not confirm that the target field
accepted the text. Preserve an unverified outcome until actual target evidence
is available, and do not automatically retry an unclear insertion.

On 2026-10-14, the owner accepted a conservative OS guard for the pilot:
require the intended KWin window and a checked composer, and permanently
suppress automatic insertion for that dictation after observed focus loss.
Returning to T3 does not authorize the old result. Clipboard mutation and
Paste remain separate OS operations; a focus change exactly between them
cannot be excluded atomically. This residual race is accepted for the pilot,
not a claim of exact target binding. A cooperative T3 integration is not a
first-slice prerequisite. Ambiguous or unavailable target evidence still
suppresses automatic insertion and clipboard overwrite.

For this pilot, the owner assumes no composer/window switch while holding the
record control. Insert into the checked focused T3 composer; do not add a
thread/draft identity contract or require a permanently pinned editor object.
Observed departure from the target application still revokes insertion
eligibility. Revalidate the current window and composer before dispatch.

The owner prefers one hold key reachable with the left hand. A multi-key chord
is a fallback only if it can be held with one hand. Meta+F9 is not the selected
test binding or product default; screen single left-side keys before testing.

The owner subsequently authorized any suitable temporary test binding and
requires the eventual keybind to be configurable. A live Caps-to-Menu remap
and native portal binding have now been exercised and fully removed. Short
and longer synthetic Holds produced activation/deactivation. Releasing Shift
while Caps was still held also produced deactivation. The owner accepted
using only the Hold key during pilot dictation. Portal `Deactivated` ends
capture, with visible feedback that recording has ended. Strict tracking of
the physical Hold key's release is outside this pilot contract. Session loss
must interrupt an open capture, preserve available audio, and revoke insertion
eligibility. Synthetic portal tests do not establish physical hardware behavior
or an implemented recording indicator.

## Accepted busy and cancellation behavior

On 2026-10-13, the owner endorsed rejecting a new hold while the current
dictation is busy, with visible feedback rather than a recording queue.
Cancellation stops the current operation, preserves available capture audio,
and prevents late recognition results from triggering insertion. Recovery
keeps interrupted data available and performs no automatic insertion.

The implementation contract must define when the operation becomes idle,
how the worker is terminated and reaped, and how insertion authorization is
invalidated. Cancellation cannot retract text already delivered to another
application. The boundary between cancellation and dispatch needs an explicit
outcome rather than a promise of undo.

## Accepted model lifetime

On 2026-10-13, the owner accepted a dedicated recognition worker per dictation.
Begin model preparation when hold starts, while Capture records independently.
Recognize complete capture audio after release and exit the worker after the job.
Do not intentionally retain the model between dictations. A configurable bounded
idle grace period remains a possible later optimization.

The [model-lifetime investigation](model-lifetime.md) records existing load-time
observations and runtime behavior. Capture/load overlap and resource release are
accepted goals, not measured app behavior. Cancellation, busy behavior, worker
failure, and stale-result suppression still need a reviewed contract. Do not
resume prototype iterations to establish these app-specific checks.

[Streaming recognition and cleanup](https://github.com/KyleDerZweite/dipper/issues/11)
are deferred optimizations. They may reduce work remaining after release, but
repeated inference can increase resource use.
Interim text can change with later speech, including spoken self-corrections.
Keep complete capture audio and final quality checks. Streaming insertion into
the target application is not an accepted requirement.

## Proposed module responsibilities

- Dictation exposes begin, release, cancellation, and observable lifecycle state.
  It coordinates a recording and its intended target application.
- Capture owns input discovery and capture lifetime. It returns actual readiness,
  complete capture audio, and completion, cancellation, or failure status.
  It has no recognition, processing, or insertion behavior.
- Recognition accepts existing audio and an explicit configuration. It produces
  a new recognition attempt with raw transcript, model-input provenance,
  timings, and failure status without changing capture audio or confirmed wording.
- Text processing accepts a raw transcript and processing configuration. It
  returns processed text and preserves unresolved ambiguity rather than inventing
  an intended correction.
- Insertion attempts to place text in the intended target application and returns
  an honest outcome, including unverified. An unclear outcome must not silently
  cause duplicate insertion through automatic retries.
- Journal owns durable identities, attempts, outcomes, retention, and visible
  missing-data/recovery state. Storage consistency belongs behind its interface.
- Replay uses actually retained audio for new attempts. It reuses recognition
  and processing without inserting text or delaying active dictation.

Desktop controls depend on Dictation. Dictation depends on Capture, Recognition,
Text processing, Insertion, and Journal. Replay depends on Journal, Recognition,
and Text processing. Playback and explicit correction use Journal. Capture,
Recognition, and Text processing do not depend on the desktop UI or Insertion.
Avoid competing model-specific histories and a speculative plugin framework.

These are logical responsibilities, not committed packages, types, processes,
or a package layout. The accepted Rust/Tauri choice does not finalize them. Refine each useful slice into a short reviewed contract.

## Delivery direction

1. Accepted first slice: the usable raw voice-to-T3 chain, with background/tray
   operation, global hold, input selection, retained recordings, separate
   attempts, honest insertion outcomes, and the accepted storage-full policy.
   Verify actual KDE Wayland control and T3 insertion, including failure paths.
2. Proposed next slice: light cleanup, clear spoken self-corrections, explicit
   correction, and both pilot retention configurations. Verify meaning-bearing
   facts, ambiguity, and honest unavailable playback/replay data.
3. Proposed journal slice: search, replay, and consistent export using retained
   data, followed by end-to-end pilot acceptance and resource measurement.

The first slice scope is accepted; its detailed contract and later slice order remain proposals until foundation and interfaces
are accepted. Keep one PR per useful slice, as already agreed. No full-corpus
completion gate applies to starting these slices, but their own acceptance
checks and dependency directions still govern readiness.

## Toolchain research

The [React Native and Rust comparison](toolchain-research.md) records primary
sources, update mechanisms, platform constraints, and performance evidence gaps.
The owner accepted its Rust-core/Tauri-desktop recommendation in ADR-0002.
Mobile UI remains deferred; no new hybrid dependency is selected.

## First-slice integration research

Three primary-source investigations dated 2026-10-13 refine the open contracts.
Their recommendations are not accepted implementation choices or runtime evidence.

- [Native KDE integration](native-desktop-research.md) recommends an ashpd
  GlobalShortcuts adapter, tray menus, setup-time permission handling, and
  actual T3 accessibility checks. KDE forwards press and release, but chord
  release order and shortcut inhibition need tests. Tauri uses WebKitGTK,
  so native desktop interaction does not imply Qt widgets.
- [Worker lifecycle](worker-lifecycle-research.md) recommends a direct one-job
  library worker, bounded events and requests, explicit termination/reaping,
  and Dictation-owned insertion authorization. The pinned CLI can preload via
  stdin, but does not provide structured readiness. Process separation does
  not guarantee capture is unaffected by resource contention.
- [Journal recovery](journal-recovery-research.md) recommends a bounded audio
  queue, sequential WAV writer, and SQLite WAL with FULL synchronization for
  acknowledged metadata transitions. Audio synchronization precedes completion
  acknowledgment. The selected input must prevent backend fallback or movement;
  a symbolic default route is insufficient.

The integrated proposal commits a recording identity and recognition-attempt
identity before worker preparation starts. Preparation overlaps capture;
inference still waits for complete finalized audio. The worker writes no
independent journal. Dictation becomes idle only after capture finalization,
worker cleanup, and settlement of any insertion dispatch.

The examined APIs do not make T3 field validation, clipboard mutation, and
Paste one atomic operation. AT-SPI and KWin can provide different levels of
target evidence. A [read-only live check](native-desktop-research.md#read-only-installed-t3-observations-2026-10-14)
found a focused, editable multi-line composer candidate with editor markers in
the running T3 build. It lacks EditableText, and its frame reported inactive
while the field reported focused. [Interactive follow-up checks](native-desktop-research.md#interactive-paste-and-focus-observations-2026-10-14)
confirmed actual synthetic Paste into T3 and sampled switching to the game.
The composer retained focused state while the game was active, so the test
guard also required KWin's active-window identity. Those standalone probes did
not establish the raw app's target behavior. Its native integration evidence is
now recorded in the development guide; the residual OS dispatch race remains.
The owner accepted the conservative OS guard above, including its residual
race. The reviewed contract below defines its observer, invalidation, and dispatch
interfaces. A cooperative T3 endpoint is
an optional later mechanism, not an assumed capability or pilot prerequisite.

The reviewed first-slice contract below settles target identity, focus-loss
invalidation, permission/session lifetime, lost-release handling, protocol
limits, supervision deadlines, and synchronization cadence. Observed hard-crash
loss still needs measurement before choosing a numerical bound. The earlier
native probes and disposable persistence experiments informed these contracts.
Current app integration evidence belongs in the [development guide](development.md).

## First-slice implementation contract

The owner instructed us to finalize the remaining contracts and build the app
on 2026-10-14. These technical choices implement the accepted scope. They do
not select a mobile UI, product license, release channel, or retention default.

### Structure and caller interface

The workspace contains a UI-independent Rust core, a Linux desktop adapter,
a direct recognition-worker executable, and a Tauri desktop executable.
The minimal frontend uses local HTML, CSS, and JavaScript without a framework.
It depends on Tauri commands and lifecycle snapshots, never native handles.
Desktop controls call begin, release, cancel, settings, and Journal queries.
The shortcut adapter sends the same lifecycle commands. Dictation owns
admission, generation, cancellation, eligibility, and settlement. Capture,
Recognition, and Journal never call desktop controls or Insertion.

One active dictation is allowed. A repeated activation does not create another
recording. Busy admission is rejected visibly. Release applies only to the
admitted Hold. Idle requires capture finalization, worker reaping, and settled
insertion. Failures preserve available data. Cancellation immediately revokes
eligibility; buffered or stale results cannot restore it. After dispatch,
cancellation cannot promise to retract pasted text.

### Capture and Journal

Journal opens a private application data directory outside the checkout,
with directory mode 0700 and file mode 0600. SQLite uses WAL and synchronous
FULL. A recording and a separate recognition attempt are committed before
worker preparation and capture. Schema versioning rejects unknown newer
schemas rather than rewriting them. A process lock prevents concurrent writers
and prevents recovery from interfering with a running instance.

Capture resolves the default input at each start, or the configured explicit
input, to a PipeWire node serial. It requests mono 48 kHz float32 PCM and
records this conversion in provenance. The stream forbids reconnect, fallback,
and movement. Device disappearance, stream error, invalid sample layout, and
queue overflow or observed sequence discontinuity fail capture and suppress
insertion. A complete outcome requires all accepted frames drained without an
observed gap or storage error.

A direct PipeWire adapter uses a preallocated two-second sample ring. The
realtime callback only validates and copies whole buffers, updates atomics,
and requeues the PipeWire buffer. It does no allocation, disk or SQLite I/O,
logging, blocking wait, or mutex acquisition. A separate writer drains the
ring and owns sequential audio writes. First valid samples establish capture
readiness; worker readiness never stands in for microphone readiness.

Create the audio file exclusively, synchronize its initial header and parent
directory, then commit its path/format association before capture begins.
Retain delivered samples exactly in float32 RF64 WAV, including canceled and
failed prefixes. RF64 avoids a hidden 4 GiB RIFF duration limit. The writer
synchronizes audio nominally once per second, then commits its synced frame
checkpoint. Finalization stops the stream, drains accepted samples, patches
the header, synchronizes the file and directory, then commits capture outcome.
Only that commit acknowledges complete capture. The cadence is an initial
engineering setting, not a maximum crash-loss promise.

Recovery scans unfinished rows and files, preserves frame-aligned available
samples through a derived reader or repaired copy without rewriting the
interrupted original, and marks interrupted audio
and attempts honestly. Record durable checkpoint and recovered frame counts;
never replay or insert automatically. Missing or corrupt audio is visible.
Preserve and report orphan files without attaching them to unrelated recordings.
If recovery cannot commit, expose the derived interrupted state as unpersisted
and retry reconciliation when writable. A preflight free-space check uses an
initial configurable 64 MiB admission floor, which reserves no storage. A later disk write
or SQLite failure retains prior data and never acknowledges successful capture.
There is no automatic purge or silent duration cap.

Journal owns recordings, recognition attempts, insertion outcomes, timing and
configuration provenance, and diagnostic file references. Raw transcripts are
separate from capture outcomes. File availability is explicit in Journal
queries. Schema version one records UUIDs, UTC start/end times, capture status,
input/retention snapshots, relative generated audio/log paths, format/frame
counts and data offset, synced checkpoint, artifact availability, and first failure. Attempts
have their own UUID, recording foreign key, immutable configuration/provenance,
timings, raw text, and outcome. Insertions have their own UUID, attempt foreign
key, target snapshot, persisted intent, outcome, and irreversible dispatch
stage. Enforce foreign keys, unique generated paths, and nonnegative counts.
An immutable audio reader exposes samples only from finalized/recovered
artifacts; Capture never receives a SQLite connection. Full diagnostics stream to private files; a bounded in-memory tail
is only for error feedback. Missing or failed diagnostic writes are recorded,
never presented as complete logs. This slice uses comprehensive retention.
Reduced retention and explicit Corrections belong to the next slice.

### Recognition

Recognition prepares one direct worker per attempt when Hold begins. The worker
uses a small native bridge against the exact pinned whisper.cpp C API and
matching Vulkan libraries. It owns no Journal writes and spawns no subprocesses.
Model weights are an explicit external setting. Development can reuse matching
installed native artifacts; the project records how to build the same revision
without modifying or publishing the preserved prototype.

Start the control-reader before native initialization. The parent retains stdin
until worker exit; unexpected EOF terminates the worker during loading or
inference. The JSON-line protocol has version, generation, recording identity,
and attempt identity on every frame. One Transcribe request may wait while
loading. Events are Ready, Started, and one Result or Failed. Ready means native
initialization completed. Recognition accepts only a valid Result followed by
successful child exit and joined output readers. A result followed by a crash
is a failed attempt, with any returned transcript preserved but not inserted.
Reserve stdout for protocol and stderr for logs. Reject unknown versions, wrong
identities, malformed frames, second requests, duplicate terminal events, and
invalid event order.

On 2026-10-14, the owner selected automatic language detection after English
speech produced German text with the fixed German configuration. New attempts
use `language: "auto"` and transcribe the spoken language without translation.
Recognition owns this default and validates the configuration before launching
its worker. The native bridge passes Auto to the pinned whisper.cpp API with
`translate = false` and `detect_language = false`. In this API, the latter flag
requests detection only when true; Auto itself detects the language and then
continues transcription. The [pinned runtime implementation](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/src/whisper.cpp#L6992-L7015)
owns these API semantics. Previously recorded `language: "de"` configurations
remain valid for Replay. Existing attempts and transcripts are unchanged.
No language selector or new persisted setting is part of this fix.

Acceptance covers the default configuration crossing the worker transport,
legacy German configuration compatibility, rejection of unsupported language
values, and native English and German fixtures producing text in their spoken
language. A transport fixture does not establish native language quality.

Use beam size five, eight CPU threads, no vocabulary prompt, gain, VAD, or
intentional trimming. Resolve and require the RX 7700S
Vulkan backend each job, recording its actual identity. Do not silently select
CPU fallback. Resample finalized 48 kHz audio to mono 16 kHz in the worker;
use rubato with explicit configuration/version provenance. Flush the final
partial input using documented padding, remove reported output delay, and
retain exactly ceil(source frames / 3) output frames. Record padding, removed
delay, actual model-input frames, and timings. Validate RF64 sizes/frame count,
finite samples, supported format, and Whisper integer limits before inference;
fail unsupported input without truncation.
Empty input or empty text produces no-text and skips clipboard mutation.

Initial safeguards are 64 KiB request frames, 8 MiB event frames, 1 MiB raw
transcripts, one pending request, and a 128 KiB diagnostic tail. Request/event/transcript overflow
fails explicitly; transcripts are never silently truncated. The diagnostic
tail rolls over with an incompleteness marker while full logs continue to disk. Preparation has a
60-second deadline. Recognition has 60 seconds plus twice the audio duration
from finalized-request dispatch, covering input validation, resampling, and
inference. Preparation retains its independent deadline if still loading. Normal exit has five seconds after a terminal event. These are
engineering safeguards, not the 30-second pilot latency acceptance ceiling.

Cancellation and deadlines invalidate result acceptance, terminate the owned
worker, and wait/reap explicitly. If cleanup is still unconfirmed after five
seconds, show cleanup failure, retain ownership, and remain busy while waiting.
Dropping a future never substitutes for reaping. Recognition failure during
capture leaves recording available until release or cancellation. Capture
failure cancels recognition. Orderly shutdown revokes insertion first, stops
capture, reaps the worker, joins readers, and settles Journal outcomes.

### Desktop interaction and delivery

A stable Dipper desktop identity owns its GlobalShortcuts session and Hold
action. Register before portal use and subscribe before binding. Configure
permissions before dictation, verify effective binding, and expose setup errors.
Use F2 as the initial configurable preferred key without remapping the keyboard.
KDE owns the effective assignment; GlobalShortcuts v1 settings link to KDE's
shortcut configuration. Existing Caps-to-Menu users may bind Menu explicitly.
Only the Hold key is used during pilot recording. Deactivated ends capture.
Unexpected session closure, connection loss, and observer failure interrupt
capture and revoke eligibility. Explicit close handles its own interruption
without waiting for a Closed signal. A visible recording state and Cancel
remain available if a physical release is missing; no unapproved duration cap
is introduced to disguise detection limitations.

A persistent, application-owned KWin observer starts before Hold admission.
It reports the active window UUID and PID and watches activation and window
removal. Authenticate its DBus sender against KWin's current unique owner.
Capture the intended T3 identity at admission. Observed departure, disappearance,
observer failure, or cancellation permanently revokes this dictation's permit.
Returning does not rearm it. Stop unloads only Dipper's script and unregisters
only Dipper's own portal resources; never start or stop unrelated scripts/apps.

At admission and dispatch, check the original KWin window plus the unique,
focused, editable, non-defunct T3 Tiptap/ProseMirror composer through AT-SPI.
Do not pin its object across recording, traverse unrelated apps, read window
titles, or persist drafts. Bound traversal and DBus waits; ambiguity or
unavailable evidence suppresses clipboard overwrite and insertion.

An owned Wayland ext-data-control helper serves the transcript clipboard until
ownership changes. Prepare native resources before dispatch; setting selection
is itself the clipboard mutation. A detached library call is insufficient for
owned cleanup. Check eligibility immediately before mutation and before Paste.
For this local pilot, use the already running ydotool daemon for Ctrl+V.
Its availability is a documented prerequisite, not an installed or restarted
service or a claim of portable production keyboard injection. Never send Enter.
Leave successful clipboard text available, including when the settings window
is hidden. No automatic restore, focus pull, or unclear-delivery retry occurs.

Record dispatch separately from verified target acceptance. OS send success is
unverified delivery. A bounded target check may confirm inserted text only with
actual composer evidence; unavailable evidence remains unverified. The accepted
OS race between checks, clipboard mutation, and Paste remains explicit.

The tray exposes status, Settings/Journal, Cancel, and Quit. Closing the settings
window hides it. Recording and processing have visible tray/status feedback.
The window lists input devices, model/runtime settings, the effective Hold key,
recent Journal outcomes, and explicit transcript Copy. Setup can be revisited
without stealing focus during an active dictation. Updaters, installers,
mobile UI, polished overlays, Replay, and final pilot acceptance are later work.

### Acceptance and review

Test through the Dictation lifecycle, Journal/audio, supervised worker protocol,
and desktop adapter interfaces. Use disposable data and deterministic adapters
for failure/race tests; they do not establish real PipeWire, Whisper/Vulkan,
portal, clipboard, or T3 integration. Verify those integrations separately on
this machine without stopping existing services. Required cases include busy,
repeat, cancellation, session/focus/device loss, queue/storage failure, stale
results, abnormal worker exit, parent loss, interrupted recovery, and no-text.
Test release before Ready, a request that never reaches Started, RF64 size
mismatches, resampling endpoint impulses and partial blocks, capture continuity
during model load, actual GPU identity, and worker disappearance. Verify the rendered UI and actual raw voice-to-T3 journey before claiming usable
delivery. Tie review and checks to the current revision and preserve unavailable
evidence as outstanding. Independent review precedes Ready implementation.

Independent review on 2026-10-14 resolved supervision deadline coverage,
resampler tail handling, protocol/log overflow distinctions, initial file and
directory durability, original-preserving recovery, orphan handling, schema
invariants, and observed discontinuity failure. No further owner decision
blocks implementation. These reviews establish contract readiness, not runtime
acceptance or deployment.

## Desktop interface polish contract, 2026-10-14

The owner requested a consistent app interface with a sidebar and the existing
Dipper logo. `ui/index.html` owns one shell and three local views: Dictation,
Journal, and Settings. `ui/styles.css` owns neutral dark/light tokens, spacing,
control geometry, and responsive sidebar behavior. `ui/app.js` owns local view
navigation; `ui/window.js` owns app window controls and host window state. Existing Tauri commands
and Dictation/Journal responsibilities remain the behavior boundary.

Dictation remains the primary screen. Status and Cancel stay available across
views. Settings retain their entered values on navigation and errors, use native
inputs, and expose runtime paths as advanced settings. Journal preserves separate
Audio/Recognition/Insertion outcomes and explicit Copy/recovery. No new recording
mode, retention policy, model, or insertion behavior is introduced.

Use root `Logo.png` as the canonical brand asset. The frontend build and native
window/tray icons derive from this asset rather than invent another mark. Keep
one quiet sidebar with labeled links at normal widths and an accessible icon rail
at narrow widths. Respect the requested sidebar instead of the skill's horizontal
navbar default. Use an underline for active navigation, restrained green accents,
system typography, and flat neutral surfaces. Use fixed dark appearance for content and app-owned window chrome.

Acceptance covers all three views, hash/back navigation, keyboard focus, unchanged
form drafts, loading/error/retry/copy/busy states, fixed dark rendering regardless of OS or old stored appearance, narrow/wide layouts and zoom, native window/tray
branding, and actual native controls. Browser fixtures establish frontend behavior
only; the real Tauri app owns native command evidence.

Reference workflows: [Handy documentation](https://handy.computer/docs) and
[history](https://handy.computer/docs/history) informed shortcut-first operation,
explicit transcript Copy, and device/paste recovery. Handy was not installed;
its full native journey was unavailable. The existing T3 interface provided an
installed reference for a quiet sidebar. Dipper keeps its own retention contract.

Native branding uses a cached 32 px logo with a recording/processing/error badge
and textual tray status. GTK's Wayland window identifies itself as `dipper` on
this host, so the development host creates a hidden `dipper.desktop` alias in
addition to its portal identifier. Existing installed or edited entries are
preserved. Recognized generated entries and brand assets use synchronized atomic
replacement. The frontend uses a symlink to canonical `Logo.png`; the actual
Tauri build was checked to embed and render it.

The owner extended this polish slice to app-owned window controls after providing
Sotto screenshots. Replace server decorations with app-owned themed window controls,
standard Minimize/Maximize/Restore and Close-to-tray buttons, a Tauri drag region,
and bounded resize edges. `ui/window.js` owns window presentation and delegates
to Tauri window APIs with narrowly scoped permissions for the main window.
Existing CloseRequested behavior still hides the window and preserves ongoing
Dictation. Controls use native button semantics, keyboard focus and explicit
names; the app owns their appearance. Recording, retention and target validation
remain unchanged. Reference images informed integrated chrome and compact
geometry, not new Dictionary/model-management features. Screenshots establish
visual reference only; the Sotto native workflow was unavailable.

Acceptance adds real native minimize/restore, maximize/restore, double-click,
move, edge resize, close/reopen from tray, keyboard operation, themes and narrow
geometry. Browser fixtures exercise presentation and errors only. Moving/resizing
is a host interaction; it does not mutate Dictation or steal focus elsewhere.

Owner revision: Dipper uses dark appearance only. Remove the appearance switcher,
Light/System tokens and persistence owner. `ui/styles.css` is the single fixed
palette for content and app-owned window chrome. This supersedes the earlier
three-mode polish requirement. Existing retained recordings/settings and window
behavior remain unchanged. Acceptance now checks no switcher and fixed dark
rendering regardless of old persisted choice or OS preference.

## Compact desktop and tray panel contract, 2026-10-14

The owner requires Auto recognition and a denser desktop/tray experience inspired
by the supplied Sotto screenshots. The useful screen keeps Hold and its key on
one row, a microphone link, and the actual last transcript with explicit Copy.
Remove decorative microphone space and repeated configuration summaries. Keep
Journal and Settings reachable, fixed dark appearance, and existing retention.
`ui/styles.css` owns compact shared geometry; a shared transcript presenter owns
preview and Copy for main and quick views. No unsupported feature controls.

The desktop host owns a lazy, frameless quick window. Left tray activation toggles
it; Escape, Close, and observed focus loss dismiss it. A small right-click menu
keeps Open Dipper, Cancel and Quit available. Use tray-icon 0.25.1's existing KSNI
backend to receive Linux activation through Tauri, preserving menu callbacks and
status updates. Scope quick-window capabilities separately. Opening/focusing
windows remains forbidden while Dictation is busy, and completion never opens one.
The quick panel shows actual state, Hold binding, selected microphone and retained
last transcript; Copy uses the existing explicit command. Open and Quit delegate
to established host behavior.

KDE tray activation supplies placement hints, but Wayland GTK cannot position a
normal window globally. A small desktop-shell placement helper may position only
Dipper's quick window, identified by owned PID and exact title, near the active
tray output using KWin. It owns its temporary script lifecycle and never changes
another window or the target-observer contract. If placement is unavailable, the
compositor-selected quick window remains dismissible. This is a KDE integration,
not cross-platform positioning evidence.

Acceptance: real Auto English/German transcription; compact main/quick views and
long/empty/error/busy states; native left/right tray paths, Copy, open/close,
Escape/outside dismissal and reopen; focus preserved during busy Dictation;
placement and multi-output limits; idle resource overhead measured. Preserve
existing recordings/attempts and settings. Technical evidence and remaining
owner judgment are recorded separately.

The tray implementation uses [the pinned KSNI backend](https://docs.rs/tray-icon/0.25.1/tray_icon/).
Short host show/focus actions use the same admission lock as Hold through
`Dictation::with_idle`; no slow work or await occurs inside that callback.
Placement tasks use an explicit Tauri runtime handle, request cancellation,
a serial cleanup gate, and a post-await generation/idle check. The quick window
starts non-focusable while positioning, then enables focus for interaction.
One-time dismissal handlers are installed on creation; initialization also
focuses the dismiss button so the first keyboard path does not depend on an
event arriving after the WebView has loaded. Temporary scripts use unique names,
bounded setup/operation/cleanup, authenticated replies, and reported file cleanup.

## Bounded composer qualification repair, 2026-10-14

Issue #16 retains DesktopAdapter's existing interface and insertion policy.
The reviewed repair replaces the per-node 650-object AT-SPI traversal with two
Collection.GetMatches calls on the application root resolved by the intended
T3 PID and unique bus owner. The root must have application role.

First query all descendants with a 4,097-reference sentinel and reject saturated,
foreign, null, or duplicate references. Then query FRAME and ENTRY roles with a
65-candidate sentinel. Reject saturation, duplicates, and candidates absent from
the first result. Read each candidate's role, classes, and states freshly. Require
the existing unique non-defunct frame and unique focused, editable, visible,
non-stale Tiptap/ProseMirror composer. Preserve KWin identity checks, the permanent
focus-loss latch, the shared four-second deadline, and one complete retry on
UnknownObject. No clipboard write or Paste is allowed on failed evidence.

The unfiltered sentinel is below the installed AT-SPI 2.58.9 provider's
65,536-child clamp; a filtered query alone cannot prove completeness. Queries
are observations rather than an atomic snapshot. A local timeout does not cancel
provider-side traversal. The existing accepted OS dispatch race remains.

Acceptance requires bounded-query regressions for saturation, foreign/duplicate
references and changed trees, existing invalid/ambiguous composer and retry tests,
actual installed T3 qualification, and native tray-hidden Hold through Paste
with synthetic content, Undo, and complete clipboard restoration. The independent
contract review confirmed wire types, sentinels, provider limits and ownership
on 2026-10-14 before implementation.

Primary references: [Collection protocol](https://github.com/GNOME/at-spi2-core/blob/2.58.9/xml/Collection.xml),
[provider traversal](https://github.com/GNOME/at-spi2-core/blob/2.58.9/atk-adaptor/adaptors/collection-adaptor.c),
and [provider limit](https://github.com/GNOME/at-spi2-core/blob/2.58.9/atspi/atspi-constants.h).

## Saved Hold restoration repair, 2026-10-14

Issue #18 belongs to the KDE DesktopAdapter shortcut owner. CreateSession and
ListShortcuts alone restore saved descriptions, not active KGlobalAccel actions.
Startup now lists the session, requires one nonempty existing hold action, binds
that same ID without a preferred trigger, and relists before exposing the binding.
Missing, empty, ambiguous or failed assignments remain unconfigured. Ordered
press/release subscriptions exist before binding; teardown and focus guards remain
unchanged. Core, Journal, and UI interfaces do not change.

This supersedes the earlier literal prohibition on startup BindShortcuts. The
installed KDE 6.7.5 backend returns immediately without a configuration dialog
when all requested IDs already exist. The portable portal API does not guarantee
that behavior; removal between listing and binding remains a backend race. New
assignments still require explicit configuration. Independent primary-source
review established this repair contract before implementation. Acceptance requires
a fresh process with no Configure Hold click, no dialog, actual press/release,
saved-key preservation, and safe missing/empty/ambiguous selection.

Source: [installed KDE registration path](https://github.com/KDE/xdg-desktop-portal-kde/blob/v6.7.5/src/globalshortcuts.cpp).

## Reviewed Turbo profile contract, 2026-10-14

The owner selected Whisper large-v3-turbo for live dictation. New Settings use
that profile and its float16 model path. Existing Settings and worker requests
without a profile deserialize as large-v3, so upgrading does not reinterpret
an existing path or historical attempt. The baseline constructor also retains
large-v3. Unknown profiles fail deserialization.

Settings owns the saved profile and path as one idle-only, atomic update.
Dictation snapshots both at admission, passes them through Recognition to the
single-job worker, and records that exact configuration in the Journal before
launch. The UI offers both profiles and reports the saved selection. A filename
never determines the model architecture.

The native bridge validates the selected profile's decoder depth, four layers
for Turbo and 32 for large-v3. Both retain 128 mel bins, 32 audio layers, 1280
state width, float16 weights, the pinned whisper.cpp revision, and the RX 7700S
Vulkan requirement. A mismatch fails preparation without inference or Paste.
Auto language, no translation, beam size five, eight threads, cancellation,
worker reaping, and insertion guards retain their existing contracts.

Acceptance requires legacy and new-settings coverage, profile persistence,
worker-launch and Journal configuration agreement, real Turbo recognition,
and a native mismatch failure. Native replay and rendered Settings evidence
must identify the tested revision; passing process fixtures alone does not
establish native model acceptance. Download, GGUF folder discovery, Reload,
and cleanup execution belong to later slices.

## Compact window header, 2026-10-15

Kyle requested removal of the duplicate Dipper label and empty top bar. The
sidebar keeps the single brand. Main-window controls share the existing page
header with the draggable view title and status. At widths of 520 CSS pixels
or less, status and Cancel use a second row to keep window controls accessible.
The separate 35-pixel titlebar is removed. Existing host window actions, close
to tray, resize edges and recording ownership remain unchanged.
