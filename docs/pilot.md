# Desktop pilot contract

Status: accepted pilot requirements; procedure and documentation review in progress. Kyle accepted the scope, recording modes, primary
target, corpus sizes, latency gate, light cleanup, baseline-first resource
evaluation, and reduced-retention comparison on 2026-10-13. Quality rejection rules and pilot storage-full behavior are also accepted. The
recording and measurement procedure is being reconciled in
[issue #1](https://github.com/KyleDerZweite/dipper/issues/1). The owner subsequently accepted the experiment as a provisional basis for app
design and useful implementation slices; see [development preparation](preparation.md).
Production choices and slice contracts remain explicit decisions.

## Accepted scope

The pilot targets Fedora KDE on Wayland with German speech and English technical
terms. Resource efficiency, performance, and quality are the product priorities.

Hold-to-record is mandatory. Toggle recording is desirable, not a pilot gate.
The primary insertion target is the T3 chat input. Browsers, Obsidian, and
terminals are additional validation targets whose compatibility must be checked
separately; success in T3 does not establish their behavior.

The core workflow includes dictation, insertion, configurable retention, and
explicit correction. Kyle's pilot requires comprehensive retention of capture
audio, recognition attempts, processing results, insertion outcomes, corrections,
and diagnostic logs. Other users must be able to retain less data. Product
defaults and detailed retention controls remain undecided.

Light cleanup adds punctuation and removes unambiguous filler words, while
preserving the intended meaning as far as possible. Strong rewriting is outside
the pilot. Assess changes to technical terms, names, numbers, and negations
explicitly rather than treating fluent output as proof of correctness.

A personal vocabulary list for expanding spoken abbreviations into configured
text is a desired later feature. It is not required for pilot acceptance.

Reference products supply optional feature ideas. Feature parity and mobile
implementation are outside this pilot. Mobile use remains a long-term ambition.

## Accepted spoken self-corrections

Apply clear spoken self-corrections directly during text processing. For example,
`Wir deployen Donnerstag, äh nein, Freitag` becomes `Wir deployen Freitag.`
The spoken self-correction replaces Thursday with Friday; it is not strong rewriting.

If the intended replacement or its scope is ambiguous, do not silently remove
or replace content. Preserve the meaning-bearing wording for user review.
Keep the literal reference transcript distinct from the expected processed text
so this intentional cleanup is not counted as a recognition error.

## App-start decision

The owner accepted the existing capture/recognition experiment as a sufficient
provisional baseline and ended further prototype work on 2026-10-13. Recordings
may accumulate incrementally; all 13 slots are not a prerequisite for app design
or useful implementation slices. Final pilot quality and workflow acceptance
remain separate from permission to start building. This baseline acceptance does not choose a model or toolchain. The subsequent
independent-app decision is recorded in [ADR-0001](adr/0001-own-dipper-app.md);
Rust/Tauri desktop is subsequently accepted in [ADR-0002](adr/0002-rust-core-tauri-desktop.md),
while mobile UI and remaining slice contracts are undecided.

## Accepted corpus

| Class | Recordings | Duration per recording | Purpose |
| --- | --- | --- | --- |
| Short | 4 | 10 to 30 seconds | Short prompts and startup overhead |
| Typical | 4 | 30 to 60 seconds | Common dictation |
| Longer | 4 | 90 to 120 seconds | Upper end of regular use |
| Brain dump | 1 | 5 minutes | Functional and resource stress |

Include English technical terms, numbers, negations, pauses, and spoken
self-corrections across the German recordings. Keep literal reference
transcripts distinct from processed text and confirmed wording. Store audio
and other private pilot data outside the Git checkout.

These classes reflect Kyle's estimated usage. They are not measured P95/P99
recording durations. The initial 13 recordings do not establish latency
percentiles or general model quality. Keep additional unseen validation
recordings separate from candidate tuning; their count remains open.

## Accepted latency gate

For recordings up to two minutes, the complete output must be present in the
T3 chat input within 30 seconds of releasing hold-to-record. Include all
recognition, text processing, and insertion after release in this measurement.
This is an outer tolerance ceiling, not a normal response-time target.

The five-minute brain dump is a functional and resource stress case. Report its
latency separately; the 30-second gate does not currently apply to this case.

Repeat warm measurements and report cold starts separately. The gate applies to both cold and warm runs for clips up to two minutes. Define
cold and warm conditions and repetition counts through the linked procedure before running acceptance. Report the sample size and individual
failures; do not turn the small corpus into a claimed P95/P99 latency result.

## Accepted retention comparison

Compare comprehensive retention with a reduced-retention configuration that does
not persist audio or detailed diagnostic logs, while retaining text and explicit
corrections. This is a pilot comparison, not the product default.

Private reference corpus audio remains available for repeatable evaluation
outside the application's retained history. Its use does not prove that the
application can replay audio omitted by its retention policy. Check that missing
playback and replay data are represented honestly. Keep measurement summaries
separate from detailed application diagnostic logs; report retained artifacts
and storage growth for each configuration.

## Evidence to collect

Record candidate versions, model configuration, CPU/GPU selection, recording
length, and retention policy for each run. Measure recording readiness,
release-to-insertion, correction effort, and time to accepted text separately.
Record raw recognition and processed output separately.

Measure CPU/GPU use, RAM/VRAM, idle overhead, and storage growth. Measure power
use where available and identify missing measurements. Compare resource costs
with observed latency and quality. First establish measured baselines, then
derive resource budgets. No hard resource budget has been accepted.

Check cancellation, retries, focus changes, recovery, export, and explicit
correction using retained data. Record failures and unavailable evidence. A
substitute text field does not establish insertion in the actual T3 application.

## Accepted quality and storage-full rules

Changed negations, incorrect numbers or names, invented content, or silently
resolved ambiguity fail a test case. Punctuation and harmless spelling variants
may differ. Assess the actual spoken content, including intended corrections.

For the pilot, insufficient storage clearly rejects a new recording and
preserves existing data. Do not automatically delete old data to make room.
Report storage failure during capture or processing separately and preserve
available data without claiming a complete record.

## Deferred design and calibration

Product retention defaults, storage limits, policy changes, and detailed
crash-loss/recovery design belong to issue #4. Comprehensive retention is not
a commitment to unlimited storage. Measure storage growth before choosing
future configurable limits.

Recording readiness, correction effort, and time to accepted text are measured
baselines in this first pilot; no additional numerical limits are set. The
accepted release-to-insertion and quality gates remain binding. The unseen
validation set is deferred until its scope is agreed and is required before
using tuning results as validation evidence.

## Recording and measurement procedure

Use the [pilot procedure](pilot-procedure.md) for corpus slots, quality examples,
cold/warm definitions, run counts, and evidence collection. The procedure separates acceptance gates, observational evidence, and deferred
design work.

## Ownership and sequence

[Issue #1](https://github.com/KyleDerZweite/dipper/issues/1) owns the completed
acceptance contract. [Issue #2](https://github.com/KyleDerZweite/dipper/issues/2)
owns corpus preparation and the reproducible procedure, which can proceed from
the accepted scope while the remaining acceptance criteria are refined.
[Issue #3](https://github.com/KyleDerZweite/dipper/issues/3) owns the candidate
comparison. [Issue #4](https://github.com/KyleDerZweite/dipper/issues/4) owns
foundation selection and reviewed component contracts. Issue #4 is no longer blocked by completion of the full comparison in #3.
Keep final corpus/comparison evidence open, and make each implementation slice
ready only when its own reviewed contract is coherent.
