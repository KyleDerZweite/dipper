# Foundation research

This is the research snapshot recorded on 2026-10-13, moved from the initial README. It has not been revalidated during the documentation split. Candidates and dependencies remain proposals.

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

## Background and ownership

This repository owns Dipper's product development, setup, architecture, code,
and delivery records. Fleet retains the earlier research and workstation records.
The following background links use Kyle's local checkout layout and are not available through this public repository:

- [Original project brief](../../../Projects/fleet/ideas/personal-dictation.md)
- [Technical research and pilot scenarios](../../../Projects/fleet/docs/evaluations/voice-project.md)
- [Target workstation record](../../../Projects/fleet/devices/kyle-ff/README.md)
