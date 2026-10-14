# Recognition and cleanup candidates

Research checked on 2026-10-14 against Dipper revision
`bb9c34cdfa4f6d48ec7edf1ee99820e87b2c58a7` and primary sources. The owner asked to
try Whisper large-v3-turbo and revisit light cleanup, including a spoken revision
such as `3, ach nee, doch 4`. This investigation proposes comparisons. It does
not select a new live default or a cleanup implementation.

The recommended order is to compare Turbo with the existing large-v3 baseline
on the same recordings, then evaluate text processing independently. Start the
cleanup comparison with SmolLM3-3B and Gemma 4 E2B IT in non-thinking mode.
Neither candidate has demonstrated Dipper's required quality or latency yet.
Qwen remains excluded by the owner's decision.

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
These establish an existing runtime path for testing Turbo, without changing
the speech runtime. Successful target-device inference remains separate evidence.

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

Gemma 4 E2B IT is the second candidate. Google's current model card describes
multilingual training, local-device use, configurable thinking, and Apache 2.0.
Its 2.3 billion effective parameters include 5.1 billion parameters when
embeddings are counted. Therefore, E2B should not be treated as a two-billion
parameter download or a guaranteed smaller memory footprint than SmolLM3.
Disable thinking for the cleanup comparison and use text input only. This
investigation has not established its German cleanup quality.
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
so those results do not rank it on Dipper's workstation. It adds little to the
first two-candidate comparison until either candidate fails an acceptance case.
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
