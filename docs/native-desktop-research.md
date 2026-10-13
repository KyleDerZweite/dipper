# Native KDE Wayland integration for Dipper

Research date: 2026-10-13. This report evaluates the accepted Rust core and Tauri 2 desktop shell on Fedora KDE Wayland. It is research, not an accepted implementation contract. [App design](app-design.md), [ADR-0002](adr/0002-rust-core-tauri-desktop.md), and [ADR-0003](adr/0003-journal-storage.md) own the current direction.

The shortest credible path is a small Rust desktop adapter for portal shortcuts, a background tray, and explicit integration checks against the installed T3 build. KDE's source implements shortcut press and release signals. The unresolved problem is automatic insertion: the examined public APIs do not provide an atomic operation that validates the intended T3 field, changes the clipboard, and delivers Paste only while that field remains valid. Keep the accepted guard. Do not declare the first voice-to-T3 slice Ready on the strength of shortcut or clipboard support alone.

Native desktop interaction and native widgets are separate goals. Tauri can use KDE-owned permission dialogs, system notifications, and a tray while its application content remains HTML in WebKitGTK. Replacing Tauri with Qt would change ADR-0002 and would not, by itself, solve the target validation or focus race.

## Scope and evidence

The accepted requirements are global HOLD, background/tray operation, and initial Clipboard plus Paste into a valid intended T3 target. Dipper must not pull focus back, overwrite the clipboard when the target is invalid, or automatically retry an unclear insertion. Retained transcripts support explicit later use. Raw-chain delivery still needs real app acceptance; polished UI is deferred.

The worktree was clean at `a743e079caa86c2e8cb034c52409648675025a12`, branch `research/native-desktop`. Research used official specifications, library documentation, and upstream source. Source snapshots are linked by revision where behavior matters:

| Component | Inspected revision or documentation |
| --- | --- |
| KDE portal backend | `bfc7ac333b9ecf06c2f928e0fbb1adf24b219f30` |
| KWin | `db6e482628aacc4543bd6d6a78b455960cd90c5e` |
| KGlobalAccel daemon | `5b7f39b88d33877aeecaf9e73e80cbf755c14f12` |
| Plasma Wayland protocols | `1db07441870d54157e937674b99380a7dcc0fb32` |
| Rust portal wrapper | `ashpd 0.13.4` |
| Tauri clipboard plugin | `clipboard-manager-v2.4.2` |
| Public T3 source | `a01c48f86245f2a779f9af84042f3b09fbdeb91f` |

These upstream versions do not establish the versions installed on Kyle's Fedora machine. No application was installed or implemented, no shortcut or permission was registered, and no private application state, clipboard, audio, or model was read. Every runtime scenario below remains unexecuted.

## Global HOLD and permission UX

The [GlobalShortcuts specification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html) defines sessions, binding, binding changes, and `Activated`/`Deactivated` events independently of application focus. Version 2 adds `ConfigureShortcuts`. Activation tokens authorize a possible window activation; Dipper must not use them to focus its UI during dictation. Timestamps have millisecond granularity and an undefined base, so they are not wall-clock journal timestamps.

