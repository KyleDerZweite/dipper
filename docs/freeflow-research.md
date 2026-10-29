# FreeFlow ideas for Dipper

Research date: 2026-10-29. Status: shelved by owner decision. None of these findings changes accepted Dipper scope, model selection, retention, or insertion behavior. No FreeFlow-inspired implementation or experiment is scheduled.

Reviewed public `main` source at revision `a647469d268d2265f036b4024c509e14eea7fa1c`. Links below use canonical repository paths and may change as upstream advances. Sources were read through GitHub API and raw downloads, without running upstream code, installing dependencies, or downloading models. Tests were inspected, not executed. The comparison uses Dipper's [product direction](product.md), [glossary](../GLOSSARY.md), [insertion contract](cross-app-insertion.md), and [ADRs](adr/).

FreeFlow is a macOS Swift app using configurable transcription and text-model APIs. Its [README](https://github.com/zachlatta/freeflow/blob/main/README.md) advertises fast transcription, context-aware cleanup, custom vocabulary and selected-text editing. Source supports those mechanisms, but does not establish their quality or speed on Dipper's Fedora KDE target.

## Current assessment

Kyle declined the initial shortcut, recovery, vocabulary and editing suggestions on 2026-10-29. Hold remains preferred. The sections below remain source observations, not a recommended backlog. The follow-up focuses on models, the audio-to-text pipeline and design.

After reviewing the follow-up, Kyle also deferred the pipeline and design ideas because they offer no sufficient current benefit. Realtime recognition remains an interesting experiment with no current priority. Revisit only after an explicit decision to resume.

Kyle prefers exploring a configurable keyword library later. The concrete desired replacement is `Merchen` → `mergen` in his German dictation, where he means the English-derived technical verb. This is an explicit text replacement, separate from FreeFlow's vocabulary hints to an LLM. It relates to [issue #8](https://github.com/KyleDerZweite/dipper/issues/8). The example does not settle general matching rules, capitalization, inflections or implementation scope. No keyword-library implementation is authorized by this research decision.

### Models and pipeline

The application default is Groq-hosted `whisper-large-v3`, confirmed in [AppState](https://github.com/zachlatta/freeflow/blob/main/Sources/AppState.swift) and [TranscriptionService](https://github.com/zachlatta/freeflow/blob/main/Sources/TranscriptionService.swift). Default cleanup uses `openai/gpt-oss-20b`, with `qwen/qwen3.8-27b` fallback and low reasoning for the default model in [PostProcessingService](https://github.com/zachlatta/freeflow/blob/main/Sources/PostProcessingService.swift). These cleanup models exceed Dipper's accepted local parameter cap. This is hosted inference, not evidence of a superior Mac-local recognizer.

The ordinary path converts capture into a 16 kHz mono PCM16 WAV, uploads after recording, parses the transcript, optionally processes it with an LLM, then inserts text. Optional realtime sends separate 24 kHz PCM16 buffers while recording and falls back to file upload. The realtime preference defaults to false through `UserDefaults.bool`; the model is separately configurable. Provider support is required, so this is not evidence that Groq's default Whisper endpoint streams. See [AudioRecorder](https://github.com/zachlatta/freeflow/blob/main/Sources/AudioRecorder.swift), AppState and section 7 below. Overlapping recognition with recording could reduce release-to-result latency; local incremental Whisper would need its own segmentation, finalization and quality experiment. Dipper already overlaps worker preparation with capture.

[TranscriptTextCore](https://github.com/zachlatta/freeflow/blob/main/Sources/TranscriptTextCore.swift) suppresses a fixed English phrase list only when the first segment's `no_speech_prob` is at least 0.1. This is a heuristic, not general hallucination detection or VAD. The comment claims tuning against roughly 500 samples; that dataset was not verified. Dipper's bridge currently concatenates returned segments without an equivalent application-level filter. Useful next evidence would be silence, background noise and genuine short replies in German and English, rather than copying these thresholds. Whisper's own decoding behavior is separate from this application filter.

The cleanup prompt explicitly preserves mixed languages and technical identifiers, interprets explicit spoken corrections, and distinguishes spoken source text from a technical replacement. For example, a rename instruction must not turn both its source and destination into the same identifier. `TranscriptOutputSanitizer` also heuristically detects when cleanup answered a dictated instruction. These rules and failure examples are useful evaluation inputs; the prompt and English token heuristic do not establish semantic correctness.

### Design

[RecordingOverlay](https://github.com/zachlatta/freeflow/blob/main/Sources/RecordingOverlay.swift) uses a small black, nonactivating overlay at the top of the display, with a notch-aware compact variant. It distinguishes initialization, recording, processing and feedback. Recording uses audio-level bars; processing starts with animated bars and changes to a spinner after one second. Width remains fixed through processing. Closing the overlay unmounts its view so continuous animations stop.

Dipper's `ui/quick.html` is a tray panel with shortcut, microphone, last transcript and actions. A separate minimal Hold-status overlay could make capture and processing visible without opening that panel or moving focus. That interaction is the transferable design idea; macOS notch geometry and NSPanel APIs are platform-specific. This remains a proposal. The published demo GIF was inspected, but no macOS app was run, and the GIF does not demonstrate every current overlay state.

## 1. Latch an active Hold into Toggle

Implemented: [`DictationShortcutSessionController`](https://github.com/zachlatta/freeflow/blob/main/Sources/ShortcutCore/DictationShortcutSessionController.swift) changes an active Hold session to Toggle on `toggleActivated`, returning `switchedToToggle` rather than stopping capture. Hold release then does nothing. Toggle stop becomes armed only after `toggleDeactivated`, so the activation that latches cannot immediately stop the recording. New starts are blocked while transcribing. [`ShortcutCoreTests`](https://github.com/zachlatta/freeflow/blob/main/Tests/ShortcutCoreTests.swift) explicitly cover the latch, ignored Hold release, repeated Toggle activation and transcription guard. The README describes adding modifiers when Toggle extends the Hold binding.

Dipper proposal: keep mandatory Hold and offer this optional transition for longer dictations. Put session transitions in the core and native key matching in the desktop adapter. Acceptance should cover modifier ordering, repeated events, release after latching, cancellation and a fresh Toggle press to stop on real Wayland input. Source-level tests do not establish KDE shortcut compatibility.

## 2. Use context as a spelling reference with a bounded purpose

Implemented: [`AppContextService`](https://github.com/zachlatta/freeflow/blob/main/Sources/AppContextService.swift) reads app/window metadata and selected text through macOS Accessibility, captures an active-window screenshot, and requests a two-sentence activity summary from a model. Full-desktop screenshot fallback has a separate preference boundary. This is more than the README's broad description of reading nearby context. [`AppState`](https://github.com/zachlatta/freeflow/blob/main/Sources/AppState.swift), `beginRecording` and `startContextCapture`, starts context work after audio startup and can await its result after recognition.

[`PostProcessingService`](https://github.com/zachlatta/freeflow/blob/main/Sources/PostProcessingService.swift) instructs cleanup to preserve commands, paths, identifiers and meaning, and use context for formatting and spelling of words already spoken. These are prompt instructions, not guarantees against additions or meaning changes.

Dipper proposal: evaluate optional, minimal context separately from the raw path, using German speech with English technical terms, numbers, negations and ambiguous spoken self-corrections. Record context provenance and text processing separately in the Journal under the selected retention policy. Screen capture, remote context inference and Accessibility are new decisions, not consequences of adopting cleanup. KDE access remains unverified, and the accepted insertion contract deliberately removed Accessibility admission.

## 3. Keep personal vocabulary distinct from text replacement

Implemented: `mergedVocabularyTerms` in [`PostProcessingService`](https://github.com/zachlatta/freeflow/blob/main/Sources/PostProcessingService.swift) splits entries on newlines, commas or semicolons, removes empty terms and deduplicates case-insensitively. It adds the spellings to cleanup and edit prompts as high-priority terms. This establishes cleanup guidance, not a speech recognizer trained on personal vocabulary or deterministic abbreviation expansion.

Dipper proposal: start with an explicit spelling list if cleanup is selected. Evaluate near-miss names and technical terms alongside cases where an unrelated vocabulary term must not appear. Keep this separate from user-defined text replacement in issue #8 and from Corrections that establish confirmed wording. Preserve the selected raw transcription path when cleanup is disabled or fails.

## 4. Give selected-text editing its own intent and failure behavior

Implemented: [`AppState.resolveSessionIntent`](https://github.com/zachlatta/freeflow/blob/main/Sources/AppState.swift) supports automatic Edit Mode when a nonempty selection exists, or manual invocation with an extra modifier. Manual mode rejects missing selection and checks shortcut collisions. The original selection remains distinct from the spoken instruction. [`PostProcessingService`](https://github.com/zachlatta/freeflow/blob/main/Sources/PostProcessingService.swift) sends selected text and voice command separately; its edit path can return the selected text unchanged when unavailable. `AppState.processTranscript` also preserves the selection on edit failure rather than inserting the command as dictation.

Dipper proposal: prefer an explicitly invoked editing intent with review and undo before automatic replacement. Preserve the original selection, instruction and processed text as separate records. Verify stale selection, changed window, cancellation, no-op and model failure. This remains optional product scope; the current window guard does not prove selection identity or safe replacement on Wayland.

## 5. Make recovery explicit and avoid duplicate insertion

Implemented: [`AppState.copyLastTranscriptToPasteboard`](https://github.com/zachlatta/freeflow/blob/main/Sources/AppState.swift) provides Paste Again into the currently focused app and waits for shortcut release. The recognition pipeline saves the new raw transcript before cleanup so recovery cannot reuse the previous dictation after a downstream failure. `insertTranscriptWhenShortcutReleased` distinguishes definite non-insertion from an unverified Accessibility write. Only definite non-insertion permits automatic fallback; uncertainty directs the user to Paste Again. Clipboard restoration checks ownership through change count or matching transcript text.

Dipper proposal: provide a deliberate Journal action or shortcut for recovered text, with the selected dictation and destination clear. Preserve the existing rule that uncertain dispatch never triggers automatic retry. FreeFlow's default follows current focus; its opt-in original-field mode can activate an earlier app and can send Enter. Those behaviors conflict with Dipper's original-window guard and no-Enter contract and should not be copied. Clipboard timing and marker types are macOS details, not demonstrated Wayland solutions.

## 6. Separate capture evidence, model input and readiness feedback

Implemented: [`AudioRecorder`](https://github.com/zachlatta/freeflow/blob/main/Sources/AudioRecorder.swift) converts incoming buffers into a 16 kHz mono PCM16 WAV for upload and independently emits 24 kHz mono PCM16 for realtime recognition. It reports file-write failures and has a two-second buffer watchdog. [`RecordingCaptureTiming`](https://github.com/zachlatta/freeflow/blob/main/Sources/RecordingCaptureTiming.swift) uses generations to suppress stale readiness callbacks and retains capture start independently of the first nonzero RMS buffer. Readiness feedback waits for nonzero RMS. This is not evidence of voice activity trimming or a measured speech-onset accuracy bound. Optional output muting in `AppState` remembers whether output was already muted before restoring it.

Dipper proposal: reuse the distinctions and generation checks, and test silent starts, cancellation during initialization, device loss and conversion failure. Preserve complete capture audio under [ADR-0003](adr/0003-journal-storage.md); FreeFlow's normalized WAV is model input and does not establish preservation of original capture fidelity. Any output interruption needs its own restoration contract and PipeWire evidence.

## 7. Overlap useful work, then measure the whole path

Implemented: [`RealtimeTranscriptionService`](https://github.com/zachlatta/freeflow/blob/main/Sources/RealtimeTranscriptionService.swift) sends audio over a WebSocket during capture. [`AppState.resolveRawTranscript`](https://github.com/zachlatta/freeflow/blob/main/Sources/AppState.swift) tries realtime completion first and falls back to file upload on error, while propagating cancellation. Context inference overlaps recording, but unfinished context can still delay cleanup. Realtime failure followed by upload can increase stop-to-result latency. Startup and audio-buffer logs are instrumentation, not benchmark results.

Dipper proposal: measure shortcut-to-capture, first buffer, release-to-raw-transcript, optional processing and insertion dispatch separately, plus total cold and warm time. Record failure-path timing as well as successful timing. Retain the accepted worker-per-job lifetime, Whisper large-v3-turbo selection and 30-second pilot ceiling for recordings up to two minutes. Streaming, remote APIs or persistent workers need separate decisions. No measured FreeFlow-versus-Dipper latency advantage was found in the inspected sources; README words such as "fast" are positioning rather than transferable performance evidence.

## Evidence limits

No app was run, no provider integration was exercised, and no macOS or KDE behavior was measured. This was a focused code review, not a complete upstream audit or quality evaluation. Source tests demonstrate intended cases only until executed against the reviewed revision.

The README's privacy wording says FreeFlow does not store or retain data because it has no server. That must not be read as absence of local retention: [`PipelineHistoryItem`](https://github.com/zachlatta/freeflow/blob/main/Sources/PipelineHistoryItem.swift) includes transcripts, prompts, context screenshots and audio references; [`PipelineHistoryStore`](https://github.com/zachlatta/freeflow/blob/main/Sources/PipelineHistoryStore.swift) persists history in a local SQLite store. Provider retention is not established by this review. Dipper should preserve its explicit retention policy and avoid adopting the README wording.
