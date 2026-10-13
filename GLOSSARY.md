# Dipper

Dipper's domain is voice dictation, text insertion, and a journal that connects speech with recognition results and explicit corrections.

## Language

**Dictation**:
A user's spoken input intended to become text in a target application.
_Avoid_: Voice command when the purpose is text entry.

**Target application**:
The application intended to receive the dictation's text.
_Avoid_: Active application when the intended recipient has not been established.

**Recording**:
A captured instance of speech associated with a dictation, including one that is canceled or fails.
_Avoid_: Recognition attempt, transcript.

**Capture audio**:
The complete audio captured for a recording before trimming or voice activity filtering.
_Avoid_: Model input when referring to the complete capture.

**Model input**:
The audio supplied to speech recognition, which may differ from the capture audio after preparation.
_Avoid_: Capture audio when referring only to the supplied audio.

**Recognition attempt**:
One attempt to derive text from a recording using a speech recognition configuration. A retry is another attempt for the same recording.
_Avoid_: Recording, retry result used as a replacement for an earlier attempt.

**Raw transcript**:
The text returned by a recognition attempt before text processing.
_Avoid_: Processed text, confirmed wording.

**Text processing**:
The transformation of a raw transcript into text intended for insertion, such as formatting or handling spoken self-corrections.
_Avoid_: Speech recognition when the input is already text.

**Processed text**:
The output of text processing applied to a raw transcript.
_Avoid_: Raw transcript, confirmed wording.

**Insertion outcome**:
The recorded result of an attempt to place text in the target application, including a failure or an unverified result.
_Avoid_: Confirmed wording, accepted text inferred solely from insertion.

**Correction**:
An explicit user revision associated with a dictation's text.
_Avoid_: Automatic formatting or an unconfirmed inferred edit.

**Confirmed wording**:
The wording explicitly confirmed by the user for a dictation.
_Avoid_: Raw transcript or processed text without user confirmation.

**Journal**:
The connected history of recordings, recognition attempts, text processing results, insertion outcomes, and corrections, including cancellation and failure records. Its available history depends on the retention policy.
_Avoid_: Transcript history when audio, attempts, or outcomes are absent.

**Replay**:
The evaluation of an existing recording with a recognition or text processing configuration.
_Avoid_: New recording, ordinary live dictation.

**Mode**:
A dictation configuration for an intended kind of output, such as raw text, a message, or a technical prompt.
_Avoid_: Model when referring to the whole dictation configuration.

**Feature parity**:
Equivalent behavior in specified user scenarios compared with a reference product.
_Avoid_: Parity inferred only from a shared shortcut or speech model.

**Retention policy**:
The user's rules for which dictation data and diagnostic logs are collected and kept, and for how long.
_Avoid_: Complete journal as an assumption for every user.

**Comprehensive retention**:
A retention policy that keeps the full available dictation history and diagnostic logs, including cancellations and failures, without automatic expiry.
_Avoid_: Default retention when no default has been chosen.
