# Dipper benchmark

This directory evaluates speech recognition, text cleanup and their composition
without changing Dipper's live model, Journal or insertion behavior.

## Contract

Dataset import owns immutable source captures and normalized sample manifests.
Evaluation owns model calls and raw run records. Scoring reads explicit reference
roles, never inferred ground truth. SkillOpt changes cleanup prompts using train
and validation only. The static publisher reads an allowlisted aggregate format;
it never copies local run directories, audio, transcripts or filesystem paths.
The page is inspired by [Litebench](https://github.com/KyleDerZweite/litebench):
a compact dark results view, filters, ranking and inspectable evidence. It is a
separate dependency-free static interface, with light and system themes.

Samples distinguish literal recognition references, intended cleanup references,
and unconfirmed historical transcripts. Private Journal rows have no gold
reference until a human supplies one. They can be replayed and inspected, but
cannot produce an accuracy ranking. Raw ASR is scored against literal wording;
cleanup and complete chains use intended wording. FLEURS supplies ASR references
only. Sotto supplies text pairs only. Nyra supplies both and audio. Dataset-specific
annotation adaptation is explicit and never applied to predictions after scoring.

Splits are deterministic and deduplicated by normalized content. The previously
inspected first 30 Nyra/Sotto rows belong to exploratory development data, never
the final test set. Prompt optimization cannot read the held-out test pool. All
prompt candidates, rejected updates, run configuration and model identities remain
with the run. WER is one metric; numeric/negation changes, invalid output and
truncation remain visible. A lower WER does not establish semantic safety.

Raw datasets, audio and full local results stay in ignored `data/` and `runs/`.
The GitHub Pages artifact contains only reviewed aggregate records. Unknown
licenses are recorded; source examples with unknown rights are never published.
No provider credentials are needed for the default local experiment.

## Local setup

Python 3.11 or newer, uv, FFmpeg and Node.js 24 are required. Native ASR also
requires the existing pinned whisper.cpp/Vulkan runtime and llama.cpp b11960.
Dependencies, including the upstream SkillOpt source revision, live in `uv.lock`.
The benchmark adds no dependency to the desktop app.

From the repository root:

```sh
source scripts/dev-env.sh
uv sync --project benchmark --locked
cd benchmark
HF_HUB_DISABLE_XET=1 uv run --locked python -m dipperbench download nyra sotto fleurs
uv run --locked python -m dipperbench prepare --per-split 40
uv run --locked python -m dipperbench import-journal \
  --database "$HOME/.local/share/dipper/app/journal.sqlite3" \
  --audio-root "$HOME/.local/share/dipper/app"
```

The source capture downloads every Nyra and Sotto parquet shard, plus every
German and English FLEURS parquet split. It deliberately excludes other FLEURS
languages and duplicate legacy archive encodings. `capture.json` records source
revision, filenames, byte sizes and local digests. The prepared mini suite takes
up to 40 samples per dataset/language/split after content deduplication. Source
captures stay unchanged. The HTTP download option avoids the Xet transfer stall
observed on this workstation. The initial download is about 5.6 GB in decimal
units; materialized audio needs additional space.

Journal import reads SQLite in read-only mode, retains audio references and
fingerprints, and writes a protected local manifest. It does not copy or modify
original audio or attempt records. Re-importing writes the latest snapshot;
keep an existing run's manifest fingerprint to identify the evaluated snapshot.
To score private recordings, create a separate local JSON annotation file:

```json
[{"id":"journal-RECORDING_ID","literal_reference":"What was actually spoken.","intended_reference":"The intended inserted wording.","reviewed_by":"Kyle","reviewed_at":"2026-10-30"}]
```

Attach it with `python -m dipperbench attach-references --samples data/journal.json
--references data/references.json --output data/journal-reviewed.json`. This
writes a new manifest and preserves the imported source. Confirmed private text
still cannot enter the public publisher.

## Run models and optimize

Model files remain external to Git. The example configs use the workstation's
existing model cache. Other installations can point the same environment
variables at their own verified downloads. Obtain LFM3 text and Audio GGUF files
from the [official collection](https://huggingface.co/collections/LiquidAI/lfm3).
The Audio config requires both the language weights and the audio projector.

In a terminal with `scripts/dev-env.sh` sourced, configure and start the local
text server. Keep it running for cleanup and SkillOpt jobs:

```sh
export DIPPER_BENCH_MODELS="$HOME/.local/share/dipper/model-comparison/lfm3"
export DIPPER_BENCH_LLAMA="$DIPPER_BENCH_MODELS/llama-b11960"
"$DIPPER_BENCH_LLAMA/llama-server" \
  -m "$DIPPER_BENCH_MODELS/text-Q4_K_M.gguf" --alias lfm3-text \
  --host 127.0.0.1 --port 18743 --device Vulkan1 -ngl 99 -t 8 \
  -c 16384 -np 1 --reasoning off --temp 0 --top-k 50 --repeat-penalty 1.1
```

Run cleanup comparisons, or the sequential ASR and chain matrix, from `benchmark/`:

```sh
uv run --locked python -m dipperbench run-suite \
  --config configs/mini.example.json --output runs/text-test-new
uv run --locked python -m dipperbench run-suite \
  --config configs/audio-mini.example.json --output runs/audio-test-new
uv run --locked python -m dipperbench optimize \
  --config configs/skillopt-local.json --rounds 3 --limit 8 \
  --output runs/skillopt-new
```

The Audio matrix also needs `DIPPER_WHISPER_DIR`, `DIPPER_BENCH_MODELS` and
`DIPPER_BENCH_LLAMA` in its shell. `scripts/dev-env.sh` supplies the Whisper
location on the pilot workstation. Select the actual GPU for other machines
with `device` for LFM3 Audio or `gpu_device` for Whisper before comparing timings.

SkillOpt uses the locally installed `qwen3.5:4b` through Ollama as the optimizer,
and LFM3 as the target. It does not call a hosted provider. Change the explicit
optimizer config to compare another optimizer. The adapter uses upstream
reflection, edit application and the strict validation gate. It is a bounded
external SkillOpt environment, not the full upstream ReflACT trainer with
aggregation, clipping, slow updates or meta-skills. Each round reflects on train
failures, applies the proposed patches, then accepts only a strict reduction
of validation corpus WER. The gate receives `1 / (1 + WER)`, so its order matches
WER without clipping insertion-heavy failures. Ties and regressions keep the
previous prompt. Test data is never passed to reflection or prompt selection.

`best_skill.md`, per-round candidates, edit reports, trajectories, completion
reasons and gate decisions remain in the output directory. The final evaluation
uses `best_skill.md` as `--prompt` with the same model settings as the seed.
`--rounds` controls the budget; optimization cannot promise an improvement.
Review meaning changes and flag counts before adopting a prompt. Numbers and
negation flags are review hints, not semantic judges, and legitimate spoken
self-corrections can trigger them.

For one model or a private inspection, use `python -m dipperbench eval --help`.
Use `--samples data/journal.json --dataset journal --split inspection` for
unconfirmed recordings. Chains require an earlier ASR run through `--upstream`;
IDs and complete sample fingerprints must match. Upstream failures remain
failures. Source audio fingerprints are checked before inference. CLI output
formats are pinned to the native runtime; an unrecognized format fails rather
than guessing where a transcript begins.

Run directories are immutable: choose a new output name for another run.
Each `run.json` retains raw inputs/outputs, reference roles, errors, timing,
configuration, model identity evidence and corpus metrics. A failed prediction
is scored as empty text where a reference exists, and the failure count remains
visible. Empty or unavailable references have no invented WER. Identity cleanup
is the no-model baseline. Text-only cleanup latency excludes model startup;
ASR includes preparation and a fresh process. Chain latency sums its recorded
ASR and cleanup stages and excludes capture and insertion. WER/CER can exceed
100% when a model inserts enough additional text.

## Public results and Pages

`configs/publication.json` registers model names and public metadata. Register
new models deliberately before export. Only exact prompt texts already reviewed
into `prompts/` may appear publicly. Unapproved prompt text, private recordings,
unconfirmed transcripts, raw model responses and local paths are excluded.
The current publisher exports no source examples, including MIT data. This keeps
unknown-license Nyra text and all private outputs outside the site.

Build an inspectable site from local raw runs:

```sh
uv run --locked python -m dipperbench build-site \
  runs/text-test-new/nyra-identity/run.json \
  runs/text-test-new/nyra-seed/run.json \
  --output runs/site-new
```

Review the generated `results.json` before copying it to `results/latest.json`
for versioned publication. Rebuild the committed public results with:

```sh
uv run --locked python -m dipperbench build-site \
  --approved-results results/latest.json --output runs/site
```

The builder copies only audited static assets and the explicit public schema.
It refuses a destination containing unrelated files. The static page has task,
dataset/language and split filters, WER/CER/exact-match/latency charts, and
inspectable prompts, configuration and limitations. All assets are local.

`Benchmark checks` tests and builds pull requests without downloading datasets
or calling models. `Publish benchmark Pages` is a separate manual workflow,
which checks and publishes the committed aggregate artifact after GitHub Pages
is configured to use GitHub Actions. It never uploads `data/` or raw `runs/`.
The workflow has not been dispatched by this implementation.

## Checks

```sh
uv run --locked python -m unittest discover -s tests -v
node --check site/app.js
BENCHMARK_CHROMIUM=/path/to/chromium node tests/site.mjs
```

The browser check uses real Chromium and synthetic public fixtures. It covers
responsive layout, keyboard evidence selection, filters, empty/error/retry,
unavailable and above-100% metrics, escaped content, and theme persistence.
Actual exported data needs its own integration check. Model replay and prompt
optimization require real runtimes; fixtures do not establish their quality.

## Executed experiment, 2026-10-30

All three source captures completed: 202 Nyra utterances, 142,424 Sotto pairs,
and 7,855 FLEURS German/English rows. The prepared suite contains 535 samples.
A read-only Journal import retained 1,176 private recording references as
unconfirmed. Two private recordings were replayed through each ASR and cleanup
chain; twelve existing private transcripts were also cleaned. These 20 local
observations have no invented accuracy score.

Two SkillOpt searches evaluated three candidates each using eight training and
eight validation samples. The first used target temperature 0.1; the second
used greedy decoding. All six candidates were rejected. The selected prompt
therefore equals the seed. These runs use a local 4B optimizer and a limited
sample, and do not show that better prompts are impossible. One malformed
analyst response in the second search yielded no usable edit. JSON recovery is
now a pinned dependency. The first integration did not retain completion reasons;
those historical values remain explicitly unknown. Current integration retains
them, and CLI-only ASR reports unknown truncation where the runtime does not
expose a finish reason.

The frozen test evaluation contains 108 public observations across ASR, cleanup
and chains, with no request failures. Public results are in
[results/latest.json](results/latest.json), and the validation search history is
in [results/optimization.json](results/optimization.json). Full outputs remain in
ignored local runs. Identity and cleanup comparisons below use the same intended
references and the same samples within each row.

| Test subset | Samples | Without cleanup WER | LFM3 cleanup WER |
| --- | ---: | ---: | ---: |
| Nyra supplied text | 12 | 23.91% | 30.43% |
| Sotto supplied text | 12 | 27.78% | 27.86% |
| Whisper Turbo on Nyra audio | 6 | 4.61% | 18.42% |
| LFM3-Audio on Nyra audio | 6 | 5.26% | 8.55% |

Raw ASR was also tested against its literal references on Nyra and on six
German and six English FLEURS clips per recognizer. These scores remain separate
from the intended-text chain scores on the page. They must not be compared as
if they used the same target wording. Native ASR ran sequentially with a resident
cleanup server on the pilot workstation, without dropping file caches. No matched
peak RAM/VRAM, energy measurement or percentile latency evidence was collected.
Public benchmark contamination in model training is unknown.

The initial exploratory results in the earlier LFM3 report used other samples,
annotation handling and prompts. Their WER is not directly comparable with this
protocol. Keep raw Turbo as the live default; this bounded optimization did not
establish a useful automatic cleanup prompt.

Seven boundary/integration tests, JavaScript syntax checks and the static export
passed. Real Chromium checks passed for synthetic states and the actual 14-group
export, including filters, keyboard evidence selection, themes, 320–1440px
layouts, error recovery and zero external requests. Review findings about stale
publication files, chain identity, exposed-row reservation and completion reasons
were resolved with focused checks. The native jobs and owned text server exited.
GitHub CI and the Pages deployment have not run for this working diff. Visual
owner acceptance remains separate from the technical checks.
