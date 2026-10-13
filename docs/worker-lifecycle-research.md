# Supervised Whisper worker lifecycle

Research date: 2026-10-13. Source inspection only. This report proposes the detailed worker contract for the accepted Rust core,
Tauri 2 desktop, and Whisper large-v3 through whisper.cpp/Vulkan on the RX 7700S. It does not make the first slice Ready. The
inspected whisper.cpp revision is `0c7be696371069e0baa2515ed7aa08d5a5473aa6`. The inspected Tokio pages display 1.53.3 and tokio-util pages display
0.7.20 on this date. Links use verified latest-documentation URLs; neither version is a selected Dipper dependency.

Use one small native worker per job. Start preparation at hold, keep capture independent, send finalized audio after release, and
exit after the result. Recommend immediate supervisor cancellation with explicit reaping, a single-job protocol, and insertion
permission owned by Dictation. Reject additional holds while a job remains active, as the owner endorsed. No idle grace, streaming
recognition, queued dictation, or intentionally resident model is proposed here. The accepted directions and existing timing limits
belong to [app design](app-design.md), [model lifetime](model-lifetime.md), [ADR-0002](adr/0002-rust-core-tauri-desktop.md), and
[ADR-0003](adr/0003-journal-storage.md).

## What the pinned runtime supports

The C API separates `whisper_init_from_file_with_params` from `whisper_full`. Initialization accepts GPU selection; inference
receives 16 kHz floating-point PCM and explicit decoding parameters. Extract final segments through `whisper_full_n_segments` and
`whisper_full_get_segment_text`, then call `whisper_free`. Concurrent use of one context is unsupported. Inference offers
encoder-begin and abort callbacks, but initialization has no cancellation parameter. These APIs support preload before complete
audio exists. [Pinned
header](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/include/whisper.h).

The full initializer also allocates recognition state. Scheduled graph work sets backend thread counts; encode and decode check the
abort callback after their graph computation. Encoder-begin cancellation can break the loop and still return zero. Audio shorter
than the effective 100 ms processing window can also return zero without text. GPU selection indexes enumerated GPU/IGPU devices;
CPU backends remain part of execution. These facts establish neither immediate Vulkan cancellation nor GPU-only CPU use. Check
actual device identity and an explicit canceled flag independently of the native return code. [Pinned
implementation](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/src/whisper.cpp).

The CLI loads the context before reading each input. It accepts `-` as stdin, rejects missing named files before loading, defaults
to at most four CPU threads and one processor, and frees the context before exit. It has no structured preparation handshake or
one-job cancellation protocol. [Pinned
CLI](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/examples/cli/cli.cpp). Its stdin reader
accumulates encoded audio bytes until EOF, then decodes that buffer to model input. It does not recognize while that input is
arriving. [Pinned audio
helper](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/examples/common-whisper.cpp).

Therefore the CLI can provide coarse preload: launch it with `-` at hold, keep stdin open during capture, then write complete
encoded audio and close stdin after release. This is an inference from source, not a tested integration. Sending live capture into
its pipe would couple capture to pipe backpressure. An unfinished named file is also unsuitable. Recommend a library worker because
its explicit readiness and finalized-file request avoid full encoded-audio pipe copies and log parsing. A CLI adapter remains
feasible if those costs and weaker observability are accepted; CLI capability alone does not prove capture quality.

## Responsibilities and minimal interface

Dictation owns admission, the current job generation, cancellation, target validity, and permission to dispatch insertion. Capture
owns readiness and complete capture audio, including preservation on cancellation or failure. Recognition owns worker supervision,
native settings, model-input preparation, raw transcript, provenance, and attempt outcome. Journal owns durable recording and
attempt identities. The worker owns only native recognition resources and job-local input preparation. It cannot insert, delete
capture audio, change retention, select a new target, or write an independent journal.

Recommend these core operations, independent of Tauri and the worker transport:

| Operation | Contract |
| --- | --- |
| `prepare(job, config)` | Starts one worker and returns a preparation handle without waiting for native initialization. |
| `recognize(handle, finalized_audio)` | Supplies one complete immutable capture reference after release, with recording and attempt identity. |
| `cancel(handle)` | Revokes acceptance of results immediately and requests worker termination; completion reports cleanup separately. |
| `shutdown()` | Rejects admission, cancels the current job, and joins supervision before desktop runtime exit. |

Keep configuration immutable for the job, including model identity, pinned runtime/build, language, decoding settings, CPU thread
count, and resolved GPU identity. Allocate durable recording and recognition-attempt identities through Journal; define the exact
attempt-creation checkpoint in its contract. Also use a process-local generation for active-job matching. Never use the OS PID as a
recording identity or insertion authorization.

