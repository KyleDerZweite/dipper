# App foundation and module design

Status: proposal for owner discussion, not an implementation contract.
The [preparation plan](preparation.md) records the accepted permission to start
app work from provisional evidence. Issue #4 owns foundation and module design;
issue #5 owns implementation breakdown. The owner selected an independent app in [ADR-0001](adr/0001-own-dipper-app.md);
language and UI stack remain TBD. No proposed slice below is Ready.

## Evidence and limits

The preserved local experiment at `prototype/native-recorder`, revision
`3436eb18`, demonstrated capture from the actual USB microphone and local
recognition with Whisper/whisper.cpp/Vulkan and Qwen3-ASR on CPU. Its README,
TRANSCRIPTION, and EVALUATION records own exact results and limitations.
The prototype is not a production foundation and remains unpublished.

Global KDE Wayland hold-to-record, real T3 insertion, production journal
consistency and recovery, and the release-to-insertion gate remain unverified.
App work may address these gaps while additional recordings accumulate.

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
or a language choice. Refine each useful slice into a short reviewed contract.

## Proposed useful slices

1. Production capture: real hold-to-record, visible readiness and cancellation,
   durable recording identity, complete audio, latest-recording playback, and the
   accepted storage-full behavior. Verify on KDE Wayland, including interruption.
2. Voice to T3: an explicitly chosen recognition configuration and actual insertion
   in T3, with separate attempts and honest focus-change/failure outcomes. Raw
   output alone does not satisfy the pilot cleanup requirement.
3. Pilot processing and retention: light cleanup, clear spoken self-corrections,
   explicit correction, and both retention configurations. Verify meaning-bearing
   facts, ambiguity, and honest unavailable playback/replay data.
4. Journal use: search, replay, and consistent export using retained data, followed
   by end-to-end pilot acceptance and resource measurement.

Slice size and integration order remain proposals until foundation and interfaces
are accepted. Keep one PR per useful slice, as already agreed. No full-corpus
completion gate applies to starting these slices, but their own acceptance
checks and dependency directions still govern readiness.

## Consequential choices to resolve

- Choose the maintained language, UI stack, and license for the accepted own-app
  foundation. Existing runtimes can be reused without adopting prototype structure.
- Select the first live recognition configuration, model lifetime, cancellation,
  and process strategy. Existing Whisper timing supports discussion of a starting
  baseline, not an automatic selection or overall quality ranking.
- Specify the first slice's durable completion checkpoints, crash-loss limit,
  recovery actions, and reduced-retention semantics. SQLite with separate audio
  remains a candidate. Never infer an atomic transaction across those artifacts.

Record consequential accepted choices in `docs/adr/` when agreed. Keep unaccepted
ideas here and in issue discussion. Do not create Ready implementation tickets
from this proposal alone.
