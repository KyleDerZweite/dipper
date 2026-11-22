# Hex comparison

Observed 2026-10-30. This is source research and a read-only host compatibility
check, not a tested Hex trial. No Hex binary, installer, model, or build was run.
The inspected upstream `dev` revision was
`d605d15158e1a183306f3c42b44f6a405051e120`. Links below use canonical upstream
paths; those paths can change after this observation.

Hex is an MIT-licensed Rust dictation app with local recognition and a Linux
beta. Borrow behavior and evaluation cases, while preserving Dipper's accepted
independent Rust/Tauri app, Whisper large-v3-turbo with Auto recognition,
per-job worker lifetime, Journal, and original-window insertion contract.
Realtime recognition, replacement-library implementation, and UI polishing remain
deferred. This report adds no implementation scope. See [product direction](product.md),
[ADR-0002](adr/0002-rust-core-tauri-desktop.md), and
[cross-app insertion](cross-app-insertion.md).

## Reusable ideas and limits

| Area | Observed Hex behavior | Dipper adaptation to consider later |
| --- | --- | --- |
| Text replacements | Literal, case-insensitive Unicode phrase matching. Longer overlapping matches win, then earlier position and rule order. Replacement output is not recursively matched within one pass. Empty output deletes a phrase and adjusts punctuation, spacing, and capitalization. | An explicit library could handle `Merchen` to `mergen` and abbreviation expansion without an LLM. Review case, punctuation, overlap, deletion, and German compound-word rules before selecting semantics. Preserve the raw transcript and rule identity. |
| Corrections and vocabulary | Mode-specific replacements follow global replacements. Vocabulary can guide a processing prompt. Hex calls automatic replacements corrections in places. | Keep Dipper's distinctions between Text replacement, recognition vocabulary, spoken self-correction, and an explicit user Correction. Automatic output is never Confirmed wording. |
| Processing failure | Replacements precede optional OpenCode rewriting, followed by custom transformations. Rewriting errors or empty output preserve replacement-processed text and record latency and a fallback reason. A failed transformation chain preserves its input as one stage. | Keep the last usable stage result and separate stage outcomes in the Journal. An optional cleanup failure must not destroy usable recognition output. Cancellation and insertion eligibility still govern delivery. |
| Desktop permissions | GlobalShortcuts portal reports granted bindings and session closure; RemoteDesktop portal can inject paste on KDE. No input-group access or privileged injection fallback. | Show granted bindings and actionable permission failures. Review session loss and release events independently of any future portal adoption. Preserve Dipper's KWin original-window guard. |
| UI | Settings exposes listener state/errors, granted shortcuts, model preparation, and microphone behavior. Settings, Modes, and History share a window; the click-through recording/processing indicator avoids focus. Closing the window leaves dictation running. | Reuse state visibility and recovery actions in Tauri. Hex's Rune UI is not a reason to replace Tauri. History does not establish Dipper's retained audio, distinct attempts, Corrections, or crash-recovery contract. |

