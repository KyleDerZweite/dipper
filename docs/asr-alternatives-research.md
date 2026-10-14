# Current local speech recognition alternatives

Checked on 2026-10-14 from Dipper `main` at `f99f9d7`, initially clean.
This is source research for German, English and mixed technical dictation on
the Ryzen 7840HS / RX 7700S. No model was downloaded or executed, and no live
setting changed. The measured Whisper large-v3-turbo baseline remains in the
[recognition report](recognition-cleanup-research.md).

Parakeet TDT 0.6B v3 is the first alternative worth measuring. Moonshine's
current German streaming models deserve a separate low-resource experiment.
Granite Speech 4.1 is a quality comparison with a larger memory cost. None is
established as better than Turbo for Kyle's speech, technical names or code
switching. Multilingual training and automatic language identification do not
by themselves establish reliable language changes within one recording.

## Shortlist

| Candidate | Language and size facts | Why compare it | Main gap for Dipper |
| --- | --- | --- | --- |
| Parakeet TDT 0.6B v3 | 600M parameters, 25 European languages including German and English, automatic detection, CC-BY-4.0 | Dedicated ASR with punctuation; native runtime already present in the pinned upstream source | Separate Parakeet API and worker adapter; RX 7700S quality, memory and speed unmeasured |
| Moonshine Small Streaming, German and English | 123M parameters per language model; Tiny Streaming is 34M; current streaming weights use MIT | CPU-oriented streaming and much smaller models fit resource efficiency and later mobile work | Language-specific checkpoints do not satisfy Dipper's Auto behavior without further design and evidence |
| Granite Speech 4.1 2B / 2B NAR | German and English supported; Apache-2.0; runtime catalog reports about 2.3B parameters for the fused stack | AR version adds punctuation and keyword biasing; NAR edits an acoustic CTC hypothesis in one parallel pass | Larger footprint; runtime capability differences and mixed-language fidelity need measurement |
| Canary 1B v2 | 978M parameters, 25 languages including German and English, CC-BY-4.0 | Dedicated multilingual ASR and punctuation | The checked native runtime requires a language hint and does not detect language automatically |
| Qwen3-ASR-0.6B | 30 languages including German and English, automatic detection, Apache-2.0 | Smaller ASR sibling could change the earlier latency tradeoff | Previous 1.7B prototype was too slow; the smaller model and a native runtime need fresh evidence |

