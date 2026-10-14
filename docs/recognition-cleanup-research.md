# Recognition and cleanup candidates

Research checked on 2026-10-14 against Dipper revision
`bb9c34cdfa4f6d48ec7edf1ee99820e87b2c58a7` and primary sources. The owner asked to
try Whisper large-v3-turbo and revisit light cleanup, including a spoken revision
such as `3, ach nee, doch 4`. The owner subsequently selected Turbo as the live recognition default and
requested a cleanup comparison capped at four billion total parameters. Cleanup
implementation and model management remain separate slices.

The owner selected Turbo activation and cleanup comparison first, with model
download/reload/selection UI afterward. The initial cleanup shortlist is
SmolLM3-3B, Qwen3-1.7B and Qwen3-0.6B, all with thinking disabled. Small Qwen
text models are explicitly allowed again. The previous Gemma 4 E2B proposal is
withdrawn because its 5.1B total parameters exceed the owner's cap.

## Whisper large-v3-turbo

Turbo retains the large-v3 encoder and reduces its decoder from 32 layers to
four. OpenAI fine-tuned it on multilingual transcription data and excluded
translation data. Its release report describes quality closer to large-v2
across languages, with larger regressions in some languages. This supports
testing it as a faster candidate, not assuming equivalent quality.
[OpenAI release announcement](https://github.com/openai/whisper/discussions/2363)

| Property | large-v3 | large-v3-turbo |
| --- | --- | --- |
| Parameters | 1,550 million | 809 million |
| Decoder layers | 32 | 4 |
| Multilingual transcription | Yes | Yes |
| Intended role in this comparison | Existing baseline | Candidate |

The parameter counts and multilingual status come from the
[OpenAI model card](https://huggingface.co/openai/whisper-large-v3-turbo).
Its automatic language detection and transcription task fit Dipper's accepted
Auto behavior. German and English need independent checks, including German
speech containing English technical terms. Automatic detection alone does not
establish reliable code switching within a recording.

OpenAI lists approximately eight times the relative speed and approximately
6 GB VRAM for Turbo, compared with approximately 10 GB for large. Those speed
measurements transcribe English on an NVIDIA A100. They do not predict Dipper's
RX 7700S Vulkan speed, memory consumption, or release-to-insertion latency.
Turbo is not trained for speech translation, which Dipper does not request.
[OpenAI Whisper README](https://github.com/openai/whisper#available-models-and-languages)

The locally pinned whisper.cpp revision
`0c7be696371069e0baa2515ed7aa08d5a5473aa6` already lists
`large-v3-turbo`, `large-v3-turbo-q5_0`, and `large-v3-turbo-q8_0` in its
[model downloader](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/models/download-ggml-model.sh).
Its loader reads encoder and decoder layer counts from the model file. The
same revision documents the
[Vulkan backend](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/README.md#vulkan-gpu-support).
These establish a native runtime path for testing Turbo. Before the explicit-profile repair, Dipper's C++ bridge required 32 decoder
layers and full float16 large-v3 and rejected Turbo in an isolated experiment.
The reviewed repair now distinguishes explicit large_v3 and large_v3_turbo
profiles, validating 32 or four decoder layers while preserving F16 and GPU
checks. Historical requests/settings without a profile retain large-v3 meaning.

Compare the unquantized converted models first to avoid changing architecture
and quantization simultaneously. Add Q5 or Q8 only if a second comparison is
useful. Record the source revision, model file digest and byte size, language
setting, decoding parameters, runtime revision, and selected GPU for every run.
The current constructor name `RecognitionSettings::large_v3` describes the
baseline configuration. A model path alone must not make experimental results
appear to be measurements of the original baseline.

## Cleanup candidates

Whisper produces a raw transcript. Resolving a clear spoken self-correction is
text processing and produces processed text. A later explicit user edit is a
Correction. Keep those records distinct, as required by the
[glossary](../GLOSSARY.md), [pilot contract](pilot.md), and [Journal](journal.md).
Replacing the speech model does not implement the cleanup slice.

SmolLM3-3B is the recommended first cleanup experiment. Its model card explicitly
lists German and English among six supported languages, documents llama.cpp as
a local runtime, and provides a non-thinking mode through `/no_think` or
`enable_thinking=False`. It uses Apache 2.0. These are useful starting conditions
for a small local processor, but its published general benchmarks do not measure
conservative dictation cleanup.
[SmolLM3 model card](https://huggingface.co/HuggingFaceTB/SmolLM3-3B)

Gemma 4 E2B IT was an initial proposal and is now excluded under the total cap. Google's current model card describes
multilingual training, local-device use, configurable thinking, and Apache 2.0.
Its 2.3 billion effective parameters include 5.1 billion parameters when
embeddings are counted. Therefore, E2B should not be treated as a two-billion
parameter download or a guaranteed smaller memory footprint than SmolLM3.
It is not included in the accepted comparison. No local German cleanup
quality was established for it.
[Google Gemma 4 model card](https://ai.google.dev/gemma/docs/core/model_card_4)

The runtime maintainers publish a
[Gemma 4 E2B GGUF conversion](https://huggingface.co/ggml-org/gemma-4-E2B-it-GGUF).
llama.cpp documents a
[Vulkan build](https://github.com/ggml-org/llama.cpp/blob/master/docs/build.md#vulkan),
which makes it a plausible shared runtime for the cleanup comparison on this
AMD machine. Neither model conversion availability nor backend availability
proves this combination's correctness, speed, or stability. Pin the exact runtime,
conversion and chat template before measuring.

Llama 3.2 3B Instruct is a fallback candidate with explicitly supported German
and English. Its license is the custom Llama 3.2 Community License. Meta's
published quantization performance uses ExecuTorch on an Android OnePlus 12,
so those results do not rank it on Dipper's workstation. It is not included in the current three-candidate comparison.
[Meta model card](https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct)

No cleanup model is selected. Model choice, runtime supervision, processed-text
storage and failure behavior still need the reviewed next-slice contract in
[issue #14](https://github.com/KyleDerZweite/dipper/issues/14).

## Spoken self-corrections and checks

The accepted behavior already permits clear replacements. `Wir brauchen 3,
ach nee, doch 4 Instanzen` should produce `Wir brauchen 4 Instanzen.` A blanket
rule that all raw numbers must survive would incorrectly reject that result.
Compare with the intended processed reference, while keeping the literal
reference for recognition evaluation.

Use a no-model baseline for whitespace and narrow, unambiguous cleanup rules.
A deterministic number-replacement rule may be an experimental baseline for a
strictly defined pattern. A broad expression that deletes everything before
`nein` or `doch` would mishandle quotations, ordinary negation, and ambiguous
scope. Neither a rule nor an LLM may silently choose an interpretation in those
cases. This is an engineering recommendation under the accepted
[self-correction contract](pilot.md#accepted-spoken-self-corrections).

Evaluate at least these public synthetic examples before private replay:

| Raw wording | Required meaning after processing |
| --- | --- |
| `Wir brauchen 3, ach nee, doch 4 Instanzen.` | Four instances; discard the explicit abandoned count. |
| `Wir deployen Donnerstag, äh nein, Freitag.` | Friday deployment. |
| `Wir deployen nicht am Freitag.` | Preserve the negation and Friday. |
| `Wir brauchen 3 oder 4 Instanzen.` | Preserve the uncertainty and both alternatives. |
| `Sie sagte: drei, ach nee, doch vier.` | Do not silently rewrite quoted speech without a defined policy. |
| `Set timeout to 300, no, 400 milliseconds.` | Preserve English and the intended 400 milliseconds. |
| `Nutze large-v3-turbo und T3 Code.` | Preserve technical identifiers and names. |

An LLM experiment should request only the processed text, preserve the input
language, avoid expansion and paraphrasing, and cap output and elapsed time.
Treat the dictated wording as data, including text that resembles instructions.
Keep raw and processed output together with the model/configuration identity.
Do not infer correctness from fluent wording, deterministic decoding, or a
model's own confidence statement. Guard failures and ambiguous edits need a
defined raw-text fallback or review behavior before live insertion.

## Measurement and remaining evidence

Use the same retained recordings for large-v3 and Turbo. Replay outside the
live Journal or in an isolated copy so the active app and original attempts
remain intact. Measure recognition separately from cleanup, and measure the
combined workflow before applying the existing 30-second gate for recordings
up to two minutes. A tiny sample cannot establish P95 or P99.

Start with one English and one German recording, then cover the accepted short,
typical, longer and five-minute classes. Include names, negations, numbers,
technical terms, silence and self-corrections. Record individual errors rather
than only aggregate WER. Assess cleanup against expected processed text and
recognition against literal wording.

Record model load, inference and full job elapsed time; host peak memory and
GPU memory; completion/cancellation and resource release. Separate a fresh
process with a warm OS file cache from a genuinely cold file-cache run. Do not
drop global caches or disrupt running user applications to manufacture cold
measurements. Preserve the current policy of exiting model workers after each
job. Start the cleanup experiment sequentially after ASR release to measure a
bounded memory option before investigating overlap or residency.

This research pass inspected documentation and the pinned runtime source. It
did not download models, execute inference, operate the UI, change live settings,
or test a cleanup model. Actual measurements and any accepted default change
must be recorded separately from these source-based expectations.

## Executed local comparison, 2026-10-14

The unquantized Turbo conversion was downloaded separately from the runtime
maintainer's Hugging Face repository, pinned to revision
`5359861c739e955e79d9a303bcbc70fb988958b1`. Its 1,624,555,275-byte file matched
LFS SHA-256 `1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69`.
The baseline file is 3,095,033,483 bytes. Model weight file size is not measured
peak RAM or VRAM. The running app's settings were not changed.

A private cloned synthetic Journal was used for the Dipper worker probe. Large-v3
completed and reaped; Turbo was rejected by the bridge's full-baseline
architecture check. No production validation was weakened. Both models were
then compared through the same pinned native whisper-cli, using Auto, eight
threads, beam size five, Vulkan device 1, and the same short German WAV.
The native log confirmed Vulkan1. No translation flag was used.

Three runs per model were interleaved in the order baseline, Turbo, Turbo,
baseline, baseline, Turbo, each in a fresh process. No global cache was dropped;
these are warm-file-cache observations on a desktop with other applications.

| Metric | large-v3 | large-v3-turbo |
| --- | --- | --- |
| Full CLI elapsed, median | 3.327 s | 2.004 s |
| Full CLI elapsed, observed range | 3.308 to 3.334 s | 1.999 to 2.024 s |
| Native model load, median | 1.967 s | 1.082 s |
| Language detection | German in all three runs | German in all three runs |
| Synthetic Friday meaning | Preserved in all three runs | Preserved in all three runs |

Turbo reduced full CLI elapsed by approximately 40% in this tiny sample. These
measurements are not the app's release-to-insertion latency: the CLI starts
loading after launch, whereas Dipper overlaps worker preparation with Capture
and also fingerprints runtime/model artifacts. They do not establish P95/P99,
general German/English quality, names/numbers/negations, longer recordings,
cold-file-cache behavior, or matched peak memory. Workers/CLI processes exited
after each job. No clipboard, composer, or live dictation was operated.

The source-only research above did not run models; this separate parent-run
experiment supplies the limited local evidence. The owner accepted Turbo as the live default after these comparisons;
large-v3 remains available as the explicit quality baseline.
A separate private English replay through the same CLI detected English for
both models: 3.551 s for large-v3 and 2.153 s for Turbo, one run each.
No private transcript is published. This checks basic multilingual operation,
not full semantic quality or distributional performance.

The following follow-up records the subsequently executed cleanup comparison.

## Accepted cleanup shortlist and local trials, 2026-10-14

The owner explicitly permitted small Qwen text models and capped cleanup at
four billion total parameters, including embeddings and required components.
Qwen3's smallest post-trained model is 0.6B; the 0.5B model belongs to Qwen2.5.
Use post-trained checkpoints, not their Base variants.

| Candidate | Converted artifact parameters | Q8 weight-file bytes |
| --- | ---: | ---: |
| Qwen3-0.6B | 596,049,920 | 639,446,688 |
| Qwen3-1.7B | 1,720,574,976 | 1,834,426,016 |
| SmolLM3-3B | 3,075,098,624 | 3,275,574,624 |

The Qwen3 language list includes German; SmolLM3 explicitly supports German.
All three use Apache 2.0. The names are rounded labels; actual artifact parameter
counts are recorded separately. Source-owned conversions:
[Qwen3-0.6B](https://huggingface.co/Qwen/Qwen3-0.6B-GGUF),
[Qwen3-1.7B](https://huggingface.co/Qwen/Qwen3-1.7B-GGUF),
and [SmolLM3](https://huggingface.co/ggml-org/SmolLM3-3B-GGUF).
Language source: [Qwen3 announcement](https://qwenlm.github.io/blog/qwen3/).

The strict cap also excludes Qwen3-4B's 4,022,468,096 serialized parameters,
despite its rounded name. Gemma 4 E2B totals 5.1B. Qwen2.5-0.5B-Instruct is an
optional later resource-floor candidate, not part of these runs.

The public GGUF downloads were pinned to their repository revisions and
verified by byte count and LFS SHA-256. The experiment used the official
llama.cpp Ubuntu Vulkan x64 release b11617, commit 14310190c, whose archive
matched its GitHub SHA-256. No system installation or existing Ollama service
change was needed. Each owned localhost server ran on Vulkan1/RX 7700S with
2,048 context tokens, eight threads, one slot, GPU offload and --reasoning off.
Requests also set enable_thinking=false; all observed reasoning outputs were
empty. Models were unloaded by stopping/reaping the owned server after its batch.

Ten public synthetic cases covered explicit numeric/date revision, unchanged
negation, uncertain alternatives, English revision, technical identifiers,
filler removal, dictated instructions, quotations and negated build numbers.
The first round used a plain instruction prompt. The second used the same cases,
wrapped as dictation data in JSON, with three disjoint examples: a different
number revision, uncertainty, and a dictated instruction. Parameters remained
temperature 0.7, top_p 0.8, seed 42 and 128 maximum output tokens. The references
were checked ignoring harmless punctuation/case/whitespace differences, then
outputs were inspected. A synonymous rewrite was rejected under light cleanup.

| Model | Plain prompt matches | Data + examples matches | Warm response median, second round | Server readiness, second round | Peak host RSS, second round |
| --- | ---: | ---: | ---: | ---: | ---: |
| Qwen3-0.6B Q8 | 3/10 | 6/10 | 0.136 s | 1.640 s | 0.772 GB |
| Qwen3-1.7B Q8 | 3/10 | 9/10 | 0.212 s | 2.834 s | 1.967 GB |
| SmolLM3 Q8 | 8/10 | 9/10 | 0.268 s | 3.202 s | 3.422 GB |

These are tiny prompt-dependent test counts, not quality percentages for real
dictation. Response times use a loaded batch server and prefix caches, not a
new worker per dictation. Readiness includes model initialization. Host RSS is
a process high-water observation, not PSS, dedicated VRAM or total system memory.
No matched peak VRAM or representative long-input latency was measured.

In the second round Qwen3-0.6B incorrectly chose three instead of four and 300
instead of 400. It is unsuitable for automatic spoken correction under this
configuration. Qwen3-1.7B correctly resolved both values but kept a filler and
reordered its sentence. SmolLM3 resolved the numeric revision but partially
translated the English case into German. Those failures remain recorded.

Qwen3-1.7B and SmolLM3 remain finalists for a broader frozen-prompt holdout,
with Qwen3-1.7B the smaller runtime footprint in this experiment. Neither is
selected for live automatic cleanup. A reviewed Text processing contract must
define provenance, deadlines, cancellation, raw fallback and language/meaning
checks before integration. Download, local GGUF folder, Reload and selection
UX are tracked separately in issue #22; the owner deferred that implementation.

The [public synthetic trial artifact](evidence/cleanup-models-20261014.json)
retains exact prompts, disjoint examples, allowed references, outputs, timings
and verified artifact identities for both rounds. It contains no owner recordings
or private transcripts.
