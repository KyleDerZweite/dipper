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

Status, 2026-10-14: the raw voice-to-T3 development app records on Hold, retains
its Journal, runs one Whisper/Vulkan job, and attempts guarded Paste. New setups
select Whisper large-v3-turbo; existing settings retain their large-v3 profile
until the model profile and path are changed in Settings.
A real T3 integration test passed with synthetic German audio. Use the
[development guide](docs/development.md) for build/run commands and verification
limits. HyperX spoken testing, cleanup, Corrections, reduced retention, and full
pilot evaluation remain open.

## Documentation

- [App design and reviewed first-slice contract](docs/app-design.md): module responsibilities, interfaces, failure handling, and acceptance evidence.
- [Development guide](docs/development.md): native dependencies, build/run commands, first use, and private storage.
- [Product direction](docs/product.md): workflow, priorities, candidate features, and scope boundaries.
- [Journal requirements](docs/journal.md): recording history, attempts, corrections, recovery, and export.
- [Recognition and cleanup comparison](docs/recognition-cleanup-research.md): Turbo measurements and local cleanup candidates.
- [Foundation research](docs/research.md): dated candidate comparison, dependencies, hardware, and background sources.
- [Native desktop research](docs/native-desktop-research.md): KDE interaction, target validation, and insertion limits. Related reports cover [worker supervision](docs/worker-lifecycle-research.md) and [Journal recovery](docs/journal-recovery-research.md).
- [Pilot procedure](docs/pilot-procedure.md): recording slots, measurement steps, and quality examples.
- [Desktop pilot contract](docs/pilot.md): accepted scenarios, corpus, latency gate, and open acceptance decisions.
- [Development preparation](docs/preparation.md): pilot sequence and acceptance evidence.
- [Glossary](GLOSSARY.md): shared domain vocabulary.

## Project work

Track preparation, specifications, and implementation in [GitHub Issues](https://github.com/KyleDerZweite/dipper/issues). New requests enter `needs-triage`; implementation readiness requires a reviewed contract and acceptance evidence. Agent configuration lives in [AGENTS.md](AGENTS.md) and [docs/agents/](docs/agents/).

Repository documentation, issues, and pull requests use English. Conversations with Kyle may mix German and English.