Language, architecture and license sources: [NVIDIA Parakeet](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3),
[Moonshine current model table](https://moonshine-voice.readthedocs.io/en/latest/models/available-models/),
[IBM Granite AR](https://huggingface.co/ibm-granite/granite-speech-4.1-2b),
[IBM Granite NAR](https://huggingface.co/ibm-granite/granite-speech-4.1-2b-nar),
[NVIDIA Canary](https://huggingface.co/nvidia/canary-1b-v2),
[Qwen ASR project](https://github.com/QwenLM/Qwen3-ASR).

## Native runtime paths

Dipper's pinned whisper.cpp revision `0c7be696371069e0baa2515ed7aa08d5a5473aa6`
already includes `parakeet-cli`, a conversion script, a separate `parakeet`
library and ggml GPU device selection. This makes a standalone Vulkan comparison
plausible without changing the live app. Dipper's current Whisper bridge still
validates Whisper architecture; changing its model path cannot activate Parakeet.
The local source inspection confirms availability, not successful inference.
[Pinned CLI documentation](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/examples/parakeet-cli/README.md),
[pinned Parakeet implementation](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/src/parakeet.cpp).

There is also a current multi-model native option: `handy-computer/transcribe.cpp`,
checked at `f1dcd15858c4bbba87ca97862b4e448b2cf15b8f`. Its maintainers document
CPU and Vulkan backends and conversions for Canary, Granite, Qwen3-ASR and
Voxtral. Their per-model reports include AMD Ryzen 4750U Vulkan runs. Those
establish a documented AMD path, not RX 7700S performance or a reason to replace
Dipper's runtime. Its model-family APIs, output behavior and cancellation would
need an isolated compatibility test before integration.
[Runtime README](https://github.com/handy-computer/transcribe.cpp/tree/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f),
[Canary AMD report](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/canary-1b-v2.md).

For resource planning, that runtime publishes approximate Q8_0 artifact sizes
of 850 MB for Qwen3-ASR-0.6B, 1.14 GB for Canary 1B v2, 2.50 GB for Granite NAR
and 2.56 GB for Granite AR. These are file sizes, not peak RAM or VRAM. The
Qwen conversion catalog counts 782M parameters; model names do not consistently
describe the whole audio/text stack. Pin and inspect the chosen artifact before
using its name as a memory estimate.
[Qwen runtime catalog](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/qwen3-asr.md),
[Canary catalog](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/canary.md),
[Granite catalog](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/granite-speech.md).

Moonshine's current native library uses ONNX Runtime `.ort` assets and provides
Linux x86_64 libraries. German Small/Tiny Streaming is now published, so earlier
English-only summaries are obsolete. Version 0.1.5 documents these models and
their MIT license; legacy non-English non-streaming models have different terms.
Start its comparison on CPU. No RX 7700S acceleration is established here.
[Moonshine changelog](https://github.com/moonshine-ai/moonshine/blob/main/CHANGELOGS.md),
[native library integration](https://moonshine-voice.readthedocs.io/en/latest/using/adding-the-library/).

## Other current choices and limits

- **Granite Speech 5.0 470M TurboCTC** is newer and compact, with a documented
  native Vulkan path, but its supported language is English. It cannot replace
  the German/English Auto default. [IBM model card](https://huggingface.co/ibm-granite/granite-speech-5.0-470m-turboctc).
- **Canary 180M Flash** is a useful smaller efficiency comparison. It supports
  German, English, French and Spanish under CC-BY-4.0. Its native language-hint
  requirement remains a gap. [NVIDIA model card](https://huggingface.co/nvidia/canary-180m-flash).
- **Cohere Transcribe 03-2026**, 2B and Apache-2.0, supports 14 languages including
  German. It has a native conversion, but requires a language hint in that
  runtime. The upstream download is gated. Keep it behind the Auto-compatible
  candidates. [Cohere model card](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026),
  [runtime capabilities](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/cohere.md).
- **Voxtral Mini 4B Realtime 2602**, Apache-2.0, supports German and English and
  offers adjustable streaming delay. Its official BF16 serving instructions
  call for at least 16 GB GPU memory. Native quantized CPU/Vulkan ports exist,
  so it is not CUDA-only, but it is a larger streaming experiment rather than
  the first efficiency comparison. The native catalog lists 4.73 GB Q8_0 and
  2.83 GB Q4_K_M artifacts. [Mistral model card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602),
  [native port](https://github.com/handy-computer/transcribe.cpp/blob/f1dcd15858c4bbba87ca97862b4e448b2cf15b8f/docs/models/voxtral-realtime.md).

Granite NAR's reference implementation requires FlashAttention 2 and documents
a CUDA installation. Its native port offers a different path that must be
validated separately. Qwen's reference package offers Transformers and vLLM;
the native port supports offline ASR but not its streaming API. Qwen3-ASR-0.6B
is an audio model, distinct from the requested Qwen3.5-0.8B cleanup candidate.
Reconsidering its runtime does not undo the earlier rejection of slow 1.7B
prototype behavior.

## Next comparison

First replay the same retained recordings through Turbo and standalone Parakeet
v3. Include German, English, mid-sentence code switching, technical names,
negations, numbers, silence, and explicit spoken revisions. Score ASR against
literal speech; optional cleanup has its own intended-text reference.

Measure fresh-process load, inference, full-job latency, peak RAM/VRAM and worker
exit for short, one-minute, two-minute and five-minute recordings. Keep the live
Journal and app untouched. Do not convert vendor batched throughput, English
leaderboard WER or a warmed runtime's timings into Dipper P95/P99 promises.
Then test Moonshine CPU for resource savings and Granite 4.1 for quality if the
first comparison leaves a useful gap. Any new app adapter needs a reviewed
contract for Auto behavior, model identity, bounded execution and raw transcript
storage before implementation.
