# Journal capture and recovery research

Research date: 2026-10-13. Status: recommendations for the first raw voice-to-T3 slice, not an accepted implementation contract.
Repository reviewed: `research/journal-recovery`, base `a743e079caa86c2e8cb034c52409648675025a12`; initially clean.

Recommend a bounded audio callback queue, one sequential audio writer, and a Journal metadata writer using SQLite WAL with `synchronous=FULL`.
Start with uncompressed WAV that preserves the negotiated capture samples. Record audio during capture and acknowledge completion only after explicit audio finalization, synchronization, and the corresponding metadata commit.
Recover unfinished recordings as interrupted, preserve their files, and expose missing or uncertain data. Restart must initiate neither recognition/replay nor insertion.
The numerical crash-loss limit remains an owner decision. These recommendations do not establish a resource budget or a retention default.

## Scope and evidence

The accepted foundation is Rust with Tauri 2 on Fedora KDE Wayland. [App design](app-design.md), [Journal requirements](journal.md), and [ADR-0003](adr/0003-journal-storage.md) require separate SQLite metadata and lossless audio, durable recording identities, distinct recognition attempts, and preserved interrupted data.
The accepted pilot rejects new recordings when storage is insufficient, preserves existing records, and never automatically purges them. The first slice delivers raw transcription to T3 with comprehensive retention. Search, replay UI, consistent export, cleanup, and the reduced-retention comparison belong to later slices.
The reduced configuration must retain text and explicit corrections without durable audio or detailed diagnostic logs. Its exact text artifacts and transient collection behavior remain open and do not block the comprehensive raw chain.

This report reviews primary documentation and upstream source. It contains no app implementation or measured runtime results.
No software was installed; no microphone, model, private recording, or settings were accessed. Suggested checks below remain unexecuted.

## Keep disk operations outside the audio callback

