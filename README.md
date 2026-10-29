<p align="center">
  <img src="Logo.png" alt="Dipper logo" width="120" height="120">
</p>

<h1 align="center">Dipper</h1>

<p align="center">Voice dictation focused on efficiency, speed, and quality.</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-development-64748b?style=flat-square" alt="Status: development">
  <img src="https://img.shields.io/badge/target-Fedora_KDE-2563eb?style=flat-square" alt="Target: Fedora KDE">
  <img src="https://img.shields.io/badge/language-Rust-64748b?style=flat-square" alt="Core language: Rust">
  <img src="https://img.shields.io/badge/stack-Tauri_2-64748b?style=flat-square" alt="Desktop shell: Tauri 2">
</p>

Dipper aims to turn speech into accurate text with low resource use and fast
responses. You choose what to keep: recordings, transcripts, corrections, and
diagnostic logs, with comprehensive retention for those who want the full history.

The first pilot targets Fedora KDE on Wayland. Recognition detects the spoken
language automatically and transcribes without translation. German speech with
English technical terms remains a pilot evaluation focus. It covers the core dictation workflow, configurable data retention, and explicit
correction, including comprehensive retention for Kyle's pilot.
Wispr Flow and Superwhisper provide ideas for optional features; feature parity
is not a project requirement.

Desktop comes first. Phone and other mobile use are a long-term ambition, well
beyond the initial desktop work.

Status, 2026-10-15: the development app records on Hold, retains its Journal,
runs one Whisper/Vulkan job, and dispatches Paste in the original external window.
New setups select Whisper large-v3-turbo; historical settings keep their explicit
saved model profile. Kyle selected window-guarded insertion on 2026-10-15:
Accessibility field admission is removed, while focus-loss and cancellation
still suppress Paste. The target app handles its focused control; Dipper does
not distinguish password or read-only fields. Actual target evidence belongs in
the development guide.
Use the [development guide](docs/development.md) for build/run commands and
verification limits. Optional cleanup, Corrections, reduced retention,
signed automatic updates and full-pilot evaluation remain open.
[0.0.1-alpha.1](https://github.com/KyleDerZweite/dipper/releases/tag/v0.0.1-alpha.1)
was published on 2026-10-15 as a manual Fedora 43 x86_64 pre-release and installed
on the Fedora target. See the delivery guide for evidence and compatibility limits.

## Documentation

- [App design and reviewed first-slice contract](docs/app-design.md): module responsibilities, interfaces, failure handling, and acceptance evidence.
- [Cross-app insertion contract](docs/cross-app-insertion.md): original-window validation, Paste behavior, and compatibility limits.
- [Desktop installation and updates](docs/desktop-delivery.md): published Alpha, installation, rollback, and proposed signed updates.
- [Development guide](docs/development.md): native dependencies, build/run commands, first use, and private storage.
- [Product direction](docs/product.md): workflow, priorities, candidate features, and scope boundaries.
- [Journal requirements](docs/journal.md): recording history, attempts, corrections, recovery, and export.
- [Recognition and cleanup comparison](docs/recognition-cleanup-research.md): Turbo measurements and local cleanup candidates.
- [Current ASR alternatives](docs/asr-alternatives-research.md): multilingual speech candidates and native runtime options.
- [Whistle comparison](docs/whistle-research.md): Cactus artifacts, runtime limits, and the local comparison with Turbo.
- [FreeFlow comparison](docs/freeflow-research.md): shortcut, cleanup, recovery, and context ideas, with platform limits.
- [Foundation research](docs/research.md): dated candidate comparison, dependencies, hardware, and background sources.
- [Native desktop research](docs/native-desktop-research.md): KDE interaction, target validation, and insertion limits. Related reports cover [worker supervision](docs/worker-lifecycle-research.md) and [Journal recovery](docs/journal-recovery-research.md).
- [Pilot procedure](docs/pilot-procedure.md): recording slots, measurement steps, and quality examples.
- [Desktop pilot contract](docs/pilot.md): accepted scenarios, corpus, latency gate, and open acceptance decisions.
- [Development preparation](docs/preparation.md): pilot sequence and acceptance evidence.
- [Glossary](GLOSSARY.md): shared domain vocabulary.

## Project work

Track preparation, specifications, and implementation in [GitHub Issues](https://github.com/KyleDerZweite/dipper/issues). New requests enter `needs-triage`; implementation readiness requires a reviewed contract and acceptance evidence. Agent configuration lives in [AGENTS.md](AGENTS.md) and [docs/agents/](docs/agents/).

Repository documentation, issues, and pull requests use English. Conversations with Kyle may mix German and English.
