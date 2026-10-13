# Whisper model lifetime

Research date: 2026-10-13. Recommendation: evaluate one recognition worker per
dictation, load Whisper large-v3 when hold begins, and exit after the job.
This follows the owner's resource-first preference without requiring a model to
remain resident between dictations. It is a proposal, not an accepted lifetime
policy or a measured implementation.

Rust/Tauri desktop, Whisper large-v3 on the RX 7700S, SQLite with separate lossless
audio, and initial clipboard paste are accepted directions. Model lifetime remains
open. This research did not run models, change settings, or read recordings or
transcripts. It inspected existing attempt settings/timings and whitelisted native
diagnostic lines, plus the pinned runtime source.

## Existing observations

The preserved pilot evidence contains these completed baseline attempts with
beam size 5, gain 0, and no vocabulary prompt. Older metadata without a settings
object was classified through recorded command flags and native device diagnostics.
All four attempts identify the inspected runtime revision. These are small samples
across recordings, not repeated measurements of one controlled workload.

| Device | Attempts | Native load, seconds | Native encode, seconds | Native decode, seconds | Native total, seconds | Startup-inclusive elapsed, seconds |
| --- | --- | --- | --- | --- | --- | --- |
| RX 7700S | 3 | 2.362 to 2.893 | 0.341 to 0.713 | 0.000 | 5.268 to 6.510 | 5.799 to 7.103 |
| Radeon 780M | 1 | 7.608 | 3.112 | 0.000 | 26.215 | 27.907 |

Both devices reported a Vulkan model buffer of 3094.36 MB. That is an allocation
diagnostic, not measured total RAM, VRAM, or peak usage. State, compute buffers,
driver resources, and the application add costs. Integrated-GPU memory also must
not be counted as independent dedicated VRAM.

The wrapper's elapsed time includes audio conversion and launching a new CLI
process. Native `load time` covers the model-loading function, excluding later
state initialization. Native `total time` starts at model loading and ends at
timing output. It includes initialization and inference work beyond `load time`;
it is not inference alone. The zero `decode` line does not mean decoding is free:
the runtime also accounts for batched decoding, prompt processing, and sampling
separately. See the pinned
[loader and timing implementation](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/src/whisper.cpp).

No resident-versus-reload comparison or controlled file-cache-cold comparison
exists. The RX 7700S load range suggests a measurable reload cost, but does not
establish the time a resident worker would save. None of these measurements
includes real T3 insertion or establishes the pilot's release-to-insertion gate.

## What actually remains loaded

The pinned runtime reads model weights into backend buffers and keeps them in
the context. Finishing recognition does not automatically unload that context.
`whisper_free` releases model buffers and state, including compute schedulers and
backends. The [CLI](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/examples/cli/cli.cpp)
calls it before exiting. The
[server](https://github.com/ggml-org/whisper.cpp/blob/0c7be696371069e0baa2515ed7aa08d5a5473aa6/examples/server/server.cpp)
retains its context between requests; `/load` frees the old context and loads a
replacement. It is not an unload endpoint.

Freeing a context inside a persistent process may leave allocator or backend
resources resident. Worker exit provides a clearer ownership boundary. Neither
approach promises zero desktop RAM/VRAM usage: the shell remains running, and
OS/driver caches have separate lifetimes. Linux's
[file cache](https://docs.kernel.org/filesystems/proc.html#meminfo)
can retain reclaimable model-file pages after unloading. Cached bytes are not a
live model allocation. A later reload can still allocate and populate GPU buffers.
[Vulkan memory release](https://docs.vulkan.org/refpages/latest/refpages/source/vkFreeMemory.html)
also requires completed GPU work and release of bound-resource references.

## Policy comparison

| Policy | Between dictations | Tradeoff |
| --- | --- | --- |
| Load on hold, unload after every job | No intentionally resident recognition worker | Repeated initialization and transfers; proposed recording/load overlap |
| Bounded idle grace | Keep the model until an idle deadline | Reuse during bursts, temporary resource occupancy, additional deadline/cancellation behavior |
| Always resident | Keep the model while Dipper runs | Avoid repeated loading, continuous model resource occupancy |

Always resident conflicts with the owner's stated preference. A bounded grace
period may later be configurable, but the evidence supports no fixed duration.

## Proposed initial behavior and unexecuted checks

Start model preparation in a separate worker when hold begins while Capture
continues independently. Recognition receives complete capture audio after
release. Report recording readiness from Capture, not model readiness. Return the
structured recognition outcome to the parent, then tear down the worker.
Existing CLI measurements do not prove this preload overlap; a worker request
interface would need a reviewed contract rather than reading an unfinished audio
file.

Owner acceptance and app evidence are still needed for cancellation during load
or inference, worker failure, rapid repeated holds, shutdown, and resource release.
Verify that cancellation prevents insertion, capture remains complete during
loading, and no worker survives its completed or canceled job. Later app checks
should measure post-exit RAM/VRAM and cold/warm release-to-insertion, including
short recordings where loading may outlast capture. No additional prototype
iterations or experiments were executed or authorized here.
