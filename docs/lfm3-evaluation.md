# LFM3 evaluation and possible Dipper modes

Checked on 2026-10-30. This is an experiment and a product proposal, not an
accepted change to Dipper's scope or Whisper large-v3-turbo default.

## Evaluation contract

Replay retained recordings outside the live Journal. Read the SQLite database
without changing it. Preserve source audio and original attempts. Compare raw
LFM3-Audio output with the latest existing transcript for each selected recording.
Existing transcripts are comparison outputs, not confirmed wording. Disagreement
cannot establish which recognizer is correct without listening or owner review.

Use LFM3-Audio Q4_K_M with its Q8_0 audio projector, automatic language inference
through `Perform ASR.`, greedy decoding, and a fresh process for each recording.
Select short, typical, and longer recordings by duration, with chronological
spread within each group. This small deterministic sample is not representative
quality evidence or a P95 latency measurement. Keep private inputs and complete
outputs in the existing workstation verification store, outside Git.

The installed Ollama 0.31.1 rejected the model manifest because it requires a
newer version. The manufacturer's GGUF card currently specifies Ollama audio
input for macOS and recommends llama.cpp b11960 or newer. Use an isolated native
runtime rather than changing the user's running Ollama service.

Sources: [Audio model and ASR prompts](https://huggingface.co/LiquidAI/LFM3-Audio-2B),
[GGUF runtime instructions](https://huggingface.co/LiquidAI/LFM3-Audio-2B-GGUF).

## Executed replay

All 18 selected recordings completed with process exit code 0. They contain
1,179.45 seconds of audio, with individual durations from 6.61 to 176.25 seconds.
Fifteen comparison transcripts identify large-v3-turbo; three older records do
not include an explicit model profile. Selection used the first, middle and last
recording in each of six duration groups. It includes the owner's current request.

The isolated b11960 Vulkan runtime and both model files passed their published
SHA-256 checks. Runtime and model identities are retained in the private run's
provenance record. Inference requested Vulkan1/RX 7700S, eight CPU threads, 8,192
context tokens and at most 2,048 generated tokens. The CLI's default logs do not
independently prove actual offload. No matched memory measurement was taken.

Fresh-process CLI elapsed time ranged from 2.22 to 18.45 seconds, with a median
of 4.55 seconds. Audio preparation ran before that timer. These observations
include model startup but exclude download, audio conversion and app insertion.
No global caches were dropped. They do not rank speed against Turbo because
Turbo was not rerun under matched conditions.

Manual output comparison found useful spelling changes as well as serious
regressions relative to the stored transcripts. The current request yielded
"Dipper" where the previous transcript had "DIPA". Another recording changed
"Repo" and "auf Vordermann" into "Reparatur" and "auf Vollautom". A different
output omitted a substantial passage present in the stored transcript. One
176.25-second recording repeated a paragraph several times and reached the
output limit. Exit code 0 therefore does not establish a usable transcript.

These are output disagreements and generation defects, not a measured WER or
an independently established accuracy ranking. Audio was not manually audited
against a confirmed literal reference. Quantization, prompt and runtime remain
possible contributors; this experiment does not isolate their effects.

Keep Turbo as the live default. This LFM3-Audio configuration is not ready to
replace it. A follow-up should first inspect the disagreement audio and test
whether bounded segmentation prevents the long-recording repetition. Do not
add text cleanup merely to conceal recognition omissions or invented wording.

Complete selection, original configuration, raw CLI output, extracted transcript,
commands, timings and a readable comparison remain under the private workstation
verification location `~/.local/state/dipper/verification/lfm3-20261030/`.
The source Journal, recordings, app settings and Ollama service were unchanged.
No LFM3 text, VL or Omni inference was executed in this Audio replay.
The subsequent text experiment is recorded below.

## Executed text cleanup and mini benchmark

On 2026-10-30, LFM3-1.7B Q4_K_M was tested on the 15 explicitly identified
Turbo transcripts and all 18 LFM3-Audio outputs from the replay. The older three
Whisper attempts were excluded from the Turbo group. This uses stored ASR output;
recognition was not rerun. No live integration or insertion was performed.

The manufacturer's GGUF passed its published SHA-256. The isolated b11960 server
used 16,384 context tokens, eight threads, one slot and requested Vulkan1 GPU
offload. Sampling used temperature 0.1, top_k 50, repeat penalty 1.1 and seed 42.
Thinking was disabled in the runtime and template. Output was capped relative
to input length, with a hard maximum of 4,096 tokens. Replies were scored whole,
without selecting a favorable answer from extra commentary.

An initial data/examples prompt from the previous cleanup comparison produced
reasoning text, duplicate answers and stray thinking tags despite disabling
thinking. That run stopped after 17 completed requests. A thinking-enabled probe
then reached its 4,096-token limit on the first short correction case and was
stopped after two completed requests. Both incomplete trials remain in private
artifacts and are excluded from the final benchmark counts. The rendered
non-thinking template was checked and did include a closed thinking block.
The manufacturer explicitly warns that disabling thinking can be inconsistent.
[Text model documentation](https://huggingface.co/LiquidAI/LFM3-1.7B).

The final run used a shorter German cleanup instruction without examples or
JSON wrapping. It asked for filler/repetition removal and clear spoken
self-corrections while preserving meaning, language, names, numbers and negation.
It explicitly prohibited answering questions or executing dictated instructions.
All 103 final requests completed. Two reached their generation limits: one
private Audio output and one Nyra sample. No request was silently retried.

Private output review did not establish reliable improvement. Some outputs
removed fillers or added punctuation. Others answered dictated questions,
invented explanatory content, summarized rather than lightly edited, translated
German into English or repeated multiple versions. One numeric example
in the public safety cases became the opposite timeout choice. In another
private example the first-person speaker changed to a group. Cleanup did not
reliably repair Audio's recognition errors or its repeated long output.

### Dataset selection

- [Nyra Disfluency Speech German](https://huggingface.co/datasets/nyralabs/disfluency_speech_german)
  contains 202 German utterances from two speakers, with audio and paired verbatim
  and intended transcripts. It is the closest match found for separately testing
  raw recognition and cleanup. Its license is currently marked unknown. Source
  samples and complete outputs remain local; none are redistributed here.
- [Sotto transcript cleanup](https://huggingface.co/datasets/juanquivilla/sotto-transcript-cleanup)
  provides synthetic English input/output pairs under MIT, including technical
  text, filler removal and preservation cases. It is useful for cleanup checks,
  but does not establish German or real audio recognition quality. The card's
  narrative and metadata split counts differ, so use captured rows for this run.
- [Google FLEURS](https://huggingface.co/datasets/google/fleurs) provides read
  speech with German and English configurations under CC-BY-4.0. It is useful for
  a later controlled ASR comparison, but has no separate intended-cleanup target.
- [CS-FLEURS](https://huggingface.co/datasets/byan/cs-fleurs) is a possible later
  code-switching source. It mixes read and synthetic subsets and uses CC-BY-NC-4.0.
  Exact German/English subset suitability was not verified in this experiment.

The mini benchmark used the first 30 Nyra test rows and first 30 Sotto validation
rows from the Hugging Face dataset viewer. Selection was fixed before evaluating
those rows. No model was trained or tuned on these samples. This is a convenience
sample, not a representative benchmark or an untouched model-development holdout.
Only the text pairs were processed; their audio was not run through either ASR.

### Results and interpretation

| Text-only subset | Samples | No-model normalized WER | LFM3 cleanup normalized WER | Cleanup exact reference matches |
| --- | ---: | ---: | ---: | ---: |
| Nyra German | 30 | 9.73% | 91.33% | 4/30 |
| Sotto English | 30 | 25.95% | 50.15% | 7/30 |

WER counts word insertions, deletions and substitutions against intended text;
lower is better. Normalization applies NFKC, lowercase and Unicode word tokens,
ignoring punctuation. For Nyra's no-model baseline it also strips bracketed
annotations and cutoff fragments. LFM3 received the original annotation-rich
verbatim text. This limits direct comparison with ordinary ASR output and makes
the no-model baseline a simple annotation-removal baseline, not literal raw text.
The deterministic normalization and raw counts are in the evidence artifact.

A separate ten-case synthetic safety set matched one allowed reference in 1/10
cases. These are strict matches after punctuation/case normalization, not a
semantic pass rate. They exposed numeric-choice changes, missing sentence
content and instruction handling problems. Exact match and WER cannot alone
judge whether a legitimate rewording is useful; manual review of the private
outputs supplied the concrete semantic failures above.

Keep raw Turbo output as the default. Neither tested cleanup chain is suitable
for automatic insertion. A later comparison should test a task-specialized
cleanup model on this same frozen sample and on a separate holdout, with manual
meaning checks. Do not treat these results as proof that every LFM3 precision,
runtime, prompt or fine-tune would behave the same way.

[Public aggregate evidence](evidence/lfm3-cleanup-20261030.json) records the exact
prompt, configuration, dataset provenance and scoring rules. Complete private
inputs, outputs, both stopped probes, runnable scripts and the readable
`cleanup-comparison.md` remain in the same private verification directory as the
Audio replay. The experiment server was terminated after the final request.

## Reusable benchmark follow-up

The [benchmark guide](../benchmark/README.md) owns the subsequent reproducible
dataset imports, split protocol, SkillOpt searches, held-out tests and static
results page. Its annotation adaptation and sample selection differ from the
exploratory trials above. Do not compare their WER values directly. No candidate
from the bounded prompt searches improved validation WER, so the seed remains
selected and Turbo remains the live default.

## Proposed composition

Keep capture, recognition, text processing, context capture, and insertion as
separate responsibilities. A Mode chooses which of them runs. The model should
not own shortcuts, screenshot permission, target validation, or insertion.

- Raw dictation uses a recognizer and inserts its raw transcript.
- Optional cleanup sends the raw transcript to LFM3 text and keeps both outputs.
  It preserves language, names, numbers and negations except for unambiguous
  spoken self-corrections. Failure falls back to raw text under a reviewed policy.
- A future context Mode captures one explicitly requested screenshot and combines
  it with a transcript through LFM3-VL. It returns text for review or insertion
  according to the Mode's contract.
- A future Omni Mode supplies audio and optionally an image to one model. It must
  still distinguish literal transcription from an answer or rewritten text.

An explicit context hotkey is easier to define than an always-active spoken
trigger. A spoken trigger needs a command Mode so that dictating the word
"Screenshot" does not unexpectedly capture the desktop. Capture timing also
matters: recognition after release would capture the later screen. Capturing at
hotkey activation gives a defined moment, but requires an explicit choice of
window, region or desktop. KDE/Wayland capture should use supported portal APIs
with their actual permission and selection behavior verified on the target.

The existing original-window insertion guard and cancellation remain responsible
for preventing late or misdirected insertion. Text or model output that mentions
a screenshot is data unless a defined command Mode authorizes the action.

[Text model](https://huggingface.co/LiquidAI/LFM3-1.7B),
[VL model](https://huggingface.co/LiquidAI/LFM3-VL-2B), and
[Omni model](https://huggingface.co/LiquidAI/LFM3-Omni-2B) document model capabilities,
not a ready-made desktop integration.

## Product recommendation

Keep reliable local text entry as Dipper's immediate purpose. Explore explicit
Modes for transforming dictated or selected text before considering a general
desktop assistant. These Modes reuse capture, Journal, cancellation and insertion.
A general assistant adds action authorization, tool execution, conversation state
and recovery from partially completed actions. Those are new product obligations.

Do not bind the product to one model family. Compare Audio for recognition and
text LFM3 for optional cleanup independently. Evaluate Omni only if a concrete
combined audio/image task justifies it. Model consolidation is useful only if
quality, full-job latency and measured resource use support it.

The current owner request prioritizes recognition and text processing. Screen
context remains secondary. No new shortcuts, screenshot capture, runtime adapter,
or automatic cleanup are implemented by this evaluation.
