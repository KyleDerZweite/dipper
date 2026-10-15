# Window-guarded desktop insertion

Status: revised for Kyle's accepted direction and independently reviewed without blockers on 2026-10-15. Kyle requested much
freer insertion into the field he has already focused. This supersedes the
2026-10-14 Accessibility admission contract and the later single-frame proposal.
One useful PR remains the delivery model. UI redesign and model changes are separate.

## Outcome and accepted scope

Focus the intended input, hold the configured key, dictate and release. Dipper
publishes the transcript and dispatches Ctrl+V to the original normal external
window. The target application handles the Paste in its currently focused control.
Accessibility availability, editor roles, frame states and application names no
longer gate insertion. No new setting or alternate insertion mode is needed.

Kyle stays in the intended field throughout the dictation. Dipper does not prove
field identity, editability, password status or read-only status. A password field
can receive text; a read-only control may ignore Paste. Missing field focus may
produce no insertion or application-specific behavior. Dipper dispatches Ctrl+V only and never sends Enter or retries uncertain delivery. This is intentionally window
validation, not field validation or universal application compatibility.

Keep the original-window guard. Reserve a normal external KWin window with a
nonempty window/application identity and valid PID. Reject Dipper itself. An
observed departure permanently revokes that job, including departure and return.
Invalid identity, observer failure, session loss, cancellation, shutdown and stale
results continue to suppress automatic insertion. Do not follow focus to another
window. Same-window field changes remain the user's responsibility.

## Responsibilities and interfaces

The core Insertion interface remains reserve/arm/dispatch. Core Dictation owns
Capture, Recognition, Journal, cancellation, stale-result suppression and
worker-per-job lifetime. It does not learn desktop app names or KWin internals.

The desktop adapter owns target qualification, KWin observation, clipboard
publication and Ctrl+V dispatch. Remove Accessibility inspection from admission
and delete its unused implementation. Keep ordered observer reports, authenticated
sender, snapshot validation, permanent permit revocation and dispatch/cancellation
serialization. Validate the permit and original active window at arm, before
clipboard publication and again before Paste. Recheck permit validity after the
snapshot and immediately before key dispatch.

Clipboard publication and key dispatch are separate OS operations, with the
previously accepted residual focus race. A focus change after clipboard publication
can leave the transcript on the clipboard while suppressing Paste. Existing
clipboard policy and honest delivery outcomes remain unchanged. Dispatch is
unverified unless external acceptance evidence confirms the target received it.
Journal schema, retained historical attempts, settings and model policy do not
change. Transcripts remain available in Journal after suppression or failed Paste.

Use concise UI copy describing the original window check. Do not imply field
validation or that dispatched Paste establishes successful insertion.

## Acceptance evidence

Run existing Insertion/Dictation cancellation, failure, stale-result, focus-loss,
clipboard and shutdown tests. Observer coverage must preserve external non-T3
reservation, invalid/self-target rejection, observer/session failure and permanent
departure-and-return revocation. The target path must have no Accessibility calls.

Verify actual Discord/Vesktop using an empty owner-designated composer: run
Hold/capture/Turbo/Paste, compare the inserted text with that job's raw transcript,
remove only the test text and never send. Preserve clipboard bytes, user drafts,
normal settings and other running applications. Verify a departure-and-return
job suppresses Paste and clipboard publication. Use a private store and owned
synthetic audio input for repeatable full-chain tests; reap workers and remove
only owned runtime resources afterward. Native tests in another editor establish
only that editor's behavior. Tests must not insert into a user password field or
terminal merely to demonstrate the absence of field restrictions.

Record current revision, runtime and evidence, distinguishing implemented behavior,
actual target acceptance and unexecuted checks. Existing stricter-field tests are
historical evidence for the superseded contract, not current acceptance rules.

## Remaining production work

This slice broadens desktop insertion while retaining the original-window guard.
It is not a production release. Model library/download/Reload (#22), explicit
Corrections and retention (#14), and installable delivery with upgrade recovery
(#26) remain separate slices. UI polishing, streaming, mobile and OTA are deferred.