Recommend launch arguments for protocol version, job identity, and immutable configuration, followed by one bounded JSON-line
`Transcribe` request on stdin. The request identifies finalized audio, sample count/format, and provenance. In normal operation the worker emits
`Ready`, `Started`, and exactly one `Result` or `Failed` event on stdout; every event carries protocol version and job/attempt
identity. Ready means full initialization finished, not microphone readiness or a latency promise. Result carries the raw transcript
and measured preparation/inference timings. Native logs go to stderr. Progress events are optional and coalesced.

Send the request when audio is finalized even if loading is unfinished. Keep at most one pending request. Reject a second request,
malformed frame, unknown version, wrong identity, or out-of-order terminal event. Set finite frame, transcript, diagnostic-buffer,
and queue limits; report overflow explicitly. Do not silently truncate transcript text or apply a new recording-duration cap.
Numerical bounds need the first-slice contract and five-minute stress evidence. `LinesCodec::new_with_max_length` supplies bounded
decoding, whereas its default constructor permits an unlimited buffered line. [Tokio-util
codec](https://docs.rs/tokio-util/latest/tokio_util/codec/struct.LinesCodec.html).

## Supervision, cancellation, and shutdown

Use Rust process supervision behind Recognition. Spawn directly with arguments, take ownership of stdin/stdout/stderr, drain both
output pipes concurrently, and retain the child handle until termination. A long-running supervisor task owns the child, deadlines,
protocol state, and reader tasks. Keep stdout parsing and stderr collection bounded, independent of UI subscribers and Journal
writes. Avoid a result-only `output()` call for an interactive worker. [Tokio process
API](https://docs.rs/tokio/latest/tokio/process/index.html).

Tokio `Child::wait` is cancellation safe, but closes stdin unless its handle was taken. `kill` sends SIGKILL on Unix and waits;
`start_kill` only requests killing and must be followed by successful waiting or reaping. Dropping a process future does not kill
its child by default. `kill_on_drop(true)` is a fallback, not the normal cleanup path. Preserve the supervisor across canceled
callers and await its terminal cleanup. Standard-library `Child` also needs explicit waiting. [Tokio
Child](https://docs.rs/tokio/latest/tokio/process/struct.Child.html), [Rust
Child](https://doc.rust-lang.org/std/process/struct.Child.html).

For the first worker, recommend no `Cancel` wire command. Cancellation first invalidates the job's insertion permit in Dictation,
independently stops and finalizes Capture if still active, and preserves available audio through Journal. The supervisor then kills
and reaps the worker during loading, waiting, or inference. This removes dependence on an unresponsive loader or GPU callback.
Normal completion publishes the result, frees the native context, and exits. Require both a valid terminal event and successful
worker exit before accepting recognition as complete. A result followed by crash remains a failed attempt.

During shutdown, revoke insertion before waiting, stop capture while preserving available audio, terminate the worker, join pipe
readers, and record honest outcomes. Use separate preparation, recognition, and post-result-exit deadlines; select their numbers
from app measurements rather than the existing load range. A deadline triggers termination, not proof of completed cleanup. If
reaping cannot be confirmed, report cleanup failure and retain supervisor ownership. Never claim a hard bound for OS or driver
termination without target evidence.

Prefer a direct worker that creates no subprocesses. Unix `process_group(0)` can isolate a worker group; signaling `-pgid` reaches
that group if helpers are introduced. Killing a direct child alone does not establish descendant cleanup. Avoid a shell and delayed
signals to a saved PID/PGID after reaping. [Tokio
Command](https://docs.rs/tokio/latest/tokio/process/struct.Command.html#method.process_group), [Linux
kill](https://man7.org/linux/man-pages/man2/kill.2.html).

Parent-crash cleanup requires an explicit mechanism beyond orderly shutdown. Recommend a small worker control-reader thread, started
before loading, that monitors stdin through inference. Keep the parent writer alive until worker exit; EOF always means lost
supervision and must terminate the worker without waiting for native cooperation. Keep its request queue at one. Abrupt process exit
does not run Rust stack destructors, so the worker must own no durable journal writes. [Rust
exit](https://doc.rust-lang.org/std/process/fn.exit.html). Linux `PR_SET_PDEATHSIG` is an alternative, but refers to the spawning
thread and has a setup race if the parent already died. Verify thread lifetime and parent identity if chosen. It is not a portable
substitute for a reviewed contract. [Linux parent-death signal](https://man7.org/linux/man-pages/man2/PR_SET_PDEATHSIG.2const.html).

## User-visible state and race handling

Start Capture and preparation independently at hold. Show Recording only after Capture reports readiness. After release show
Preparing if loading remains, otherwise Transcribing; show finalizing or insertion feedback as appropriate. Do not turn a native
progress callback into promised elapsed-time percentages. These phase names are recommendations; no polished overlay is selected.

Reject another hold while recording, preparing, recognizing, inserting, or cleaning up. Ignore key-repeat begins. Match release to
the admitted hold so a rejected hold's release cannot end another recording. Become idle only after capture finalization, worker
reaping, and settlement of any insertion dispatch. Preserve a received raw transcript even if a later worker-exit failure makes the
attempt unsuccessful. Keep errors visible and permit explicit retry from retained audio as a new attempt once cleanup completes.

Serialize cancel and insertion authorization in Dictation. Before dispatch, require the current generation, matching attempt,
uncanceled state, and valid T3 target. Late Ready, Result, and failure events cannot reopen an invalidated job. Insertion checks
permission again at its actual dispatch boundary. Once a paste has been dispatched, cancellation cannot promise to retract delivered
text. Record that boundary and preserve an unverified insertion outcome when target acceptance is unknown. Do not automatically
retry an unclear paste.

| Case | Recommended outcome |
| --- | --- |
| Release before load finishes | Preserve and finalize complete audio, retain one request, wait for preparation, then recognize. |
| Cancel during load or inference | Revoke insertion, preserve audio, terminate and reap; ignore buffered late results. |
| Spawn, model, GPU, or protocol failure | Record a failed attempt, preserve audio, skip insertion, expose the reason. |
| Recognition fails while capture continues | Show recognition failure and continue capture until release/cancel; preserve its audio for explicit retry. |
| Device disappears or storage fails | Stop capture with failure, cancel worker, preserve available data, never claim complete capture or insert. |
| Empty or extremely short audio | Return an explicit no-text outcome, preserve capture, skip clipboard overwrite/paste; do not invent padding policy. |
| Worker emits a result but never exits | Trigger the exit deadline, terminate/reap, record cleanup failure, skip automatic insertion. |
| Desktop exits or crashes | Orderly shutdown reaps; parent-loss mechanism terminates the worker; recovery never inserts automatically. |

## Efficiency and evidence still needed

Overlap can hide only preparation completed before release. A short dictation may finish before loading does. Process separation
protects the desktop event loop and ownership boundaries; it does not reserve CPU, memory bandwidth, disk, or GPU time for Capture.
Keep file writes, resampling, protocol IO, and native calls out of audio callbacks. PipeWire realtime stream callbacks require
RT-safe functions. Set an explicit CPU thread count and one inference job; tune only after measuring capture continuity and latency
on the RX 7700S. [PipeWire stream contract](https://docs.pipewire.org/group__pw__stream.html).

A `spawn_blocking` task does not provide process cancellation. A started blocking task cannot be aborted, and
runtime shutdown can wait indefinitely for it. The worker may run native synchronous code, while the desktop supervises asynchronous
pipes and exit. [Tokio blocking tasks](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html). Tauri supports packaged
sidecar binaries with target-triple-specific names; decide whether to bundle a worker or use an early worker mode in the app binary
without coupling Recognition to frontend shell APIs. [Tauri sidecars](https://v2.tauri.app/develop/sidecar/).

Exit removes intentionally resident worker/model resources, but does not promise zero desktop RAM/VRAM or power use. OS model-file
cache can remain reclaimable; Vulkan memory freeing requires completed commands that refer to that memory. Measure process
disappearance and GPU-memory return separately. The existing RX 7700S load range and model-buffer diagnostics are observations, not
speed, total memory, or cancellation guarantees. [Linux memory reporting](https://docs.kernel.org/filesystems/proc.html#meminfo),
[Vulkan memory release](https://docs.vulkan.org/refpages/latest/refpages/source/vkFreeMemory.html), [Existing
observations](model-lifetime.md).

Before implementation readiness, settle protocol limits, deadlines, attempt creation/checkpoints, parent-loss mechanism, and
packaged worker ownership. Do not treat these recommendations as accepted product or architecture changes. Later acceptance evidence
needs injected spawn/EOF/protocol/exit failures, cancel-result-insertion races, release during loading, rapid rejected holds,
shutdown and parent death, empty input, disk/device failure, and worker reaping. Real Whisper/Vulkan checks must include capture
continuity while loading, resolved RX 7700S identity, cold/warm short and five-minute jobs, post-exit RAM/VRAM and idle CPU/power,
and release-to-insertion in actual T3. Issue #4 owns the reviewed contracts; the pilot owns quality and latency evidence. No model,
benchmark, private recording/transcript/log, installation, setting change, or application verification was executed for this report.