KDE supplies more than an abstract capability. Its [shortcut backend](https://github.com/KDE/xdg-desktop-portal-kde/blob/bfc7ac333b9ecf06c2f928e0fbb1adf24b219f30/src/globalshortcuts.cpp) maps KGlobalAccel's `globalShortcutPressed` and `globalShortcutReleased` to portal activation and deactivation. New bindings show a KDE configuration dialog with conflicts. Existing shortcut IDs reuse stored bindings. Rejected new shortcuts are recorded with empty key combinations to avoid prompting on every restart. Configuration opens the relevant System Settings shortcut page. Consequently, a successful request alone is insufficient evidence that HOLD has an assigned usable binding.

Release behavior needs care. In the inspected [KGlobalAccel registry](https://github.com/KDE/kglobalacceld/blob/5b7f39b88d33877aeecaf9e73e80cbf755c14f12/src/globalshortcutsregistry.cpp), any subsequent key-release event releases `m_lastShortcut`; activating another shortcut also releases the previous one. Modifier-only shortcuts become active during modifier release. This is not an independent stream of raw physical key states. A modifier-only binding is therefore a poor HOLD candidate. Chords require tests for both release orders and unrelated keys.

KWin's [input filter](https://github.com/KDE/kwin/blob/db6e482628aacc4543bd6d6a78b455960cd90c5e/src/input.cpp) forwards both presses and repeats to shortcut handling and checks shortcut inhibition on press and release. Dipper should suppress duplicate activation for one recording and handle session loss or missing release as a failure/cancellation condition. A timeout can bound a stuck recording, but cannot prove the physical release occurred. Choose that bound in the reviewed dictation contract.

The stock Tauri plugin's underlying [`global-hotkey` library](https://github.com/tauri-apps/global-hotkey/blob/35b81e58ffb6b7c333851ee4b75121616ea0ab3b/README.md) documents Linux X11 only. Do not use XWayland shortcut support as acceptance evidence for a native Wayland HOLD. A portal adapter is the smaller appropriate change within the accepted foundation.

[`ashpd 0.13.4`](https://docs.rs/ashpd/0.13.4/ashpd/desktop/global_shortcuts/struct.GlobalShortcuts.html) wraps session creation, binding/listing, configuration, and activation/deactivation streams. Its [feature metadata](https://docs.rs/crate/ashpd/0.13.4/features) allows selected portal features and a Tokio runtime without enabling the optional GTK4 integration. Tauri's Linux GTK integration does not require adding GTK4 to use these D-Bus APIs. Prefer ashpd for the standard portals; direct zbus proxies are reasonable for KDE-specific APIs or an unsupported portal method. Direct proxies also make Dipper responsible for request response races, signal subscriptions, session closure, and version handling that a wrapper already covers.

Recommendation: establish stable Dipper desktop identity and shortcut IDs, show the effective binding, and offer an explicit configuration action. Prepare permission-dependent integrations in setup, before dictation. A dialog appearing while transcription finishes can invalidate the original target. Handle denial and revocation visibly without repeated automatic prompts. Portal parenting requires an exported Wayland handle; a pointer or numeric native window ID is insufficient. An empty parent is permitted if no suitable handle exists, according to [window identifiers](https://flatpak.github.io/xdg-desktop-portal/docs/window-identifiers.html).

## What can identify the intended target

Ordinary Wayland clients receive keyboard focus for their own surfaces. The [core protocol](https://wayland.freedesktop.org/docs/html/apa.html) defines keyboard enter/leave events and a data-device selection request tied to an input serial. It does not give an arbitrary background client an API to enumerate every other application's focused field. A global shortcut activation therefore does not identify T3 or its composer.

There are three distinct candidates for further validation:

| Candidate | Useful evidence | Limits and maintenance |
| --- | --- | --- |
| AT-SPI accessibility | Application/object identity, focus events, role, state, and editable-text interfaces | Depends on the actual T3 accessibility tree and state freshness; no atomic focus-conditioned clipboard transaction |
| KWin scripting | `activeWindow`, window identity, process/desktop-file information, activation changes | KDE-specific installed script and IPC; cannot see T3's internal editable field |
| Plasma window-management protocol or KWin D-Bus | Window UUID, app ID, process ID, active state or queried metadata | KDE-specific support and packaging constraints; still only window evidence |

AT-SPI is the best first candidate for field-level evidence. Its [Accessible API](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/class.Accessible.html) exposes application ancestry, process identity, interfaces, and object state. The [state definitions](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/enum.StateType.html) distinguish focused, editable, enabled, sensitive, defunct, visible/showing, and read-only objects. A recommendation is to identify the intended T3 window and composer object at HOLD start, then require matching current identity, editable focus, and no observed invalidation before insertion. Ambiguous, missing, stale, or defunct evidence must fail closed. Exclude password and unrelated fields. Never call an accessibility focus-grab operation to repair a failed check.

This is a proposed validation strategy, not proof that T3 exposes sufficient evidence. [T3's public README](https://github.com/pingdotgg/t3code/tree/a01c48f86245f2a779f9af84042f3b09fbdeb91f) identifies an Electron desktop app. Electron [documents accessibility-tree support](https://www.electronjs.org/docs/latest/tutorial/accessibility), but that does not prove this installed build's Linux composer identity. T3's [editor contract](https://github.com/pingdotgg/t3code/blob/a01c48f86245f2a779f9af84042f3b09fbdeb91f/docs/internals/composer-editors.md) uses Tiptap, remounts editors on mode changes, and applies Markdown serialization and paste transformations. A generic editable role or window title cannot distinguish its prompt from search, a terminal, another composer, or a modal. Object remounting also makes cached field references unsafe. Test the actual build using synthetic content and record whether accessibility is available without changing system settings.

The [KWin scripting API](https://develop.kde.org/docs/plasma/kwin/api/) exposes `workspace.activeWindow`, `windowActivated`, and window metadata such as `internalId`, `pid`, and `desktopFileName`. A read-only observer script could publish target window changes over an authenticated local channel. Its packaging, installation, restart behavior, version support, and removal become Dipper responsibilities. This may complement AT-SPI when window identity is insufficient there. It still cannot determine which DOM editor accepts Paste.

KWin's [D-Bus implementation](https://github.com/KDE/kwin/blob/db6e482628aacc4543bd6d6a78b455960cd90c5e/src/dbusinterface.cpp) makes `queryWindowInfo()` interactive through window selection; it is unsuitable for silent per-dictation focus checks. `getWindowInfo(uuid)` queries a known window and returns its metadata, not the active input field. Treat KDE implementation APIs as a tested desktop adapter, not a portable core dependency. Avoid using private fake-input APIs as a substitute for the portal permission flow.

The [Plasma protocol](https://github.com/KDE/plasma-wayland-protocols/blob/1db07441870d54157e937674b99380a7dcc0fb32/src/protocols/plasma-window-management.xml) advertises window IDs, active state, app IDs, and PIDs. Its prose says only one client can bind. The inspected [KWin implementation](https://github.com/KDE/kwin/blob/db6e482628aacc4543bd6d6a78b455960cd90c5e/src/wayland/plasmawindowmanagement.cpp) uses resource maps and sends existing windows on bind. Do not resolve this source/spec discrepancy by assuming compatibility. The [KWin global filter](https://github.com/KDE/kwin/blob/db6e482628aacc4543bd6d6a78b455960cd90c5e/src/wayland_server.cpp) denies the window-management and data-control interfaces to sandboxed clients. Native unsandboxed packaging may permit them, but this needs installed-version evidence and does not validate a field.

## Clipboard, Paste, and honest insertion outcomes

The [RemoteDesktop portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html) can request keyboard control without requesting pointer, touchscreen, or ScreenCast. `Start` may show a permission dialog. Version 2 supports restore tokens and persistence; restoration can fail and prompt again. `ConnectToEIS` is the recommended input transport. Once EIS is connected, the same session must not use the D-Bus `Notify*` input methods.

[`ashpd::RemoteDesktop`](https://docs.rs/ashpd/0.13.4/ashpd/desktop/remote_desktop/struct.RemoteDesktop.html) exposes both EIS connection and D-Bus keyboard methods. EIS additionally needs a sender implementation, device negotiation, event dispatch, and balanced key state. [libei's documentation](https://libinput.pages.freedesktop.org/libei/api/index.html) makes the compositor responsible for processing emulated input. An accepted keyboard event is therefore transport evidence, not proof that T3 consumed a paste. D-Bus `NotifyKeyboardKeysym` is a smaller initial transport candidate where supported; EIS is a separate alternative, not an extra simultaneous fallback. Verify modifier handling against the current layout and physically held keys.

KDE's [RemoteDesktop implementation](https://github.com/KDE/xdg-desktop-portal-kde/blob/bfc7ac333b9ecf06c2f928e0fbb1adf24b219f30/src/remotedesktop.cpp) supports restoration, forwards EIS connections to KWin, and owns a Remote Control status indicator. Restored sessions can emit a Remote Control Started notification. Its KDE-specific broad authorization mechanism is not a recommended Dipper setup step. Use the normal permission flow. A long-lived keyboard session avoids a permission dialog per dictation but leaves desktop control authorized while Dipper runs. The owner must choose session lifetime and restoration behavior.

The [Clipboard portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Clipboard.html) extends an authorized session rather than supplying an unrestricted clipboard API. For a RemoteDesktop session, request clipboard access before `Start` and inspect `clipboard_enabled`. `SetSelection` publishes MIME types; transfer requests require writing data through the supplied file descriptor and reporting completion. Do not advertise a pasteable selection before its data supplier is ready. [`ashpd::Clipboard`](https://docs.rs/ashpd/0.13.4/ashpd/desktop/clipboard/struct.Clipboard.html) wraps these methods and signals.

KDE's [Clipboard implementation](https://github.com/KDE/xdg-desktop-portal-kde/blob/bfc7ac333b9ecf06c2f928e0fbb1adf24b219f30/src/clipboard.cpp) uses Wayland data control and enables clipboard access only for supported sessions. It unsets its selection when the owning session closes. Keeping the transcript in the clipboard after insertion therefore needs a live data owner or verified clipboard-manager persistence. Dipper must not promise persistence after process/session exit solely because `SetSelection` succeeded.

A native unsandboxed path may be smaller. The released [Tauri clipboard plugin manifest](https://github.com/tauri-apps/plugins-workspace/blob/clipboard-manager-v2.4.2/plugins/clipboard-manager/Cargo.toml) enables arboard's `wayland-data-control` support. [arboard documents the feature](https://docs.rs/arboard/latest/arboard/) as depending on compositor data-control support. This is stronger evidence than assuming a foreground GTK clipboard write works from a hidden tray app. It is not a portable sandbox guarantee. Select one clipboard provider after packaging and actual KDE checks, including evidence that it uses native Wayland rather than an X11 fallback. The plugin's Linux support table alone is insufficient. Keep automatic writes inside Insertion rather than exposing an unconditional frontend copy command for recognition completion.

The focus race remains with either provider. Consider this sequence:

1. Dipper queries a valid T3 composer.
2. The user switches away or opens a modal before the query result reaches Dipper.
3. Dipper updates the global clipboard based on the stale result.
4. Paste reaches the field focused when KWin processes the injected keys, or a later field when the application handles them.

Extra rechecks and focus-event invalidation reduce the interval but do not make these operations atomic. Clipboard ownership, input delivery, and T3's editor run in different components. The examined methods contain no expected-window or expected-field condition. Loss of focus between the clipboard write and Paste cannot be repaired by pulling focus back, restoring old clipboard content, or automatically retrying; those conflict with the accepted behavior.

AT-SPI's object-directed insert/paste operations can avoid redirecting a key event to another app, but are still separate from the focus check and may alter an old object after focus moved. They are also a different insertion mechanism whose T3 support and editor semantics need acceptance. Native Qt widgets would not change this conclusion.

An exact guarantee requires cooperation at the component owning the contested state. A possible T3 endpoint could check current composer identity, editability, focus, and a generation token in the same operation that accepts text, then acknowledge the insertion ID. That is a new integration proposal requiring an owner decision and T3 support. Keeping the strict clipboard guard as well would require coordinated compositor/clipboard behavior; a T3 acknowledgement alone does not make an earlier global clipboard write conditional. No such complete integration was verified here. Do not silently replace the accepted guard with best-effort checks or replace automatic delivery with manual-only copying and call the first slice complete.

## Background UX, visual fit, and resource cost

Tauri's [tray guide](https://v2.tauri.app/learn/system-tray/) supports native tray menus, but documents unsupported direct tray mouse events on Linux. Menu actions are the reliable candidate for opening the journal/settings, explicit copy, cancellation, and quit. Closing a secondary window must preserve the authorized background dictation process. Make quit explicit and test it separately from close/hide.

Prefer tray icon/tooltip updates for recording and processing, with actionable system notifications for a retained result or failure. KDE's [status guidance](https://develop.kde.org/hig/status_changes/) recommends sparing notifications, forbids application use of Plasma OSDs, and generally discourages permanent tray icons. Dipper's accepted background/tray direction is a deliberate product exception. This guidance does not authorize removing the tray requirement. Notifications can be suppressed by desktop settings, so they cannot be the sole evidence that recording is ready.

A status window requires explicit no-focus behavior. The [Tauri window builder](https://docs.rs/tauri/2.11.6/tauri/webview/struct.WebviewWindowBuilder.html) exposes initial focus, focusability, and taskbar visibility; window creation focuses by default. These are configuration capabilities, not proof of KDE compositor behavior. Start with a tray and verify whether its status feedback is sufficient. If an overlay is later selected, test focus on create/show/update and all virtual desktops before claiming that it cannot steal focus.

Tauri's [WebView documentation](https://v2.tauri.app/reference/webview-versions/) identifies WebKitGTK on Linux. HTML controls do not become Qt/Kirigami widgets because Fedora runs Plasma. Use portal-owned dialogs, platform keyboard conventions, accessible HTML, and theme-aware CSS. KDE's [icon guidance](https://develop.kde.org/hig/icons/) favors theme-aware standard icons and Breeze compatibility. The [Settings portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html) supplies appearance preferences, but does not provide a complete Qt style engine for HTML. Exact Qt visual matching would need explicit design work; it is not a reason to reopen the accepted shell during this first slice.

Resource costs are architectural observations, not measurements. Portal signals and focused accessibility events can avoid polling and repeated subprocess startup. AT-SPI observers, EIS dispatch, clipboard ownership, and any KWin script add persistent work. The [Tauri process model](https://v2.tauri.app/concept/process-model/) includes the core and WebView processes; hiding a WebView does not prove it releases memory. A tray-first shell that creates UI on demand is a candidate for measurement, not an established optimization. No Dipper RAM, CPU, power, or integration-latency numbers were measured. Keep model startup and recognition-worker measurements separate from desktop adapter overhead.

## Recommended implementation path and owner decisions

The smallest production path consistent with accepted scope is to keep the Rust/Tauri foundation, use ashpd for native HOLD, and implement desktop adapters behind the proposed Dictation and Insertion responsibilities. Use only the required portal features. Begin with tray menus and state feedback. Test AT-SPI against the installed T3 composer before selecting a field-validation strategy; add a KWin observer only if window evidence requires it. Avoid a daemon, a private input bypass, screen capture, continuous UI polling, or a new Qt shell without an accepted need.

Automatic Clipboard plus Paste is not Ready from this research. A capability-only implementation can demonstrate clipboard ownership and keyboard delivery, but cannot claim the exact target guard. Preserve that limit in the first-slice contract and acceptance evidence. Implementing unrelated Capture, Recognition, and Journal behavior can continue when their own contracts are coherent.

The owner decisions needed before the insertion contract is Ready are:

- Specify the intended T3 identity. Decide whether validity requires the original window and composer, the same thread/draft, and uninterrupted focus through processing. Returning to T3 after a focus loss must not silently re-enable an old automatic attempt.
- Keep the exact accepted guard and choose an integration capable of enforcing it, or explicitly decide whether to change the guarantee after reviewing the race. This report recommends no implicit relaxation.
- Decide whether a cooperative T3 integration is worth adding and whether it may replace Clipboard plus Paste. That changes the mechanism and maintenance responsibility, not the Rust core choice.
- Choose packaging, session lifetime, and permission restoration together. Sandboxed and native packages have different data-control and KWin access limits. Define permission revocation and session-loss behavior.
- Choose the supported HOLD chord and lost-release policy from actual KDE evidence. A toggle is not an equivalent replacement.

## Acceptance evidence still required

These are proposed app-integration tests, all unexecuted. Use synthetic text, record installed Fedora/Plasma/KWin/portal/Tauri/T3 versions and packaging, and tie the results to the tested Dipper revision. Dipper owns adapter/lifecycle traces and honest Journal outcomes; T3 owns evidence that its intended composer accepted text. The owner approves actual workflow usefulness. Do not use a mock target as final T3 acceptance.

| Scenario | Required evidence |
| --- | --- |
| Native HOLD with Dipper hidden | One recording begins on activation and ends on physical release in T3 and another application; Dipper never becomes focused |
| Chord details | Repeat, key-first/modifier-first release, unrelated key release, another shortcut, keyboard layout, and shortcut inhibition produce the documented behavior without duplicate recording |
| Permission lifecycle | First bind, conflict, denial, empty binding, reconfiguration, restart, service loss, session revocation, and failed restoration remain visible and do not prompt inside insertion |
| Intended field | Actual T3 prompt succeeds; search, terminal, read-only text, password, modal, other T3 window/thread, editor remount, missing accessibility, stale cache, and target exit block insertion and clipboard writes |
| Focus loss before completion | Switching away at HOLD, release, model completion, and insertion preparation preserves the transcript without focus activation or clipboard overwrite; returning does not cause a stale automatic insertion |
| Race at every boundary | Inject controlled focus changes after validation, during clipboard ownership, before Paste, and while T3 handles Paste; verify no disallowed write or wrong-field insertion. A race failure blocks a strict-guard claim |
| Clipboard ownership | The selected provider writes from a hidden background app, serves the intended MIME/text, survives the required post-insertion lifetime, and preserves existing clipboard content on every blocked path |
| Real Paste and outcome | Unicode, German technical text, newlines, existing selection, rich/plain composer mode, and physical modifier state work; no accidental Enter/send; transport-only success remains unverified |
| Unclear outcome | Dropped acknowledgement, target hang/exit, clipboard ownership change, and partial input delivery never cause an automatic duplicate retry; synthetic keys are released |
| Background UI | Tray menus work on Plasma; window close hides and explicit quit exits; recording/processing/failure indicators remain understandable under dark/light themes, scaling, multiple displays, and notification suppression |
| Resource behavior | Compare release builds with no UI, settings/journal open, and hidden UI; record idle CPU/RSS, process count, wakeups, and adapter latency independently of recognition/model costs |

A test passing a sampled focus-race schedule does not prove an atomic guarantee. The implementation contract must identify who enforces validity at each mutation, or retain the unresolved limitation. No live HOLD, real T3 insertion, rendered UI, permission UX, or resource acceptance has been established by this report.
