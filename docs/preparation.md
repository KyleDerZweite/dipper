# Development preparation

The following sequence preserves the initial preparation plan. GitHub Issues owns live status, triage notes, and dependencies. None of these steps is complete merely because it is documented.

Kyle selected the core dictation workflow, complete journal, and explicit correction as the first pilot scope on 2026-10-13. The benchmark contract must refine that direction without silently expanding it to broader feature parity.

## Start development here

1. Agree a short benchmark and parity contract. Track this work in [issue #1](https://github.com/KyleDerZweite/dipper/issues/1). Select mandatory features,
   supported languages and target apps, output examples, and measurable latency
   and quality limits. Record intentional differences from the reference products.
2. Run a bounded workstation pilot. Track this work in [issue #2](https://github.com/KyleDerZweite/dipper/issues/2). Start with roughly 20 representative German
   clips with English keywords, short prompts, longer speech, numbers, pauses,
   and spoken corrections. Keep additional unseen clips for validation.
3. Compare foundations and models using evidence. Track this work in [issue #3](https://github.com/KyleDerZweite/dipper/issues/3). Replay identical audio for
   recognition comparisons. Measure insertion and processing separately and
   together. Test cancellation, retries, focus changes, recovery, and export.
4. Choose the foundation and toolchain. Track this work in [issue #4](https://github.com/KyleDerZweite/dipper/issues/4). Record a reviewed design for capture,
   recognition, processing, insertion, journal storage, and replay, with explicit
   responsibilities and dependency directions. Document real setup and run
   commands once verified.
5. Derive implementation tickets using one PR per useful slice. Track this work in [issue #5](https://github.com/KyleDerZweite/dipper/issues/5). Implement useful
   slices with acceptance evidence, preserving the agreed product scope.

Measure recording readiness, end-of-speech to insertion, correction time, and
time to accepted text. Separate cold and warm runs and record CPU/GPU selection,
RAM, and VRAM. Median and p95 require repeated runs and a stated sample size;
20 different clips alone do not establish a reliable p95.

WER and CER use literal reference transcripts. Processed output also needs
meaning-preservation checks. A grammar improvement or intentional rephrasing
is not automatically a recognition error. Terminal tests must cover Unicode,
newlines, shell prompts, and text interfaces without unintended execution.

## Accepted delivery model

Kyle selected one PR per useful slice on 2026-10-13. Each slice must reference its reviewed component contract, dependencies, and acceptance evidence. Define the slices and integration sequence after the component contracts in [issue #4](https://github.com/KyleDerZweite/dipper/issues/4) are coherent. [Issue #5](https://github.com/KyleDerZweite/dipper/issues/5) tracks the remaining breakdown.