CPAL delivers audio through a callback and supports device enumeration and configuration queries. Its documented callback thread can have high priority.
PipeWire explicitly forbids file operations and blocking operations in a realtime process callback. [CPAL 0.18.2](https://docs.rs/cpal/0.18.2/cpal/), [PipeWire streams](https://docs.pipewire.org/page_streams.html)

Proposed implementation:

- The callback copies complete interleaved frames into preallocated bounded blocks and records frame counts and timestamps. It performs no SQLite, disk I/O, encoding, transcript logging, or UI calls.
- An audio writer consumes those blocks sequentially and writes buffered batches. It reports accepted, written, and successfully synchronized frame counts separately.
- A Journal writer serializes short metadata transactions. Recognition and UI read retained artifacts through Journal instead of opening writer internals.
- Queue overflow or an audio discontinuity stops the recording with an explicit failure and preserves available audio. Never silently drop samples and report complete capture.
- Stop/release closes capture, drains accepted blocks, and waits for writer completion before passing a complete recording to Recognition. A failed recording is retained for explicit review and must not trigger automatic insertion.

A bounded queue absorbs a finite disk stall; it cannot guarantee capture under an indefinitely stalled filesystem. For queue duration `q`, sample rate `r`, channels `c`, and stored bytes per sample `b`, sample memory is `q * r * c * b` bytes, plus block bookkeeping.
Choose queue capacity from measured stall behavior and memory use. Do not accumulate the full comprehensive recording in RAM just to write it at release.

## Resolve the default input, then pin the actual device

At each recording start, discover the current system default source or the explicitly selected source. Resolve it to a concrete device and keep it fixed until stop.
A changed default affects the next recording. Device disappearance stops capture, preserves available audio, and suppresses automatic insertion, as already accepted in [app design](app-design.md).

CPAL is a practical candidate, but its backend must be explicit and observed. The current README lists ALSA as the Linux default and PipeWire as optional.
Pinned v0.18.2 source instead prefers PipeWire when its feature is enabled and initialization succeeds, then PulseAudio, then ALSA.
Select and report the intended host rather than infer routing from `default_host()`. [CPAL README](https://github.com/RustAudio/cpal), [v0.18.2 host selection](https://github.com/RustAudio/cpal/blob/v0.18.2/src/platform/mod.rs)

The PipeWire implementation's `default_input_device()` represents a symbolic default route. Its default-route monitor permits rerouting.
Concrete node devices set `target.object` and do not watch default changes; their IDs use the node name. Creating a stream from the symbolic default is therefore insufficient for Dipper's fixed-device contract. A native adapter must resolve the current default to a concrete node before starting capture. [CPAL PipeWire host](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/pipewire/mod.rs), [CPAL PipeWire devices](https://github.com/RustAudio/cpal/blob/v0.18.2/src/host/pipewire/device.rs)

Pinning also needs session-manager behavior. PipeWire documents `node.dont-reconnect`, `node.dont-fallback`, and `node.dont-move` alongside `target.object`.
WirePlumber can otherwise reconnect or move a stream. The inspected CPAL device properties do not set these restrictions. Prefer a narrow native PipeWire adapter that can enforce them, or establish equivalent behavior before selecting CPAL for this contract. Preserve upstream packages unchanged. [PipeWire properties](https://docs.pipewire.org/page_man_pipewire-props_7.html), [WirePlumber linking policy](https://pipewire.pages.freedesktop.org/wireplumber/policies/linking.html)

Treat stream invalidation, device loss, and backend errors as lifecycle events outside the callback. CPAL's error categories distinguish unavailable devices, permissions, host failure, and overruns.
Their presence in the API does not prove delivery on Kyle's actual backend. Record actual readiness only when capture delivers valid frames, rather than when stream construction returns. [CPAL errors](https://docs.rs/cpal/0.18.2/cpal/enum.ErrorKind.html)

## WAV first, FLAC after evidence

WAV stores an explicit format and audio data in RIFF chunks whose headers include lengths. Interrupted recording can leave those lengths stale.
Do not assume a fixed 44-byte header: valid writers can emit additional chunks. Persist the actual data offset and format needed to interpret Dipper's own output. [Microsoft RIFF description](https://learn.microsoft.com/en-us/windows/win32/xaudio2/resource-interchange-file-format--riff-)

Hound 3.5.1 documents `flush()` as updating the WAV header and flushing its writer while allowing capture to continue.
Its `finalize()` reports errors that the destructor cannot report. These operations do not establish power-loss durability; explicitly synchronize the underlying file afterward.
Use exclusive file creation instead of its convenience `create()` method, which overwrites an existing path. [Hound WAV writer](https://docs.rs/hound/3.5.1/hound/struct.WavWriter.html)

WAV avoids compression CPU during capture and makes sample storage size predictable. This is a design inference, not a benchmark.
Preserve all negotiated capture samples, channels, and rate. If capture supplies `f32`, retain floating-point WAV rather than silently quantize to 16-bit integer audio.
Losslessness refers to the negotiated stream delivered to Dipper. PipeWire can convert audio before delivery, so this does not promise bit equality with the microphone ADC or bypass system resampling. Record the delivered format and known upstream conversion separately. [PipeWire stream conversion](https://docs.pipewire.org/page_streams.html)
Model input can be resampled or normalized separately; record that transformation and keep the original capture distinct.
Confirm writer support for every accepted sample format and reject an unsupported format before claiming recording readiness.

FLAC provides lossless integer audio compression, frame checksums, and stream metadata that can leave total sample count and MD5 unknown.
A truncated final frame need not invalidate all preceding frames, but the selected decoder's partial-file behavior still needs verification. Converting arbitrary floating-point capture to integer FLAC is not automatically lossless. [RFC 9639](https://www.rfc-editor.org/rfc/rfc9639.html)

Defer FLAC unless measured WAV growth warrants its encoding and recovery work. A later conversion must verify sample equality, synchronize the replacement, and commit its reference before any explicit removal of the original.

## Order acknowledgments across the two stores

SQLite WAL lets readers coexist with a writer, but still permits only one writer at a time. WAL must remain on a suitable local filesystem.
Its automatic checkpoint normally starts around 1000 pages and can make the triggering commit slower. Keep reader transactions short and measure commit/checkpoint latency before adding custom checkpoint scheduling. [SQLite WAL](https://www.sqlite.org/wal.html)

Use `synchronous=FULL` for acknowledged metadata lifecycle transitions. In WAL mode it adds a WAL sync at commit.
`NORMAL` can lose recently committed transactions after OS crash or power loss even though WAL remains consistent. A WAL checkpoint is not the same operation as Dipper's audio checkpoint.
Do not select `NORMAL` merely because it is faster without accepting its durability tradeoff. Set connection-specific settings explicitly and verify them. [SQLite synchronous settings](https://www.sqlite.org/pragma.html#pragma_synchronous)

Proposed comprehensive capture sequence:

1. Journal commits a fresh recording identity and an unfinished status, retention snapshot, planned unique audio path, and selected input. Reject capture if this commit fails.
2. Create the audio file exclusively, write its format header, and establish the stored format and data offset. Synchronize the new file and its parent directory, then commit its format and data offset before acknowledging a durable file association. If initialization fails, retain the identity with a failure when writable.
3. Capture starts and reports actual readiness. The writer continuously writes batches. At the chosen durability cadence, flush audio and update its header, synchronize the file, then commit the corresponding synchronized frame count to SQLite.
4. On release, cancellation, or failure, stop capture and drain accepted frames. Explicitly finalize the header and synchronize the file. Commit complete, canceled, or failed status with available frames, completeness, and error information only after that succeeds.
5. Only a complete capture is supplied automatically for inference. Commit the worker's raw transcript or failure separately under its existing attempt identity. Retries always add attempts.
6. Before an insertion side effect, commit its intent with the transcript/attempt reference and target context. Record the observed outcome afterward. A pending or unverified insertion remains uncertain after a crash and never triggers an automatic retry.

Steps 2 through 4 describe the capture branch. After step 1, commit the recognition-attempt identity and immutable configuration before starting model preparation, in parallel with capture initialization. Record preparing/waiting-for-audio separately from actual inference. A failed capture cancels that attempt without supplying incomplete audio. This preserves the accepted load-on-hold policy rather than postponing model loading until step 5. See the [worker contract proposal](worker-lifecycle-research.md).

The ordering makes metadata acknowledgment follow audio synchronization; it does not create a transaction spanning both stores. If step 3 or 4 audio sync succeeds but its SQLite commit fails, recovery may discover more audio than the stored count. Keep it and report interrupted or uncertain state.
If recording metadata commits but the audio path is absent or shorter than promised, mark missing or inconsistent audio. Never infer completeness from the row alone.

Rust `File::sync_all()` reports synchronization failures; dropping a file ignores close errors. Linux requires an explicit parent-directory `fsync()` to synchronize a new filename. Successful writes alone do not promise durable storage, and writes can be partial. [Rust File](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all), [Linux fsync](https://man7.org/linux/man-pages/man2/fsync.2.html), [Linux write](https://man7.org/linux/man-pages/man2/write.2.html)

Use one stable capture path per identity initially. Avoid introducing a final rename solely to signal completion; Journal owns that status.
## Minimal Journal contract and recovery

These are proposed logical records and interface responsibilities, not a package layout or final SQL schema:

| Record | Minimum first-slice information |
| --- | --- |
| Recording | Identity, start/end times, capture status, retention snapshot, input identity, available frames, completeness, error category |
| Audio association | Relative private-store path, stored sample format/rate/channels, data offset, synchronized frame count, availability/recovery state |
| Recognition attempt | Own identity, recording reference, worker/configuration/artifact versions and hashes, model-input provenance, timings, raw transcript or failure |
| Insertion attempt | Own identity, recognition reference, intended target context, pending/observed/unverified/failed outcome, error category |
| Diagnostic artifact | Recording/attempt reference, private log location, available range and write failure; comprehensive logs append outside the callback |

Journal exposes begin-recording, audio-checkpoint, finish-capture, begin/finish-attempt, record-insertion, read-artifacts, and recover operations. Capture owns the device and callbacks; Journal owns storage ordering and durable acknowledgment through its audio writer. Recognition receives a read-only artifact reference and adds an attempt through Journal.
Desktop controls depend on Dictation. Dictation coordinates these interfaces without receiving SQLite connections or writable audio paths. No Tauri dependency belongs in the core storage contract.

On startup, acquire exclusive ownership of the store before recovery. Open SQLite normally so it can recover its WAL; never delete a leftover `-wal` to resolve a startup problem.
The WAL is part of the database's persistent state. Copying the main database alone can lose committed records. [SQLite WAL file handling](https://www.sqlite.org/wal.html#the_wal_file)

Find unfinished rows first and mark them interrupted, including pending recognition and insertion work. Retain successfully committed transcripts and separate insertion uncertainty from capture status.
Inspect the associated audio's format, actual length, frame alignment, and last recorded synchronization point. Expose a recoverable prefix through a reader or derived repaired copy without overwriting the interrupted source.
Bytes beyond the last checkpoint are available candidates, not proof of complete speech or power-loss durability. A header rewrite can itself be interrupted; use persisted format information rather than trust damaged header lengths.
PCM contains no frame checksum. Plausible bytes or file length cannot prove absence of corruption, gaps, or zero-filled unwritten space.

Preserve orphan files and report their identities where recoverable. Do not attach them to a different recording or invent missing model/input metadata.
Scan directory names and metadata for orphans; avoid decoding or hashing the entire historical audio library at every start. Validate an artifact when it is read.
Recovery must be idempotent. If storage remains full and interrupted status cannot be committed, expose the derived status as unpersisted and retry the reconciliation when storage becomes writable.

## Storage-full behavior and private files

Perform admission checks before capture and check every write, flush, sync, and SQLite commit. Available-space checks are advisory: another process, quotas, or an expanding WAL can exhaust space later.
The admission floor must cover measured initial audio and metadata work plus a chosen margin; its numerical value remains unselected. Do not preallocate a guessed maximum recording size or silently impose a duration limit.

SQLite can report `SQLITE_FULL` for its main store or temporary files. `SQLITE_IOERR` and full-disk errors can roll back a statement or an entire transaction.
Check transaction state or roll back explicitly rather than assume earlier updates survived. Preserve the original failure category. [SQLite result codes](https://www.sqlite.org/rescode.html#full), [SQLite transaction errors](https://www.sqlite.org/lang_transaction.html#response_to_errors_within_a_transaction)

After a mid-capture storage failure, stop, retain the written prefix, and report failure separately from start rejection. If metadata cannot be written, the prior unfinished identity and file support later reconciliation.
Further logging can fail too. Display a short in-memory error and do not claim that a diagnostic entry was durably saved.

Keep the Journal and recordings outside Git in a user data directory; diagnostic state belongs in a user state directory. Use user-only directories and files, including SQLite sidecars and logs.
Do not use a cache directory for retained recordings or the runtime directory for crash recovery; the latter does not survive reboot/logout.
XDG specifies these location classes, but does not enforce permissions on Dipper's data directory. [XDG base directories](https://specifications.freedesktop.org/basedir/latest/)
Proposed initial modes are `0700` for private directories and `0600` for files. Create them with restrictive permissions from the start and use application-generated relative paths.

## Calculated growth and measurements

Uncompressed sample payload is `seconds * sample_rate * channels * bytes_per_sample`, excluding headers, database, WAL, logs, and filesystem allocation.
These examples are arithmetic estimates, not selected capture formats, observed compression ratios, resource limits, or usage forecasts:

| Stored capture | Per minute | Five minutes | One hour |
| --- | ---: | ---: | ---: |
| 48 kHz, mono, 16-bit integer | 5.76 MB | 28.8 MB | 345.6 MB |
| 48 kHz, mono, 32-bit float | 11.52 MB | 57.6 MB | 691.2 MB |
| 48 kHz, stereo, 32-bit float | 23.04 MB | 115.2 MB | 1,382.4 MB |

MB means 1,000,000 bytes. The accepted 13-clip duration ranges total 820 to 1140 seconds, yielding 157.44 to 218.88 MB for mono 48 kHz float samples.
For FLAC, measure actual speech/noise compression and encode/decode CPU; there is no justified fixed savings percentage.

Measure capture readiness, callback gaps, queue high-water mark, writer throughput, flush/sync/metadata latency, release-to-recognition handoff, and total release-to-T3 insertion. Include the actual input/backend/format, filesystem and mount, cold/warm conditions, disk pressure, retention policy, and individual failed runs.
Measure CPU, RSS, idle wakeups, logical bytes, allocated bytes, WAL growth, diagnostic growth, and recovery startup time separately from model load/inference.
Compare later reduced retention on the same scenarios. A disk spool deleted afterward still persisted audio and does not meet that configuration's stated requirement.

## Checks required for the first slice

| Scenario | Acceptance evidence |
| --- | --- |
| Normal release and cancel | Complete capture preserves every accepted frame; cancel preserves available audio and distinct status; raw attempts remain separate; only the normal valid T3 chain inserts. |
| Process kill during each persistence step | Restart finds the identity/files, labels unfinished work interrupted, retains the recoverable prefix, and performs no automatic recognition, replay, insertion, or duplicate retry. Repeat restart to establish idempotence. |
| Missing, orphaned, stale-header, or shortened audio | Recovery preserves originals and reports incomplete/missing/uncertain artifacts. It never manufactures a complete record or associates unrelated audio. |
| Start rejection and mid-capture disk full | New capture is visibly rejected when insufficient; later write/sync/SQLite failure stops with available data preserved. Existing artifacts remain intact, including when recovery cannot commit its status. |
| Stalled writer and queue overflow | Callback stays bounded; no silent frame drops; failure retains its available prefix; cancellation/release completes without a callback waiting on disk. |
| Default changes and USB microphone disappears | Current recording keeps its selected source; the next uses the new default. Disappearance stops with a failure and no automatic insertion or fallback source. Test actual PipeWire/WirePlumber behavior. |
| Crash around insertion and permission denial | Persisted transcript remains available; pending/unverified insertion never retries on restart. Denied audio/store permissions produce honest errors without changing system settings. |
| OS crash/power-loss durability | In an authorized disposable environment, establish the actual synchronized boundary and detect torn headers/metadata mismatches. Process-kill tests alone do not establish this evidence. |

## Decisions and unexecuted evidence

Before making the raw slice Ready, agree on what a durable acknowledgment means for app crash versus OS crash/power loss, then select the audio synchronization cadence and numerical allowable loss.
A nominal timer interval alone is not a bound: include callback backlog, writer backlog, scheduling delay, filesystem sync latency, and failure detection. Without a proven bound, report the last successfully synchronized frames and make no maximum-loss promise.
Review the proposed completion/insertion ordering, queue-overflow outcome, audio format, and admission margin with the first-slice contract.
Selecting a permanent retention default, a FLAC conversion policy, every future text artifact, or a resource budget is unnecessary for this slice.

The implementation owner owns the first-slice fault checks and actual Fedora KDE microphone/T3 evidence. Kyle owns consequential durability/retention decisions and pilot acceptance.
All runtime, disk-pressure, process-kill, power-loss, microphone-routing, performance, and T3 checks above are unexecuted. This report supports design review and cannot establish app durability, native responsiveness, or pilot acceptance.
