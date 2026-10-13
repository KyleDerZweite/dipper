# Dipper

Dipper is a PC-first voice dictation project focused on performance, output
quality, and feature parity with [Wispr Flow](https://wisprflow.ai/) and similar
tools such as [Superwhisper](https://superwhisper.com/). It also aims to keep a
complete local journal of recordings, recognition attempts, and corrections
for personal analysis and model comparison.

The first target is Fedora KDE on Wayland, with German speech and English
technical terms. Local models are of particular interest. Cloud processing and
paid services remain acceptable options.

Status, 2026-10-13: initial project preparation. The application, foundation,
stack, license, and numerical acceptance targets are still undecided. The
features below describe the intended product. There are no application setup,
build, or run commands yet.

## Project work

Track preparation, specifications, and implementation in [GitHub Issues](https://github.com/KyleDerZweite/dipper/issues). New requests enter `needs-triage`; implementation readiness requires a reviewed contract and acceptance evidence. Agent configuration lives in [AGENTS.md](AGENTS.md) and [docs/agents/](docs/agents/).

## Intended workflow

Press a shortcut, speak, and insert usable text into the intended application.
Recording state and cancellation should be clear. The daily target apps include
T3, browsers, Obsidian, and terminals.

A second shortcut should open the latest dictation for playback and explicit
correction. A searchable journal connects its audio, raw transcript, processed
text, insertion outcome, and confirmed wording. Reprocessing and analysis run
separately from normal dictation so experiments do not delay the daily workflow.

## Product priorities

- Measure performance through the whole workflow: recording readiness, speech
  recognition, text processing, and insertion. Cold and warm starts matter.
- Assess quality by meaning preservation, technical vocabulary, names, numbers,
  negations, and the manual effort needed to reach accepted text.
- Define feature parity through concrete user scenarios. A similar shortcut or
  the same speech model does not establish equivalent behavior.
- Keep sufficient source data to investigate errors and compare configurations.
  Recording failures, cancellation, and retries must remain distinguishable.

German dictation with English keywords needs its own evaluation. Manufacturer
benchmarks and performance on other hardware do not establish Dipper's quality
or latency on the target PC.

## Feature parity to specify

The reference products are confirmed. The exact first-release feature set and
intentional differences still need to be agreed. The candidate areas are:

- Configurable shortcuts, hold and toggle recording, clear recording feedback,
  cancellation, and reliable insertion.
- Raw transcription and processed output with punctuation, paragraphs, lists,
  filler-word handling, and spoken self-corrections.
- Personal vocabulary, reviewed spelling corrections, replacements, and snippets.
- Modes for raw text, messages, email, and technical prompts, with appropriate
  model, language, and processing settings.
- Context-aware output and editing selected text through a shortcut or spoken
  instruction, with review and a way to undo changes.
- Playback, explicit correction, journal search, export, and replay through
  different model configurations.

Useful feature references are Wispr's
[formatting](https://docs.wisprflow.ai/articles/5373093536-how-do-i-use-smart-formatting-and-backtrack),
[dictionary](https://docs.wisprflow.ai/articles/4052411709-Teach-Flow-your-words-with-the-dictionary),
[snippets](https://docs.wisprflow.ai/articles/5784437944-create-and-use-snippets),
[context](https://docs.wisprflow.ai/articles/4678293671-Context-Awareness),
[transforms](https://docs.wisprflow.ai/articles/8068950331-how-to-use-transforms-beta),
and [command mode](https://docs.wisprflow.ai/articles/4816967992-how-to-use-command-mode),
plus [Superwhisper's custom modes](https://github.com/superultrainc/superwhisper-docs/blob/main/modes/custom.mdx).
Language, platform, and editor support differ between these features. Automatic
field-context access and final-text collection are unverified on KDE Wayland.

Mobile use remains optional. Server deployment, fine-tuning, unattended correction
learning, and a general voice assistant are outside the proposed initial pilot.
Sync, meeting notes, and team features need explicit scope decisions if selected
for parity. OpenWhispr was rejected for workflow and quality.

## Journal requirements

Create a durable recording identity at capture start. Preserve complete capture
audio before voice activity detection or trimming and distinguish it from the
actual model input. Record sample rate, channel count, and normalization.

Each recognition attempt should have its own identity, source recording, model
and engine versions, relevant settings, local artifact hashes, timings, result,
and error status. Retries add attempts rather than overwrite earlier results.
Keep raw recognition, processing prompts and results, insertion outcomes, and
user-confirmed corrections separate. For cloud models, record available provider
metadata and identify unknown versions.

Cancellation and failures retain available data and an explicit status. Define
crash-loss limits, recovery behavior, and what happens when storage is full.
Neither a final-text paste hook nor a history importer can recover events that
were never recorded. Silent retention expiry conflicts with a complete journal;
intentional deletion behavior still needs a decision.

SQLite with separate lossless audio files is a candidate. Their writes do not
share one atomic transaction, so recovery and consistent export need a concrete
design. Store private recordings, journals, and credentials outside the Git
checkout. Export should preserve audio, identities, attempts, settings, and
corrections independently of the application UI.

## Researched foundations and dependencies

The research dated 2026-10-13 suggests these candidates for a workstation pilot:

- [Handy](https://github.com/cjpais/Handy) is the first desktop candidate, with
  MIT code, a Tauri UI, local models, and history. Its current
  [history code](https://github.com/cjpais/Handy/blob/14ae9c6b42d729371c443c8617ad02c89c4c8061/src-tauri/src/managers/history.rs)
  overwrites retry results, and its
  [capture sink](https://github.com/cjpais/Handy/blob/14ae9c6b42d729371c443c8617ad02c89c4c8061/src-tauri/src/audio_toolkit/audio/sink.rs)
  normally returns audio after voice activity detection. A complete journal
  requires changes at capture and processing boundaries.
- [Dettivo](https://github.com/gmickel/dettivo-linux) has a more detailed
  [history and replay contract](https://github.com/gmickel/dettivo-linux/blob/043ce30e9bfa2903611f7f15fd0849091a45825d/docs/history.md),
  but canceled dictation recordings are discarded. Native KDE focus verification
  and Fedora builds remain concerns. Its
  [project policy](https://github.com/gmickel/dettivo-linux/blob/043ce30e9bfa2903611f7f15fd0849091a45825d/README.md)
  declines code contributions; an extension would likely require a maintained
  fork. Its code uses GPL-3.0-or-later.
- [Voxtype](https://github.com/peteonrails/voxtype) is the lean daemon alternative
  with MIT code, Fedora packaging, and CPU/Vulkan options. Journal capture and
  a correction UI would still need development.

hyprwhspr, Dictée, and Voisu were also investigated. Their audio retention,
journal completeness, or platform support make them secondary references.
No foundation has yet demonstrated the required performance, quality, parity,
or journal behavior on the target workstation.

The proposed speech comparison uses
[whisper.cpp](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.9.5) on CPU
and AMD Vulkan, multilingual Whisper models, and
[Parakeet v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3).
Parakeet v3 supports German; v2 is English-only. Its original weights use
CC-BY-4.0, while the [ggml conversion card](https://huggingface.co/ggml-org/parakeet-GGUF)
lists MIT. This metadata discrepancy needs resolution before redistributing
converted weights. Code, model, and bundled binary licenses must be evaluated
separately.

sherpa-onnx is an alternative CPU runtime. SQLite and lossless audio files are
storage candidates, and [JiWER](https://github.com/jitsi/jiwer) can provide WER
and CER in a separate evaluation process. Optional text processing can use local
or cloud models. Compare raw recognition first, then processed output and its
full latency. These are proposals, not selected dependencies.

The recorded target hardware is a Ryzen 7 7840HS, Radeon RX 7700S, integrated
Radeon 780M, and about 32 GB RAM. Fleet's 2026-10-10/11 checks found both GPUs
through RADV Vulkan. Speech performance and KDE insertion remain unmeasured.

## Start development here

1. Agree a short benchmark and parity contract. Select mandatory features,
   supported languages and target apps, output examples, and measurable latency
   and quality limits. Record intentional differences from the reference products.
2. Run a bounded workstation pilot. Start with roughly 20 representative German
   clips with English keywords, short prompts, longer speech, numbers, pauses,
   and spoken corrections. Keep additional unseen clips for validation.
3. Compare foundations and models using evidence. Replay identical audio for
   recognition comparisons. Measure insertion and processing separately and
   together. Test cancellation, retries, focus changes, recovery, and export.
4. Choose the foundation and toolchain. Record a reviewed design for capture,
   recognition, processing, insertion, journal storage, and replay, with explicit
   responsibilities and dependency directions. Document real setup and run
   commands once verified.
5. Choose the delivery model before breaking work into tickets. Implement useful
   slices with acceptance evidence, preserving the agreed product scope.

Measure recording readiness, end-of-speech to insertion, correction time, and
time to accepted text. Separate cold and warm runs and record CPU/GPU selection,
RAM, and VRAM. Median and p95 require repeated runs and a stated sample size;
20 different clips alone do not establish a reliable p95.

WER and CER use literal reference transcripts. Processed output also needs
meaning-preservation checks. A grammar improvement or intentional rephrasing
is not automatically a recognition error. Terminal tests must cover Unicode,
newlines, shell prompts, and text interfaces without unintended execution.

## Background and ownership

This repository owns Dipper's product development, setup, architecture, code,
and delivery records. Fleet retains the earlier research and workstation records.
The following background links use Kyle's current local checkout layout:

- [Original project brief](../../Projects/fleet/ideas/personal-dictation.md)
- [Technical research and pilot scenarios](../../Projects/fleet/docs/evaluations/voice-project.md)
- [Target workstation record](../../Projects/fleet/devices/kyle-ff/README.md)

## Logo

The animal identity is the white-throated dipper, known as Wasseramsel in German.
Kyle selected [Logo_18_universal.png](Logo_18_universal.png) as the final Dipper
logo. The transparent PNG uses the universal contrast treatment for white and
charcoal backgrounds.

![Dipper logo](Logo_18_universal.png)
