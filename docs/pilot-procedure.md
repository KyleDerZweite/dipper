# Pilot recording and measurement procedure

This procedure prepares the [accepted desktop pilot](pilot.md). The corpus can
be recorded from the accepted scope. Issue #1 now owns review of the bounded
contract; issue #3 requires that review and the completed corpus. Later resource
calibration and detailed recovery design belong to issue #4. No candidate, toolchain, or resource budget is selected here.

## App development can proceed

This procedure owns corpus preparation and final evaluation evidence, not
permission to begin app development. The owner accepted the current experiment
as a sufficient provisional baseline and ended further prototype work.
Recordings can accumulate when useful; completing all 13 slots is not an
app-design or implementation-start prerequisite. Keep final acceptance distinct
from this permission, as recorded in [development preparation](preparation.md).

## Corpus preparation

Kyle records natural German speech with English technical terms on the target
workstation. Use the prepared private location `~/.local/share/dipper/pilot/`, or an existing
private location outside the checkout if Kyle prefers. The prepared location
contains private recording and evidence references. The original template alone
was not completion evidence; the current experiment has partial S01/S02 coverage. Record
its reference, microphone, capture format, and actual clip durations in the
private corpus manifest. Do not place audio, personal dictation, or raw logs in
this public repository.

Use the 13 slots below. Prompts are synthetic topic cues, not reference
transcripts. Speak naturally, meet the duration band, and then transcribe what
was actually said. Include audible fillers and self-corrections in the literal
reference. Write expected cleanup separately; do not manufacture a perfect
reference from the prompt.

| ID | Duration | Recording cue and coverage |
| --- | --- | --- |
| S01 | 10 to 30 seconds | Ask T3 to inspect a failing build and the last commit. Use English terms such as build and commit. |
| S02 | 10 to 30 seconds | Say that a migration must not run automatically. Include a clear negation and a version number. |
| S03 | 10 to 30 seconds | Include the accepted correction: `Wir deployen Donnerstag, äh nein, Freitag`. Add one related sentence. |
| S04 | 10 to 30 seconds | Describe two alternatives without deciding: `Vielleicht Donnerstag, vielleicht Freitag`. Do not invent a correction cue. |
| T01 | 30 to 60 seconds | Ask for a small refactor with a constraint against changing the public API. Include a natural pause. |
| T02 | 30 to 60 seconds | Describe a synthetic bug with steps, expected behavior, and actual behavior. Include a number and a clear correction. |
| T03 | 30 to 60 seconds | Dictate a short message mentioning fictional names, dates, and English project terms. Include unambiguous fillers. |
| T04 | 30 to 60 seconds | Explain an unresolved choice between two approaches. Include uncertainty and a negation that must remain. |
| L01 | 90 to 120 seconds | Describe a synthetic project plan, its constraints, and what is explicitly outside scope. |
| L02 | 90 to 120 seconds | Explain a multi-step bug investigation with English technical terms and several numeric values. |
| L03 | 90 to 120 seconds | Dictate a review comment with two clear self-corrections, natural pauses, and a rejected alternative. |
| L04 | 90 to 120 seconds | Discuss competing design ideas, preserving uncertain conclusions rather than resolving them. |
| B01 | 5 minutes | Give a natural synthetic project brain dump with topic changes, pauses, technical terms, numbers, negations, and self-corrections. |

For each recording, preserve source audio, actual duration, literal reference,
expected cleanup notes, and the meaning-bearing facts to check. Preparation and size of an additional unseen validation set are deferred. If
candidate tuning begins, agree and isolate that set before using its results
for validation; it is not part of the 13-slot preparation completion gate.
Corpus preparation is complete when all 13 slots have actual recordings and
reviewed literal references in the private store. A public manifest may report
IDs, durations, coverage, and protected storage references, without private data.

## Quality examples

The clear correction example is accepted. The other examples below operationalize
light cleanup and ambiguity preservation; their exact output spelling is an
illustration, not a requirement to match every character.

