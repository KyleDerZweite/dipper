# Desktop pilot contract

Status: partial contract. Kyle accepted the scope, recording modes, primary
target, corpus sizes, and latency gate on 2026-10-13. Quality behavior,
resource-use acceptance, and reduced-retention scenarios remain open in
[issue #1](https://github.com/KyleDerZweite/dipper/issues/1). This document does
not authorize implementation or select a foundation.

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

Reference products supply optional feature ideas. Feature parity and mobile
implementation are outside this pilot. Mobile use remains a long-term ambition.

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
cold and warm conditions and repetition counts before running acceptance. Report the sample size and individual
failures; do not turn the small corpus into a claimed P95/P99 latency result.

## Evidence to collect

Record candidate versions, model configuration, CPU/GPU selection, recording
length, and retention policy for each run. Measure recording readiness,
release-to-insertion, correction effort, and time to accepted text separately.
Record raw recognition and processed output separately.

Measure CPU/GPU use, RAM/VRAM, idle overhead, and storage growth. Measure power
use where available and identify missing measurements. Compare resource costs
with observed latency and quality; no hard resource budget has been accepted.

Check cancellation, retries, focus changes, recovery, export, and explicit
correction using retained data. Record failures and unavailable evidence. A
substitute text field does not establish insertion in the actual T3 application.

## Open acceptance decisions

- Allowed text processing and concrete quality acceptance examples.
- Whether the first comparison establishes resource baselines before hard
  budgets are chosen.
- Reduced-retention behavior to exercise alongside comprehensive retention.
- Cold/warm definitions and repetition counts.
- Recording-readiness and correction acceptance limits, crash-loss and
  storage-full behavior, and the private corpus location.

## Ownership and sequence

[Issue #1](https://github.com/KyleDerZweite/dipper/issues/1) owns the completed
acceptance contract. [Issue #2](https://github.com/KyleDerZweite/dipper/issues/2)
owns corpus preparation and the reproducible procedure.
[Issue #3](https://github.com/KyleDerZweite/dipper/issues/3) owns the candidate
comparison. [Issue #4](https://github.com/KyleDerZweite/dipper/issues/4) owns
foundation selection and reviewed component contracts. Keep their native
GitHub dependencies and avoid marking dependent implementation ready while
its contract is incomplete.
