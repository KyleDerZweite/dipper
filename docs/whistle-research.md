# Whistle source research

Verified on 2026-10-25 against Dipper commit
`641b99e36c4f8bbb25b86a272d023e49b525497e`. The working tree was clean before
this report. The source inspection below is separate from the local replay
comparison recorded at the end of this report.

Whistle has a publicly downloadable model and Linux x86_64 runtime artifacts.
The exposed interfaces support German and English, automatic language selection
and keyword biasing. These facts justify an isolated comparison, but establish
neither mixed-language accuracy nor performance on Dipper's Fedora target.

## Artifacts and licenses

The [release announcement](https://cactuscompute.com/blog/whistle), dated
2026-10-02, links the official model, runtime and Python source repositories.
The following revisions were current when checked.

| Artifact | Verified identity and evidence |
| --- | --- |
| Whistle weights | Hugging Face revision `741118687e1f008f06d1351a402a731274789790`. [`whistle.cact`](https://huggingface.co/Cactus-Compute/whistle/resolve/741118687e1f008f06d1351a402a731274789790/whistle.cact) is 16,919,407 bytes. The repository also lists `checkpoints/whistle.safetensors`, 220,618,620 bytes. |
| Linux x86_64 runtime | Needle3 revision `d6c7d6a2e9655505e0f8e23339356dd1705900eb`. [`linux-x86_64/needle`](https://huggingface.co/Cactus-Compute/needle3/resolve/d6c7d6a2e9655505e0f8e23339356dd1705900eb/linux-x86_64/needle) is 1,550,176 bytes; the same folder lists `libneedle.a`, 2,204,068 bytes, and `needle.h`, 4,528 bytes. |
| Python package | [PyPI metadata](https://pypi.org/pypi/cactus-needle/3.2.8/json) reports `cactus-needle` 3.2.8, Python >=3.9, uploaded 2026-10-19. Its universal wheel is 130,098 bytes, SHA-256 `1a0f58430041574de6e823f5db47a3b9b72286da265af11817fe6e97ee9930ed`. |
| Source inspection | [`cactus-compute/needle`](https://github.com/cactus-compute/needle/tree/b4fc11b04f1852888d375c8c6dfbaca44572d014), commit `b4fc11b04f1852888d375c8c6dfbaca44572d014`. |

The model and runner URLs above both returned HTTP 200 to HEAD requests with
matching content lengths during source inspection. The model and engine wheel
were subsequently downloaded and verified for the local trial below. The official
[Whistle metadata API](https://huggingface.co/api/models/Cactus-Compute/whistle?blobs=true)
reports model LFS SHA-256
`b6e02f048568ac5d01a2042556c658061e699acbc0aa2a1439f52f3d461dffeb`.
The [Needle3 metadata API](https://huggingface.co/api/models/Cactus-Compute/needle3?blobs=true)
reports runner SHA-256
`77cd35ee75c53df8532b397e11ed7cf94a45d89a9f14f50881fd9c68ba54a2d0`.
Use the pinned resolve URLs and compare downloaded bytes against these hashes
in the local experiment. Mutable `main` URLs do not pin an experiment.

The inspected [weights license](https://huggingface.co/Cactus-Compute/whistle/blob/741118687e1f008f06d1351a402a731274789790/LICENSE),
[runtime repository license](https://huggingface.co/Cactus-Compute/needle3/blob/d6c7d6a2e9655505e0f8e23339356dd1705900eb/LICENSE)
and [GitHub license](https://github.com/cactus-compute/needle/blob/b4fc11b04f1852888d375c8c6dfbaca44572d014/LICENSE)
all contain Apache License 2.0. PyPI declares Apache-2.0 too. This identifies
published licensing, not a complete dependency or redistribution audit.

## Interfaces verified in published files

The pinned [`needle.h`](https://huggingface.co/Cactus-Compute/needle3/blob/d6c7d6a2e9655505e0f8e23339356dd1705900eb/linux-x86_64/needle.h)
declares `needle_load`, `needle_transcribe`, streaming process/stop functions,
`needle_embed` and `needle_last_error`. It specifies 16 kHz mono float PCM in
`[-1, 1]`, JSON output and negative failure returns. Models are process-global
and not thread-safe. Its documented long-audio behavior uses consecutive
30-second windows cut at sentence ends. Streaming retranscribes buffered audio
and commits words agreed by two passes. These are published API contracts;
execution and failure behavior remain untested here.

The inspected [`needle/agent/whistle.py`](https://github.com/cactus-compute/needle/blob/b4fc11b04f1852888d375c8c6dfbaca44572d014/needle/agent/whistle.py)
implements `needle.transcribe(audio, language=None, keywords=None,
word_timestamps=False, weights=None)`, `Whistle`, `stream` and `embed` through
ctypes. It reads WAV files or float samples. Non-16-kHz WAV resampling needs
NumPy and soxr; microphone capture also needs sounddevice. The base package
depends on `huggingface_hub`, so the announcement's dependency-free statement
does not describe the Python installation.

The [`CLI parser`](https://github.com/cactus-compute/needle/blob/b4fc11b04f1852888d375c8c6dfbaca44572d014/needle/cli.py)
exposes a file-based comparison path without microphone capture:

```sh
needle whistle playground clip.wav --weights /path/to/whistle.cact --language de --keywords "Dipper,Vulkan" --word-timestamps
```

Omit `--language de` for automatic selection. This command is source-verified,
not executed. The announcement documents the separate native runner command
`./linux-x86_64/needle --model whistle.cact --audio clip.wav` and
`--audio-word-timestamps`, `--audio-depth` and `--audio-stream` controls across
its examples and current model card. Native flag parsing was not verified.

## Language and keyword handling

The wrapper and header enumerate `en`, `de`, `fr`, `es`, `it`, `nl`, `pl`.
An absent language passes NULL for detection; `language="de"` forces German.
The wrapper joins keyword lists with newlines and passes UTF-8 phrases to the
native decoder. The Python CLI accepts comma-separated phrases. No bias strength
parameter appears in these transcription interfaces. This guides recognition;
it is not Dipper's text replacement or a confirmed correction.

The announcement describes Aho-Corasick phrase biasing during beam search and
language prediction through a language token. That algorithm is a vendor claim,
not independently verified implementation evidence. One returned language value
does not establish reliable German/English changes within a recording, technical
term preservation or non-translation behavior. Those require retained recordings
with literal references, including keyword bias enabled and disabled.

## Discrepancies and comparison limits

- Package and engine versions are separate. The inspected PyPI 3.2.8 wheel's
  `needle/agent/fetch.py` pins engine 3.3.3. Needle3 contains matching
  `python/cactus_needle-3.3.3-py3-none-manylinux2014_x86_64.whl` and musllinux
  wheels. The GitHub `pyproject.toml` still says 3.2.1. Record all identities;
  do not infer the native folder's version from the package version.
- The GitHub tree contains Python code and tests, but no C/C++ runtime
  implementation or native build recipe. The announcement's source link does
  not establish reproducible native builds or access to the decoding algorithm.
- The official [devices guide](https://cactuscompute.com/blog/needle-supported-devices)
  says engines are under 1 MB. The current Linux runner and static library exceed
  that size. Its platform count also differs from the current 17-folder catalog.
- The same guide says inference never accesses the network. The Python wrapper
  calls usage telemetry, and the fetch code registers downloads even with cached
  weights. [`_telemetry.py`](https://github.com/cactus-compute/needle/blob/b4fc11b04f1852888d375c8c6dfbaca44572d014/needle/_telemetry.py)
  honors `NEEDLE_TELEMETRY=0`; offline evaluation must also account for Hub fetches.
  This does not establish that the standalone native runner sends telemetry.
- `needle whistle compare` forces English for both Whistle and Whisper in the
  inspected wrapper. It is unsuitable unchanged for Dipper's German/Auto test.
  The announcement's 11.1 ms first-token figure uses 10 seconds of audio on an
  Apple M4 Pro. It is neither fresh-process latency nor Fedora performance.

An app adapter would still need explicit contracts for cancellation, deadlines,
model identity, Auto behavior and Journal provenance. Replay evidence alone does
not establish the live dictation or insertion path.

## Local comparison contract, 2026-10-25

Kyle selected Whistle as the next speech experiment and excluded Qwen from this
round because of its earlier quality. Compare the current Turbo baseline with
Whistle in Auto mode, then repeat Whistle with the single keyword `Mergen`.
The keyword run measures recognition bias, not an automatic text replacement.
No live model switch or app integration is part of this experiment.

The private inventory contains the 20 recordings used on 2026-10-14, the latest
20 completed app recordings, and three additional recordings selected by
`Mergen`/`Merchen` in existing transcripts. Deduplication leaves 43 clips,
984.76 seconds of capture audio, with a maximum duration of 119.232 seconds.
Only two clips have owner-confirmed literal references, totaling 93 normalized
words. Historical and freshly generated Turbo transcripts are comparison
outputs, not ground truth. The term-selected clips are a targeted diagnostic
sample, not an unbiased quality sample.

Every configuration receives the same full clip, converted with FFmpeg to
16 kHz mono PCM16, without trimming, gain changes or VAD. Each attempt runs in
a fresh process. Wall time includes process startup, model loading, audio
reading, recognition and process exit. It excludes capture and insertion.
Whistle uses its CPU runtime defaults. Turbo uses the existing pinned
whisper.cpp runtime, F16 large-v3-turbo, Auto, eight threads, beam size five,
and Vulkan device 1 on the RX 7700S. Jobs run sequentially with uncontrolled
file caches and desktop load. Host peak RSS excludes VRAM and does not describe
total system memory or all memory mapped model pages.

The experiment loads the unpacked official Python 3.2.8 wrapper and its pinned
3.3.3 shared engine directly from a protected directory. The actual model and
engine wheel downloads match their official LFS SHA-256 values. Explicit local
weight/library paths, `NEEDLE_TELEMETRY=0` and `HF_HUB_OFFLINE=1` avoid automatic
fetches and wrapper telemetry during inference. No system package installation
or app settings change is needed.

Private recordings, transcripts, snapshot, exact commands, scripts, timings and
artifact hashes live at `~/.local/state/dipper/verification/whistle-20261025`.
`comparison.html` provides local audio playback and all three outputs per clip;
`results.json`, `inventory.json`, `manifest.json`, `provenance.json` and
`input-verification.json` retain the evidence. Source recordings are read-only;
the experiment does not add attempts to the live Journal.

## Local results and decision

All 129 jobs completed successfully and exited. Source audio hashes remained
unchanged. The [sanitized evidence](evidence/whistle-20261025.json) records the
aggregate measurements and pinned official artifacts without private wording.

| Configuration | Median full job | Maximum full job | Maximum host RSS | Word edits against 93 confirmed words |
| --- | ---: | ---: | ---: | ---: |
| Whisper large-v3-turbo F16, Vulkan | 2.269 s | 6.078 s | 220.4 MB | 15 |
| Whistle, CPU | 2.330 s | 18.133 s | 134.8 MB | 31 |
| Whistle, CPU, keyword `Mergen` | 2.275 s | 19.729 s | 134.8 MB | 31 |

The 119.232-second recording took 6.078 seconds with Turbo, 18.133 seconds
with Whistle and 19.729 seconds with keyword biasing. Short clips sometimes
favored Whistle, but this configuration has no overall median speed advantage.
The 16.9 MB weight file is not its runtime memory requirement. Lower host RSS
and CPU-only execution are useful properties; the comparison does not measure
energy use or matched total RAM plus VRAM.

The literal scoring uses the existing comparison's case-folded `\w+` tokens
and Levenshtein word edits. It ignores punctuation and case, but retains numeric
wording differences. Two reference clips cannot rank general quality. On those
clips, however, Whistle makes more edits and damages technical vocabulary and
the final negative instruction. Both configurations retain the explicit revised
numeric values. This is ASR evaluation against literal speech, not a cleanup test.

Spot checks of newer mixed German/English dictation also show technical-term
regressions. Whistle returns `Mörtchen` for the owner's current reported `Mergen`
example, while Turbo returns `Merchen`. Passing `Mergen` as a keyword does not
change Whistle's output for that clip. Across all 43 clips, the keyword produces
one changed text output and leaves the other 42 identical. This establishes no
useful correction for the target term under the tested configuration; it does
not prove that keyword biasing can never help.

Kyle accepted these results on 2026-10-25 and authorized closing the current
model evaluation in issue #3. Keep Turbo as the live default. Whistle is runnable and substantially smaller
on disk, but this trial does not justify an app adapter or a replacement for
the owner's technical dictation. Any future Whistle experiment should have a
specific new hypothesis, such as decoder settings or a CPU-only target, before
repeating the same comparison. Whistle remains a possible future CPU/low-memory
candidate, with no current integration work planned. Qwen was not evaluated
in this round and further Qwen speech trials are deferred.

Streaming, deliberate process failure/cancellation, standalone silence,
five-minute recordings, power and live insertion were not tested. No production
behavior changed, so application regression tests were not run. Private audio
playback and final wording acceptance remain with Kyle; the agent's spot checks
compare text outputs and the two existing confirmed references.
