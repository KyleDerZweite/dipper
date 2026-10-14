# App foundation and module design

Status: proposal for owner discussion, not an implementation contract.
The [preparation plan](preparation.md) records the accepted permission to start
app work from provisional evidence. Issue #4 owns foundation and module design;
issue #5 owns implementation breakdown. The owner selected an independent app in [ADR-0001](adr/0001-own-dipper-app.md);
Rust core and Tauri desktop are accepted in [ADR-0002](adr/0002-rust-core-tauri-desktop.md).
The detailed first-slice contract is not Ready.

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

Whisper large-v3 through whisper.cpp/Vulkan on the RX 7700S is the accepted first
live baseline. Evaluate other configurations later without treating this as a
final model ranking. Qwen is excluded from active candidates by owner decision:
the observed CPU startup-inclusive latency exceeded the tolerable wait, despite
useful experimental quality. Preserve its historical evidence and installed
artifacts; do not delete or reinterpret them as a failed capture.

If the user switches away from T3 before insertion, do not insert automatically
or pull focus back. Retain the transcript for explicit copying or later insertion.
An unclear insertion outcome must not trigger an automatic duplicate retry.

Use a Rust core and Tauri 2 desktop shell. Keep core behavior behind interfaces
that do not depend on Tauri or desktop controls. Mobile UI and the frontend
framework remain deferred choices. Insertion verification and detailed Journal
contracts still need decisions. SQLite with separate
lossless audio and interrupted-recording recovery are accepted in ADR-0003. Long-term update requirements include both
desktop whole-app delivery without manual downloads and compatible mobile
UI/JS updates without a new store release. Native-code changes and model-weight
updates require separate versioning and delivery; update implementation is not
added to the first raw-chain slice by this requirement. The scope is accepted; the implementation contract is not Ready.

## Accepted persistence and initial insertion

Use SQLite for journal metadata and separate lossless audio files, as recorded
in [ADR-0003](adr/0003-journal-storage.md). Write audio during capture and preserve
available data on interruption. Recovery marks unfinished recordings as
interrupted and performs no automatic insertion after restart. Detailed schema,
audio codec, commit checkpoints, and numerical crash-loss bounds remain open.

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
guard also required KWin's active-window identity. Production target validity,
focus-loss invalidation, and dispatch race behavior remain unverified.
The owner accepted the conservative OS guard above, including its residual
race. Finalize its observer, invalidation, and dispatch interfaces before
marking the automatic insertion contract Ready. A cooperative T3 endpoint is
an optional later mechanism, not an assumed capability or pilot prerequisite.

Remaining design work includes target identity and focus-loss invalidation,
permission/session lifetime, lost-release handling, protocol limits and
supervision deadlines, and audio synchronization cadence. Crash-loss policy is
accepted; its observed loss still needs measurement rather than a guessed bound.
Runtime acceptance belongs to the production slice. The native report now
includes explicitly authorized live focus and Paste probes; no new
microphone/model runs, production permission setup, or storage/worker fault
tests through Dipper were executed. Separate disposable SQLite/audio fixtures
now exercise storage ordering and limited-database failure; they are not the
Journal implementation.

## Consequential choices to resolve

- Resolve frontend needs, packaging, and license for the accepted Rust/Tauri
  foundation. Existing runtimes can be reused without adopting prototype structure.
- Finalize supervised-worker cancellation, busy behavior, and stale-result
  suppression for the accepted on-demand Whisper large-v3 worker. Qwen is not an
  active candidate. Other models can be evaluated later without blocking app progress.
- Specify the first slice's durable completion checkpoints, measured crash loss,
  recovery actions, and reduced-retention semantics. SQLite with separate lossless audio is accepted in ADR-0003. Never infer an
  atomic transaction across those artifacts.

Record consequential accepted choices in `docs/adr/` when agreed. Keep unaccepted
ideas here and in issue discussion. Do not create Ready implementation tickets
from this proposal alone.
