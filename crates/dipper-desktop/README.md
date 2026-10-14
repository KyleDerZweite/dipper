# KDE desktop adapter

This package implements the local Fedora KDE Wayland pilot behind the core
`Insertion` interface. It does not depend on Tauri, Capture, Recognition, or
Journal. `DesktopAdapter::new` starts only its KWin observer. The application
must install its own `io.github.KyleDerZweite.dipper.desktop` identity before
calling `restore_hold` or the explicit `setup_hold` settings action.

`restore_hold` registers a fresh own portal connection, creates and subscribes
to its session, and lists available actions without binding or showing a
permission dialog. It returns `None` when no effective Hold exists. Portal
backends that expose no persisted actions in a fresh session need explicit
setup. `setup_hold` may display KDE's permission dialog. Its preferred key does
not override KDE's persisted assignment. GlobalShortcuts v1 uses the KDE
shortcut settings launcher for changes.

Use `subscribe` to forward `HoldPressed`, `HoldReleased`, and `SessionLost` to
Dictation. A lagged broadcast receiver must cancel the active dictation because
lost input events cannot establish a release. Press and release share one
ordered portal signal stream. Portal Deactivated is the recording end signal;
strict physical-key release detection is outside this pilot. Manual Cancel
remains necessary if a release disappears without session failure.

The target permit requires the original KWin window UUID, PID, and T3 app
identity, plus one current focused and editable Tiptap/ProseMirror entry on its
accessibility process. KWin reports authenticate their unique sender. Observed
focus departure permanently revokes the shared permit, including if T3 returns.
The adapter never activates a window or reads drafts. A small script-health
check runs once per second while a permit remains valid; dispatch also checks
script availability. Accessibility checks have a four-second total deadline
and a 650-object bound. The compositor, clipboard, and target still perform
separate operations, with the accepted residual focus race.

Build the `dipper-clipboard` executable alongside the app and pass its absolute
path to `DesktopAdapter::new`. It uses wl-clipboard-rs 0.9's native data-control
support, including ext-data-control. A bounded JSON request prepares the
supplier, followed by a separate guarded publish command. Publication is
acknowledged only after reading its private ownership MIME token back through
the compositor. stdin EOF terminates the helper even during a blocked native
call. The parent retains and reaps its helper and classifies failed publication
acknowledgement conservatively as possible clipboard mutation. Clipboard
ownership survives dictation settlement until replacement or app shutdown;
persistence after app exit depends on the desktop clipboard manager.

Paste requires the existing `ydotool` executable and running daemon. The adapter
sends only Ctrl+V, releases introduced keys after failure, and never retries or
sends Enter. Dispatch success is unverified target delivery. This is a local
pilot prerequisite rather than a portable keyboard-injection backend. The
intended environment has one desktop input seat.

Call `stop` during explicit app shutdown. It revokes permits, settles dispatch,
closes only its own portal session, stops and reaps its helper, and unloads only
Dipper's observer script. Persisted KDE user assignments remain configured.
Synthetic tests exercise focus revocation, clipboard protocol bounds,
cancellation during preparation, publication failure stages, and owned helper
cleanup. They establish no live portal, KWin, AT-SPI, clipboard, or T3 evidence.
