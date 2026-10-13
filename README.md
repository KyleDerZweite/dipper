<p align="center">
  <img src="Logo.png" alt="Dipper logo" width="120" height="120">
</p>

<h1 align="center">Dipper</h1>

<p align="center">Voice dictation focused on efficiency, speed, and quality.</p>

<p align="center">
  <img src="https://img.shields.io/badge/status-planning-64748b?style=flat-square" alt="Status: planning">
  <img src="https://img.shields.io/badge/target-Fedora_KDE-2563eb?style=flat-square" alt="Target: Fedora KDE">
  <img src="https://img.shields.io/badge/language-TBD-64748b?style=flat-square" alt="Implementation language: to be decided">
  <img src="https://img.shields.io/badge/stack-TBD-64748b?style=flat-square" alt="Stack: to be decided">
</p>

Dipper aims to turn speech into accurate text with low resource use and fast
responses. You choose what to keep: recordings, transcripts, corrections, and
diagnostic logs, with comprehensive retention for those who want the full history.

The first pilot targets Fedora KDE on Wayland, with German speech and English
technical terms. It covers the core dictation workflow, configurable data retention, and explicit
correction, including comprehensive retention for Kyle's pilot.
Wispr Flow and Superwhisper provide ideas for optional features; feature parity
is not a project requirement.

Desktop comes first. Phone and other mobile use are a long-term ambition, well
beyond the initial desktop work.

Status, 2026-10-13: project preparation. The foundation, stack, license, and
remaining quality, resource, and recovery acceptance details remain open. There are no application setup,
build, or run commands yet.

## Documentation

- [Product direction](docs/product.md): workflow, priorities, candidate features, and scope boundaries.
- [Journal requirements](docs/journal.md): recording history, attempts, corrections, recovery, and export.
- [Foundation research](docs/research.md): dated candidate comparison, dependencies, hardware, and background sources.
- [Pilot procedure](docs/pilot-procedure.md): recording slots, measurement steps, and quality examples.
- [Desktop pilot contract](docs/pilot.md): accepted scenarios, corpus, latency gate, and open acceptance decisions.
- [Development preparation](docs/preparation.md): pilot sequence and acceptance evidence.
- [Glossary](GLOSSARY.md): shared domain vocabulary.

## Project work

Track preparation, specifications, and implementation in [GitHub Issues](https://github.com/KyleDerZweite/dipper/issues). New requests enter `needs-triage`; implementation readiness requires a reviewed contract and acceptance evidence. Agent configuration lives in [AGENTS.md](AGENTS.md) and [docs/agents/](docs/agents/).

Repository documentation, issues, and pull requests use English. Conversations with Kyle may mix German and English.