Sources: [replacements](https://github.com/anomalyco/hex/blob/dev/src/text_replacements.rs),
[mode ordering and History draft](https://github.com/anomalyco/hex/blob/dev/src/dictation_modes.rs),
[processing and fallback](https://github.com/anomalyco/hex/blob/dev/src/dictation_processor.rs),
[vocabulary](https://github.com/anomalyco/hex/blob/dev/src/vocabulary.rs),
[Linux operation and UI](https://github.com/anomalyco/hex/blob/dev/docs/linux.md),
[license](https://github.com/anomalyco/hex/blob/dev/LICENSE).
Modes are experiment-gated in the inspected runtime; with Modes off, only global
replacements run. Do not assume every advertised processing control is enabled.

A consequential failure-path limit appears in Linux `DictationOutput::deliver`:
it rejects empty processed output, then attempts paste before recording History.
A paste error returns before that History write. This is not a sufficient model
for Dipper's durable Journal fallback. Hex's clipboard fallback is useful, but
cannot replace retained results. Source:
[Linux delivery](https://github.com/anomalyco/hex/blob/dev/src/linux_dictation.rs).

Hex snapshots processing settings before potentially slow inference so a settings
write need not wait for cleanup. Its default processing deadline is 30 seconds,
which alone could consume Dipper's entire accepted end-to-end ceiling. Copy the
bounded-stage principle, not that budget. Hex's listener retains a transcriber
and can prepare a replacement model on a worker thread. This trades faster reuse
for resident memory and possible temporary model overlap. Dipper's accepted
load-on-hold, exit-after-job policy remains unchanged. Hex can release the
microphone while idle, at the cost of opening it at the shortcut and losing
pre-roll. Vulkan inference can fall back to CPU, but the graphical window still
needs a working Vulkan driver. No RAM, VRAM, latency, power, or quality advantage
was measured here. Sources:
[processor](https://github.com/anomalyco/hex/blob/dev/src/dictation_processor.rs),
[model swapping](https://github.com/anomalyco/hex/blob/dev/src/local_transcriber.rs),
[listener](https://github.com/anomalyco/hex/blob/dev/src/linux_dictation.rs),
[Linux requirements](https://github.com/anomalyco/hex/blob/dev/docs/linux.md).

## Fedora KDE trial prerequisites

The current Linux guide describes KDE GlobalShortcuts and RemoteDesktop paths,
but its supported-target introduction still names X11 and compatible wlroots
compositors. The Wayland beta release note explicitly disclaims KDE/GNOME support
and is marked unreleased. Treat KDE as an exploratory compatibility trial, not
established upstream support. Sources:
[Linux guide](https://github.com/anomalyco/hex/blob/dev/docs/linux.md),
[beta release note](https://github.com/anomalyco/hex/blob/dev/docs/releases/linux-wayland-beta.md).

Read-only checks on this host found Fedora 43 KDE, Wayland, x86_64, glibc 2.42,
Rust 1.95.0, a Ryzen 7 7840HS, and about 30 GiB total RAM. Installed components
include xdg-desktop-portal 1.20.4, its KDE backend 6.7.5, PipeWire 1.4.11,
`wl-clipboard`, `wtype`, Vulkan loader, and Mesa Vulkan drivers. These package and
session checks do not prove functioning audio, GPU inference, portal permissions,
or paste. No permission dialogs were opened.

For a binary trial, use the x86_64 standalone executable from the
[official download](https://anoma.ly/hex/download/linux-x64), not the RPM or
installer. Upstream documents Ubuntu 24.04 builds requiring glibc 2.39 or later;
this host passes that version check. Exact downloaded ELF symbol requirements
and shared-library resolution remain unchecked. The package manifest requires
ALSA, Vulkan, xkbcommon and xkbcommon-X11, X11, XCB, Xcursor, Xi, Wayland client,
and curl. Runtime package checks found these libraries, including
`libwayland-client` 1.26.0 after resolving its library path with `rpm -qf`.
Mesa being installed does not prove Vulkan initialization. Sources:
[requirements](https://github.com/anomalyco/hex/blob/dev/docs/linux.md),
[package dependencies](https://github.com/anomalyco/hex/blob/dev/packaging/linux/nfpm.yaml).

A source build avoids inheriting the release build's glibc floor by linking on
the local system; it does not fix compositor compatibility. Upstream documents
Rust, C/C++ build tools, Git, Python, CMake, pkg-config, Clang, ALSA and OpenSSL
development files, OpenBLAS, xkbcommon/X11/XCB development files, Vulkan headers,
shaderc, and SPIR-V headers. Fedora prerequisites include `alsa-lib-devel`,
`openssl-devel`, `openblas-devel`, `libxkbcommon-devel`,
`libxkbcommon-x11-devel`, `libX11-devel`, `libxcb-devel`, `vulkan-headers`,
`shaderc-devel`, `spirv-headers`, and a working `glslc` shader compiler, plus the
tools above. This host lacks `alsa-lib-devel`, `libxkbcommon-x11-devel`,
`vulkan-headers`, `shaderc-devel`, and `spirv-headers`; `glslc` was absent from
PATH. This is a prerequisite list, not a verified complete Fedora build recipe.
Native transitive dependencies may impose further requirements. The Linux build
script uses `cargo build --locked --release`; ordinary Linux linkage also names
OpenBLAS. Sources:
[build guide](https://github.com/anomalyco/hex/blob/dev/docs/linux.md),
[manifest](https://github.com/anomalyco/hex/blob/dev/Cargo.toml),
[build script](https://github.com/anomalyco/hex/blob/dev/scripts/build-linux.sh),
[link configuration](https://github.com/anomalyco/hex/blob/dev/build.rs).

A later authorized non-installing source trial can use `cargo build --locked
--release` followed by `target/release/hex`, without running either installer.
Set `HEX_LOCAL_BUILD=1` during compilation to disable update offers. Downloading
or building was not performed for this research.

## Isolated data and runtime

Use a short, private, absolute `HEX_APPLICATION_SUPPORT_DIR` for every trial
command. It contains settings, logs, model data, History, the RemoteDesktop token,
and `service.sock`. It separates the running instance and suppresses use of the
installed systemd unit when `HEX_SYSTEMD_UNIT` is unset. An extracted binary can
run directly, or a source binary can run without installing launchers, autostart,
or a service through an installer. Use the same environment for `hex status`
and `hex quit`; closing the window alone does not stop Hex. Sources:
[Linux lifecycle](https://github.com/anomalyco/hex/blob/dev/docs/linux.md),
[data paths](https://github.com/anomalyco/hex/blob/dev/src/app_paths.rs),
[socket and unit selection](https://github.com/anomalyco/hex/blob/dev/src/linux_service.rs).

That variable is not full isolation. Configuration and experiments also use
`$HOME/.config/hex`, and a loose binary can create
`$XDG_DATA_HOME/applications/hex.desktop` for portal identification. For an
exploratory same-session run, use a private HOME, XDG_CONFIG_HOME,
XDG_DATA_HOME, and XDG_CACHE_HOME in addition to the support directory. Keep the
real session's XDG_RUNTIME_DIR, WAYLAND_DISPLAY, and D-Bus connection so the app
can reach the compositor and portals. Do not replace XDG_RUNTIME_DIR with an
empty directory. Hex's own runtime socket is isolated by its support directory.
A private XDG_DATA_HOME may prevent the desktop portal from discovering the new
entry. That remains a trial question, not a proven recipe. Portal grants are
stored by the desktop and are not isolated by Hex's data variable. A disposable
KDE user session is the stronger option for a full portal trial without changing
Kyle's desktop grants or launcher state. Sources:
[paths](https://github.com/anomalyco/hex/blob/dev/src/app_paths.rs),
[portal entry creation](https://github.com/anomalyco/hex/blob/dev/src/linux_portal.rs).

KDE needs a working user session bus, GlobalShortcuts and RemoteDesktop portal
backends, a microphone reachable through ALSA/PipeWire compatibility, and
`wl-copy`/`wl-paste`. Installed `wtype` alone does not establish KDE paste support.
First portal paste asks permission to control the keyboard and saves a restore
token. Shortcut requests can conflict or retain an older KDE assignment; inspect
the granted binding in Settings. Upstream says Wayland uses the Global mode
because it cannot identify the focused application. That does not replace
Dipper's separate KWin target contract. Source:
[Wayland requirements](https://github.com/anomalyco/hex/blob/dev/docs/linux.md).

## Models and practical inspection

Download a compatible Hex model before offline dictation. Explicitly select
Whisper large-v3-turbo and Auto for a relevant Dipper comparison; an English
system can otherwise default to Parakeet Unified English. Hex's pinned Turbo
artifact is `whisper-large-v3-turbo-Q8_0.gguf`, 886,381,824 bytes, from
`handy-computer/whisper-large-v3-turbo-gguf`. Dipper's existing Whisper model file
is not proven compatible with Hex's transcribe.cpp format. Allow additional disk
space and working RAM beyond the model file. Initial model setup needs Hugging
Face and its download CDN; ordinary local dictation does not need OpenCode.
Optional rewriting may use a provider and must be evaluated separately. Sources:
[model catalog and defaults](https://github.com/anomalyco/hex/blob/dev/src/transcription_models.rs),
[setup and privacy](https://github.com/anomalyco/hex/blob/dev/README.md).

Inspect these scenarios in a later authorized trial:

1. Dictate German with English identifiers and `Merchen`, numbers, negations, and
   a spoken self-correction. Compare raw and processed output. Exercise longer
   overlapping replacements, uppercase input, word boundaries, and deletion.
2. Make optional processing unavailable or return empty output. Inspect fallback
   text, reason, latency, and History. Deny paste separately and check what survives.
3. Hold, release modifiers in either order, cancel, deny portal permission, and
   close a portal session. Check shortcut grants, recording termination, and
   focus preservation with the Settings window open and closed.
4. Paste into T3, a browser, Obsidian, and a terminal with the required binding.
   Change focus before completion. Hex's dispatched shortcut is not confirmed
   insertion. Check clipboard restoration and manual clipboard fallback too.
5. Measure cold preparation, repeat dictation, idle memory, GPU/CPU inference,
   microphone release, and shutdown. Hex's warm resident model and different
   runtime require separate accounting from Dipper's per-job worker.

Upstream documents clipboard restoration after 500 ms where supported, with a
32 MiB/one-second snapshot bound and protection against overwriting a newer copy.
A slow target may receive the restored clipboard instead of the transcript.
These are useful failure cases to inspect, not a restoration policy selected for
Dipper. Source: [Linux paste behavior](https://github.com/anomalyco/hex/blob/dev/docs/linux.md).

Verification for this report was limited to source inspection, read-only host
commands, repository diff checks, and local documentation-link checks. No build,
model download, dictation, GUI interaction, portal grant, or performance test ran.
Root retains integration and final verification.
