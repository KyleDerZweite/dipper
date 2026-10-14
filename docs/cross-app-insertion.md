# Guarded insertion into desktop text fields

Status: component contract independently reviewed with no blockers, 2026-10-14. This is the next useful
slice after the T3-only development app. Kyle requested direct insertion into
the already focused Discord composer and progression toward everyday desktop
use. UI redesign, optional cleanup and model changes are separate work.

## Outcome and accepted scope

Hold the configured key while a supported text field is focused, dictate,
release, and insert the raw transcript there. T3 remains supported. App identity
must no longer restrict admission to T3. Discord/Vesktop is the next actual
integration target, not a claim that every desktop application is supported.

Keep the established operating assumption that the user stays in the intended
field during a dictation. Observe the original KWin window throughout the job
and revalidate the currently focused editor before Paste. A departure permanently
revokes that job; returning never reactivates it. This slice does not add an
editor-object identity or follow focus to a new target.

Missing, ambiguous or unsupported target evidence suppresses automatic insertion
and clipboard publication. The recording and raw transcript remain in Journal
for explicit Copy. Clipboard publication and Paste remain separate OS operations
with the previously accepted residual race. Dispatch remains unverified without
external acceptance evidence, and uncertain delivery never triggers a retry.

## Responsibilities and interfaces

The existing core `Insertion` interface remains the testing and coordination
interface: reserve the target synchronously, arm it asynchronously, and dispatch
only with a valid permit. Core Dictation retains Capture, Recognition, Journal,
cancellation, stale-result suppression and worker-per-job ownership. The core
must not learn desktop app names, accessibility roles or D-Bus objects.

The desktop adapter owns KWin identity and Accessibility inspection. The KWin
observer reserves a normal external window with a nonempty identity and valid
PID, regardless of its application name. Reject the Dipper process itself. Keep
observation sequencing, snapshot revocation, session-loss handling and shutdown
ownership intact.

Accessibility proves a unique focused editable field belonging to the reserved
process, under a unique active top-level frame in that process. Accept ENTRY or
TEXT roles with editable, enabled, focused, sensitive, showing and visible states.
Do not require CSS classes, multiline state or the EditableText interface: the
working T3 composer exposes Text, not EditableText. Reject read-only, defunct,
stale and password fields. Terminal roles are not text-field targets for this
slice. Do not read field text, names, window captions or private attributes.

Retain the bounded complete Collection proof before selecting candidates. Query
active frames and focused editable fields separately, avoiding every ordinary
TEXT node in a browser. References must belong to the unique provider owner,
remain unique, and be part of the complete proven tree. Prove the selected field's
parent chain reaches the selected active frame, with at most 64 parent edges,
rejecting cycles, foreign owners and disappearing objects. The root and frame
must remain valid. Unsupported Collection providers fail with a useful reason;
there is no blind-Paste fallback.

KWin and AT-SPI do not share a window UUID here. Unique process ownership, active
frame ancestry and unchanged KWin snapshots form a conservative OS proof, not an
atomic cross-provider binding. Multiple frames are allowed only when exactly one
active frame and one qualifying field prove the target. Missing or ambiguous
active-frame evidence suppresses insertion. A transient UnknownObject can retry
the complete proof once inside the existing four-second deadline; never skip
failed objects or restart the deadline.

Before publishing the clipboard and again before dispatching Paste, preserve the
existing permit, window and accessibility checks and cancellation serialization.
The desktop adapter returns the existing honest outcomes. Journal schema,
retention, stored historical attempts and clipboard policy do not change.

User-facing lifecycle messages describe the focused text field and target window
rather than suggesting that T3 is the only possible application. Keep the
existing compact UI. Recognized text always remains available in Journal.

## Acceptance evidence

Use the existing Insertion/Dictation scenarios and desktop inspection interfaces.
Tests cover an external non-T3 reservation, Dipper/invalid target rejection,
unique editable ENTRY and TEXT admission, single-line fields, password/read-only
rejection, active-frame ancestry, cycles/foreign references, unsupported providers,
bounds/timeouts, departure-and-return, cancellation and unchanged clipboard on
suppression. Existing T3 and delivery-failure scenarios must keep passing.

Native verification must exercise metadata-only inspection against the running
T3 provider and a known non-T3 field. A disposable native editor can prove generic
field handling, including read-only/password counterexamples. Real Discord or
Vesktop Paste requires an empty owner-designated composer; insert a distinct
short test text, verify it, remove only that text, and never send a message.
Preserve existing drafts, clipboard content and running user applications.
A full isolated Hold/capture/Turbo/insertion journey should establish the same
adapter works through Dictation, with worker exit and Journal outcomes.

Record current head/runtime, target app identity, passed/failed/unexecuted checks,
and unavailable integration evidence. A synthetic editor does not establish
Discord compatibility. A passing native dispatch does not rewrite unverified
production Journal outcomes into confirmation.

## Remaining production work

This slice makes guarded insertion app-independent where target evidence is
available. It is not a production release or universal toolkit compatibility.
Qt providers without Collection, browser-specific gaps and terminal interaction
remain explicit compatibility work rather than silent bypasses.

The next separately reviewed slices are model library/download/Reload in issue
#22, explicit Corrections and retention in #14, and installable desktop delivery
with startup, versioned upgrades and recovery evidence. Cleanup stays optional;
Turbo remains the live Auto baseline. Mixed German/English within a recording
needs quality evaluation, not a model switch based on monolingual speed alone.
UI polishing, streaming, mobile and OTA implementation remain deferred.
