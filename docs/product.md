# Product direction

This document records the current product direction. The broader first-release feature set and some acceptance targets remain open. The [desktop pilot contract](pilot.md) records the accepted initial scope and latency gate.

## Accepted pilot scope

On 2026-10-13, Kyle selected the core shortcut, dictation, and insertion workflow with configurable data retention and explicit correction for the first pilot. Kyle's pilot uses comprehensive retention of recordings, recognition attempts, processing results, corrections, and diagnostic logs. Other users must be able to retain less data. Feature parity is not a project requirement. Wispr Flow and Superwhisper are sources of optional ideas; select features only when they serve Dipper's workflow. T3 is the primary target, hold-to-record is mandatory, and the accepted latency ceiling is 30 seconds for recordings up to two minutes. The [pilot contract](pilot.md) records the accepted quality rules, baseline-first resource evaluation, and pilot storage-full behavior. Detailed recovery design and future retention limits belong to issue #4.

## Current development direction

The owner accepted the recorder/transcription experiment as a sufficient
provisional baseline on 2026-10-13. Proceed toward the real app and stop further
prototype iterations. Additional recordings can be collected incrementally;
corpus completion is not an app-start gate. The [preparation plan](preparation.md)
records the preserved evidence and separate final acceptance work. The subsequent foundation decision is an independent Dipper app, recorded in
[ADR-0001](adr/0001-own-dipper-app.md). Production language, UI stack, storage,
and model configuration remain TBD.

## Platform direction

Desktop is the initial focus, starting with Fedora KDE on Wayland. Phone and other mobile use are a long-term ambition, well beyond the initial desktop work. Mobile platforms and delivery plans remain undecided.

## First app delivery

The first usable slice runs in the background/tray and delivers the raw
hold-to-record, capture, recognition, and T3 insertion chain. UI polishing is
deferred. Use the system default input by default and list devices for explicit
selection. Resolve the input at recording start and keep it fixed; device loss fails the
recording while preserving available audio. A focus change away from T3 prevents
automatic insertion and leaves the transcript available for explicit use.

Whisper large-v3 is the first live baseline. Qwen is excluded from active
candidates; historical experiment evidence remains preserved. Long-term
React Native versus Rust toolchain research must include resource efficiency,
mobile support, and OTA update mechanisms. Detailed behavior and unresolved choices belong to the
[app design record](app-design.md). Cleanup remains part of the overall pilot,
not a requirement to add before the first raw-chain slice.

## Intended workflow

Press a shortcut, speak, and insert usable text into the intended application.
Recording state and cancellation should be clear. The daily target apps include
T3, browsers, Obsidian, and terminals.

A second shortcut should open the latest dictation for playback and explicit
correction. When retained, a searchable journal connects its audio, raw transcript, processed
text, insertion outcome, and confirmed wording. Available playback and replay
depend on the retained data. Reprocessing and analysis run
separately from normal dictation so experiments do not delay the daily workflow.

## Product priorities

- Resource efficiency: assess CPU/GPU use, memory, idle overhead, and storage
  growth across recognition, processing, and the chosen retention policy.
  Measure power use where the target environment permits it.
- Performance: measure recording readiness, recognition, processing, and
  insertion through the whole workflow, including cold and warm starts.
- Quality: preserve meaning, technical vocabulary, names, numbers, and negations,
  and measure the effort needed to reach accepted text.

The first resource comparison establishes baselines; budgets follow measured
results. The pilot uses light cleanup, including clear spoken self-corrections, rather
than strong rewriting. Ambiguous corrections must not silently remove content.

Data retention is configurable. Kyle wants comprehensive history for analysis
and model comparison; other users may choose less retention. Retention defaults,
limits, and deletion behavior remain open. Optional features must support these
priorities and concrete user scenarios.

German dictation with English keywords needs its own evaluation. Manufacturer
benchmarks and performance on other hardware do not establish Dipper's quality
or latency on the target PC.

## Optional feature ideas

Wispr Flow and Superwhisper inspired some of the ideas below. They are not a
parity checklist or a commitment to implement every feature. The accepted pilot
scope above owns mandatory capabilities; additions require an explicit scope
decision. Candidate areas include:

- Configurable shortcuts, hold and toggle recording, clear recording feedback,
  cancellation, and reliable insertion.
- Raw transcription and processed output with punctuation, paragraphs, lists,
  filler-word handling, and spoken self-corrections.
- Personal vocabulary, reviewed spelling corrections, replacements, and snippets.
  Kyle wants configurable expansion of spoken abbreviations into replacement
  text as a later feature, not a pilot acceptance requirement. Track it in
  [issue #8](https://github.com/KyleDerZweite/dipper/issues/8).
- Modes for raw text, messages, email, and technical prompts, with appropriate
  model, language, and processing settings.
- Context-aware output and editing selected text through a shortcut or spoken
  instruction, with review and a way to undo changes.
- Playback, explicit correction, journal search, export, and replay through
  different model configurations.

Optional references include Wispr's
[formatting](https://docs.wisprflow.ai/articles/5373093536-how-do-i-use-smart-formatting-and-backtrack),
[dictionary](https://docs.wisprflow.ai/articles/4052411709-Teach-Flow-your-words-with-the-dictionary),
[snippets](https://docs.wisprflow.ai/articles/5784437944-create-and-use-snippets),
[context](https://docs.wisprflow.ai/articles/4678293671-Context-Awareness),
[transforms](https://docs.wisprflow.ai/articles/8068950331-how-to-use-transforms-beta),
and [command mode](https://docs.wisprflow.ai/articles/4816967992-how-to-use-command-mode),
plus [Superwhisper's custom modes](https://github.com/superultrainc/superwhisper-docs/blob/main/modes/custom.mdx).
Language, platform, and editor support differ between these features. Automatic
field-context access and final-text collection are unverified on KDE Wayland.

Mobile implementation is deferred to the long term. Server deployment, fine-tuning, unattended correction
learning, and a general voice assistant are outside the proposed initial pilot.
Sync, meeting notes, and team features need explicit scope decisions if selected. OpenWhispr was rejected for workflow and quality.
