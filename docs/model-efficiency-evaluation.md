# Whisper Quantization, Parakeet Ultra, and Language Router Evaluation (2026-11-20)

This note summarizes the empirical model efficiency evaluation conducted on the target Fedora KDE Wayland host (`AMD Radeon RX 7700S (RADV NAVI33)` via Vulkan1, 8 CPU threads) and explains why Dipper integrates quantized `whisper.cpp` Turbo profiles (`LargeV3TurboQ8_0` and `LargeV3TurboQ5_0`) while rejecting a multi-model Language Router architecture and deferring a separate `transcribe.cpp` / Parakeet runtime.

See the sanitized machine-readable evidence in [model-eval-20261120.json](evidence/model-eval-20261120.json).

## Corpus and Methodology

- **Concurrency rule**: Strictly **1 speech-recognition process at a time** with a 1-second cooldown between runs to avoid GPU memory contention and thermal/VRAM distortion.
- **Corpus (`C01..C13`)**:
  - `C01`, `C02`: Confirmed pilot recordings (`48` and `45` reference words respectively) with German/English technical vocabulary.
  - `C03`: Retained 46.85 s partial-capture recording (`8,994,908` bytes RF64) used to verify recovery and compare F16 vs Q8_0 on identical worker paths.
  - `C04..C13`: Ten diverse live dictation samples spanning `4.03 s` to `117.09 s` across German speech, English speech, and intra-sentence German/English code-switching.

## Benchmark Results

| Candidate | Backend | Artifact Size | Cold Load (min / med / max) | Wall Time (min / med / max) | Peak Host RSS (min / med / max) | Confirmed Pilot Edits (`C01`, `C02`, Total) |
|---|---|---:|---:|---:|---:|---:|
| `Whisper large-v3-turbo` (`F16`, `ftype=1`) | `whisper.cpp` Vulkan1 | 1,624,555,275 B (1.62 GB) | 1299 / 1500 / 1561 ms | 2.386 / 2.728 / 6.862 s | 221.0 / 221.4 / 221.8 MB | 9/48, 6/45 (**15/93**, 16.13%) |
| **`Whisper large-v3-turbo Q8_0`** (`ftype=7`) | `whisper.cpp` Vulkan1 | **874,188,075 B (874 MB)** | **712 / 823 / 1083 ms** | **1.678 / 2.200 / 5.975 s** | **155.6 / 155.9 / 177.8 MB** | 9/48, 5/45 (**14/93**, 15.05%) |
| `Whisper large-v3-turbo Q5_0` (`ftype=8`) | `whisper.cpp` Vulkan1 | 574,041,195 B (574 MB) | 505 / 566 / 665 ms | 1.454 / 1.970 / 5.815 s | 130.8 / 135.1 / 178.0 MB | 9/48, 5/45 (**14/93**, 15.05%) |
| `parakeet-ultra-Q8_0` (`gguf`) | `transcribe.cpp v0.4.2` Vulkan | 740,363,168 B (740 MB) | 693 / 769 / 787 ms | 1.065 / 1.406 / 3.426 s | 171.4 / 174.8 / 196.6 MB | 7/48, 5/45 (**12/93**, 12.90%) |
| `parakeet-ultra-Q8_0` (`gguf`) | `transcribe.cpp v0.4.2` CPU | 740,363,168 B (740 MB) | 574 / 588 / 630 ms | 1.153 / 2.576 / 10.511 s | 1213.4 / 1308.0 / 1716.0 MB | 7/48, 5/45 (**12/93**, 12.90%) |

### Direct `dipper-worker` Verification on `C03` (46.85 s Recovery Clip)

Running the pinned `dipper-worker` / `native_job` binary sequentially on `AMD Radeon RX 7700S` (`Vulkan1`) produced **bit-for-bit identical transcripts** (`884` bytes) while leaving the original RF64 file SHA-256 unchanged:

- `large_v3_turbo` (`F16`): `preparation_ms = 3503`, `inference_ms = 2368`, `wall_ms = 6037`
- `large_v3_turbo_q8_0` (`Q8_0`): `preparation_ms = 2279` (**-35% cold-load time**), `inference_ms = 2557`, `wall_ms = 4961` (**-18% total wall time**)

## Architectural Decisions

### 1. Recommend `Whisper large-v3-turbo Q8_0` (plus `Q5_0` as compact option)

- **Efficiency win without quality loss**: `Q8_0` reduces model weight size by **46%** (`874 MB` vs `1.62 GB`), cuts median cold-load latency by **45%** (`823 ms` vs `1500 ms`), lowers peak host RSS by **30%**, and improved end-to-end wall time by ~`0.5 s` median (`1.08 s` faster on the 47 s `dipper-worker` test) with virtually indistinguishable transcripts relative to F16 across all 13 clips.
- **Backward compatibility**: `Settings::defaults` keeps `LargeV3Turbo` so existing installs without a Q8_0 file continue working uninterrupted, while Settings presents `Whisper large-v3-turbo Q8_0` as `"Recommended · Efficient"` and `Q5_0` as `"Compact"`. Both load cleanly in the existing pinned `whisper.cpp` worker with strict `expected_ftype` verification (`F16=1`, `Q8_0=7`, `Q5_0=8`).

### 2. Why a Language Router + Monolingual Experts Is Rejected

A language-ID router dispatching to separate English and German expert models is a **worse architecture** for Dipper's actual workload and latency model:

1. **Intra-sentence code-switching**: 10 of 11 inspected live German/English clips mix German grammar with English technical terms (`PR`, `Sub-Agents`, `Build`, `Daily Logins`, `Mockups`) or switch languages mid-recording. Monolingual experts corrupt embedded foreign technical vocabulary.
2. **Incompatible with Hold-overlap preparation**: Dipper starts preparing the recognition worker while the user holds the shortcut key, before audio finishes. Routing based on spoken audio either delays model loading until after Hold release (eliminating Hold-overlap gains) or requires holding two models in VRAM simultaneously (negating memory efficiency).
3. A single compact multilingual model (`Whisper large-v3-turbo Q8_0`) handles both languages and code-switching in one Vulkan allocation that cleanly frees VRAM between dictations.

### 3. Why `parakeet-ultra` (`transcribe.cpp`) Is Documented as Follow-Up Work

`parakeet-ultra-Q8_0` on Vulkan demonstrated impressive speed (`1.406 s` median wall time, `3.426 s` max on the 117 s clip) and lower confirmed edit distance (`12/93`), especially on mixed phrasing. However, we did not ship a second C++ inference runtime inside this PR because:

- Bundling `transcribe.cpp` adds an unpinned second C++/Vulkan runtime ABI alongside `whisper.cpp`, expanding packaging, `RuntimeProvenance`, and shader/library surface area.
- Qualitative inspection showed a minor sensitivity gap on a couple of domain terms/German particles relative to `Turbo Q8_0`.
- **Follow-up criteria**: Consider adding an optional `transcribe.cpp` / Parakeet worker backend in a dedicated future PR once (a) provenance pinning and Vulkan packaging follow the single-worker contract, and (b) a larger (>= 50-clip) labeled code-switching WER evaluation confirms consistent net accuracy gains over `Turbo Q8_0`.
