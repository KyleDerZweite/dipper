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
framework remain deferred choices. Model lifetime, insertion verification, and
the Journal implementation details still need decisions. SQLite with separate
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

For the first slice, insertion may use Clipboard plus Paste while the intended
T3 target remains valid. Leave the transcript in the clipboard after insertion;
do not automatically restore its former content. If focus changes away from T3,
do not automatically insert or overwrite the clipboard merely to make the
transcript available. Keep it in the journal for explicit use instead.
Clipboard transfer and input-event delivery do not confirm that the target field
accepted the text. Preserve an unverified outcome until actual target evidence
is available, and do not automatically retry an unclear insertion.

## Open model lifetime

The owner does not want the model held in RAM/VRAM indefinitely. No model-lifetime
policy is accepted yet. Investigate on-demand loading and prompt unloading versus
a bounded idle grace period, using existing timing evidence before choosing a
default. Do not infer resident-worker performance from one-shot CLI runs or
resume prototype iterations to settle this decision. The app's actual lifecycle
will need target measurements during implementation. The
[model-lifetime investigation](model-lifetime.md) records existing load-time
observations and the proposed on-demand worker policy without claiming new
benchmark evidence.

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

## Consequential choices to resolve

- Resolve frontend needs, packaging, and license for the accepted Rust/Tauri
  foundation. Existing runtimes can be reused without adopting prototype structure.
- Define model lifetime, cancellation, and process strategy for the accepted
  Whisper large-v3 live baseline. Qwen is not an active candidate. Other models
  can be evaluated later without blocking app progress.
- Specify the first slice's durable completion checkpoints, crash-loss limit,
  recovery actions, and reduced-retention semantics. SQLite with separate lossless audio is accepted in ADR-0003. Never infer an
  atomic transaction across those artifacts.

Record consequential accepted choices in `docs/adr/` when agreed. Keep unaccepted
ideas here and in issue discussion. Do not create Ready implementation tickets
from this proposal alone.
