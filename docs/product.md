# Product direction

This document preserves the intended product from the initial project brief. The broader first-release feature set and numerical acceptance targets remain open.

## Accepted pilot scope

On 2026-10-13, Kyle selected the core shortcut, dictation, and insertion workflow with a complete journal and explicit correction for the first pilot. Feature parity is not a project requirement. Wispr Flow and Superwhisper are sources of optional ideas; select features only when they serve Dipper's workflow. Target application scenarios, numerical acceptance limits, and recovery limits still need agreement in [issue #1](https://github.com/KyleDerZweite/dipper/issues/1).

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
- Select useful features through concrete user scenarios rather than a reference
  product checklist.
- Keep sufficient source data to investigate errors and compare configurations.
  Recording failures, cancellation, and retries must remain distinguishable.

German dictation with English keywords needs its own evaluation. Manufacturer
benchmarks and performance on other hardware do not establish Dipper's quality
or latency on the target PC.

## Optional feature ideas

Wispr Flow and Superwhisper inspired some of the ideas below. They are not a
parity checklist or a commitment to implement every feature. The accepted pilot
scope above owns mandatory capabilities; additions require an explicit scope
decision. Candidate areas include:

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

Optional references include Wispr's
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
Sync, meeting notes, and team features need explicit scope decisions if selected. OpenWhispr was rejected for workflow and quality.