| Spoken content | Expected treatment |
| --- | --- |
| `Wir deployen Donnerstag, äh nein, Freitag` | Produce `Wir deployen Freitag.` |
| `Vielleicht Donnerstag, vielleicht Freitag` | Keep both possibilities; do not choose a date. |
| `Die Migration darf nicht automatisch laufen` | Preserve the prohibition. |
| `Erhöhe das Limit auf fünfzehn, nein, fünfzig` | Preserve the clear revised value of fifty. |
| `Ähm bitte prüf den Build und den letzten Commit` | Remove the unambiguous filler and add punctuation without rewriting the request. |

Accepted owner quality rule: changed negations, incorrect
numbers or names, unsupported facts, or silently resolved ambiguity fail the
case. Harmless punctuation and spelling variants may differ. Record raw
recognition errors separately from cleanup errors and explicit user corrections.
Measure correction effort; do not infer acceptance from fluency alone.

## Measurement sequence

1. Prepare the private corpus and references without selecting a foundation.
2. Once candidate configurations are agreed, replay the same corpus for initial
   recognition comparison. Record versions and settings. This establishes
   recognition evidence only, not hold-to-record or T3 insertion compatibility.
3. For the bounded end-to-end comparison, run one cold and three warm measurements
   per corpus slot and retention configuration for the selected finalist. If
   more configurations reach this phase, report their number and total runs.
4. Exercise hold-to-record, release, and insertion in the actual T3 chat input.
   Record live input and actual duration; repeated live dictation is not identical
   audio. Create and confirm a literal reference and meaning checks for each
   actual live recording; do not score it against an earlier replay transcript.
   Report replay and live results separately. Do not submit the message.
5. Run both comprehensive retention and the accepted reduced-retention comparison.
   Keep private evaluation audio separate from retained application history.
6. Report every regular clip against the 30-second gate, including cold runs.
   Report the five-minute case separately without applying that latency gate.

A cold run starts from a stopped candidate process without its model already
resident. Verify the candidate state where possible; record OS file-cache state
as unknown if it cannot be established. This does not imply a cold OS or reboot.
A warm run follows an unscored warm-up using the same configuration and keeps
its model resident. Record lazy loading or eviction that invalidates the label.
The initial repeat counts are procedure choices, not statistical confidence.

Use monotonic timing where instrumentation is available. Measure release to
complete insertion, not recognition completion alone. If instrumentation is
missing, record the observation method and resolution. Use screen evidence or
operator observation to corroborate insertion in T3.

Record readiness, recognition, processing, insertion, correction effort, and
time to accepted text separately where observable. Record CPU/GPU selection,
CPU/GPU use, peak RAM/VRAM, idle overhead, and storage growth. Take an idle
baseline before recording and report the sampling interval and process scope.
Measure power use where available; mark unavailable measurements explicitly.
Budgets follow measured baselines rather than guessed limits.

## Retention and failure checks

Check which artifacts exist after success, cancellation, retry, explicit
correction, and export. Under comprehensive retention, later attempts must not
replace earlier attempts. Under reduced retention, persistent application audio
and detailed diagnostic logs must be absent, while text and corrections remain.
Do not infer disabled collection merely from absent retained files.

Check focus changes and failed insertion without submitting text to T3. Report
missing playback or replay data honestly. Failure records must distinguish
cancellation, recognition failure, processing failure, and insertion failure.

For the pilot, insufficient storage must clearly reject a new recording and
preserve existing data. Do not automatically purge old records to make room.
If storage is exhausted during capture or processing, report the failure and
retain available data without claiming completeness. Record this separately
from rejecting a recording before capture.

Future configurable storage limits and detailed crash-loss/recovery design
remain in issue #4. Comprehensive retention does not promise unlimited storage
or hundreds of gigabytes of growth. Measure storage costs to inform later limits. Do not run destructive disk
filling or delete real data during corpus preparation. Later failure simulation
must use an isolated candidate data store. Record unexecuted recovery checks as
unexecuted rather than passed.

## Results and handoff

Keep detailed run records private, including clip ID, candidate revision, model
settings, retention configuration, cold/warm state, actual duration, timings,
resource measurements, output, error classification, and correction notes.
Publish a sanitized summary with run counts, failures, median and observed
maximum latency, resource baselines, retention behavior, and unknowns. Do not
claim P95/P99 from this small pilot.

Issue #2 owns corpus preparation. Issue #3 owns comparison and acceptance
execution once issues #1 and #2 are complete. Component design and foundation
selection stay in issue #4. Text replacement in issue #8 remains deferred.
