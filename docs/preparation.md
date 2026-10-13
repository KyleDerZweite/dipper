# Development preparation

The following plan tracks development preparation and the accepted move from experimentation to app development. The [desktop pilot contract](pilot.md) records the accepted test scope and open acceptance decisions. GitHub Issues owns live status, triage notes, and dependencies. None of these steps is complete merely because it is documented.

Kyle selected the core dictation workflow, configurable retention, and explicit correction, with comprehensive history for his pilot as the first pilot scope on 2026-10-13. The benchmark contract must refine that direction. Feature parity is not a requirement; ideas from reference products remain optional.

## Start development here

The owner accepted the current recorder/transcription experiment as a sufficient
provisional starting point on 2026-10-13 and ended further prototype work.
Completion of all 13 corpus slots is not a prerequisite for app design or useful
implementation slices. Add recordings incrementally when they answer a concrete
question. This changes delivery sequencing, not the final pilot acceptance gates.

1. Use the accepted [pilot contract](pilot.md) and existing experimental evidence
   to design a bounded app foundation and module interfaces in
   [issue #4](https://github.com/KyleDerZweite/dipper/issues/4). The owner selected an independent app in [ADR-0001](adr/0001-own-dipper-app.md).
   Language and UI stack remain TBD. Whisper large-v3 is the accepted first live
   baseline; Qwen is excluded from active candidates, with historical evidence preserved.
2. Once a useful slice has reviewed responsibilities, interfaces, dependency
   directions, and acceptance evidence, prepare its implementation issue through
   [issue #5](https://github.com/KyleDerZweite/dipper/issues/5). Keep one PR per
   useful slice. Do not import the throwaway prototype wholesale into main.
3. Continue corpus work in [issue #2](https://github.com/KyleDerZweite/dipper/issues/2)
   alongside app development. The accepted 13-slot corpus remains a useful target,
   not an app-start gate. Do not call partial recordings corpus completion.
4. Continue the final comparison in
   [issue #3](https://github.com/KyleDerZweite/dipper/issues/3). It still needs
   representative recordings and actual workflow evidence. Starting app work
   does not prove quality acceptance, global hold-to-record, T3 insertion,
   recovery, or the 30-second release-to-insertion gate.

The [app design proposal](app-design.md) outlines responsibilities and candidate
slices for discussion. It is not an accepted implementation contract.

## Provisional experimental evidence

The local `prototype/native-recorder` branch at `3436eb18` preserves the
capture and recognition experiment. Its source records are
`prototypes/recorder/README.md`, `TRANSCRIPTION.md`, and `EVALUATION.md`.
These artifacts remain local and unpublished; consult that branch or its
existing worktree rather than assuming they exist on main.

Actual USB microphone audio was captured, retained, and run through Whisper
large-v3 with whisper.cpp/Vulkan and Qwen3-ASR-1.7B on CPU. S01 tuning and S02
fixed-configuration validation provide provisional evidence, not an overall
model ranking or complete corpus. Exact runs, revisions, limitations, and checks
belong to the experimental records. Private source audio, references, raw
transcripts, and reports remain outside Git.

Python and native command-line runtimes in the experiment are tooling choices,
not production architecture decisions. Global Wayland hold-to-record, insertion
in the actual T3 input, production retention/recovery, and end-to-end latency
remain unverified. The owner has accepted that limitation for starting app design.

Measure recording readiness, release-to-insertion, correction time, and
time to accepted text. Separate cold and warm runs and record CPU/GPU selection,
RAM, VRAM, CPU/GPU use, idle overhead, and storage growth under the selected
retention policies. Measure power use where available. Median and p95 require repeated runs and a stated sample size;
The initial 13 recordings alone do not establish a reliable p95.

WER and CER use literal reference transcripts. Processed output also needs
meaning-preservation checks. A grammar improvement or intentional rephrasing
is not automatically a recognition error. Terminal tests must cover Unicode,
newlines, shell prompts, and text interfaces without unintended execution.

## Accepted delivery model

Kyle selected one PR per useful slice on 2026-10-13. Each slice must reference its reviewed component contract, dependencies, and acceptance evidence. Define the slices and integration sequence after the component contracts in [issue #4](https://github.com/KyleDerZweite/dipper/issues/4) are coherent. [Issue #5](https://github.com/KyleDerZweite/dipper/issues/5) tracks the remaining breakdown.
