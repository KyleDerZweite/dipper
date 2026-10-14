# React Native and Rust for Dipper

Research date: 2026-10-13. This report compares candidates for the independent
Dipper app. The owner subsequently accepted the Rust-core/Tauri-desktop recommendation in
[ADR-0002](adr/0002-rust-core-tauri-desktop.md). Mobile UI remains deferred.

Rust with Tauri 2 is the stronger candidate for Dipper's immediate Linux desktop
work. React Native with Expo has the clearer documented route to later mobile
JavaScript updates. Neither candidate satisfies both delivery goals without
further choices. This recommendation concerns platform support and maintenance;
there is no representative Dipper benchmark that establishes a resource or speed
winner.

## Decision context

The [product direction](product.md), [pilot contract](pilot.md), and
[app design proposal](app-design.md) establish Fedora KDE on Wayland first,
background/tray operation, global hold-to-record, capture, recognition, and T3
insertion. Mobile comes much later. The own-app decision is accepted in
[ADR-0001](adr/0001-own-dipper-app.md); the proposed module boundaries still need
an implementation contract.

The owner's subsequent direction on 2026-10-13 sets Whisper large-v3 as the
first live baseline and excludes Qwen from active candidates after observed
latency above 30 seconds. Historical Qwen experiment results remain evidence.
The toolchain comparison must not reopen that model choice or confuse raw-chain
delivery with final cleanup and quality acceptance.

The owner also wants both forms of updates long term: desktop whole-app updates
without manual downloads, and compatible mobile UI/JavaScript updates without
a new store binary. Native audio, recognition, and runtime changes still need
binary releases.

## What the candidates actually contain

React Native combines React application code, a JavaScript engine, native UI
rendering, and platform modules. Android and iOS are its main implementation
targets. Windows and macOS have partner-maintained implementations; Linux relies
on community work outside the main repository. The official
[platform list](https://reactnative.dev/docs/out-of-tree-platforms) does not
establish equal support across these targets.

Rust is a programming language, so it needs a UI choice. The concrete comparison
here is a Rust core with Tauri 2, whose UI uses HTML, CSS, and JavaScript in a
system WebView. React can be that UI framework. Tauri supports desktop and mobile
builds, with WebKitGTK on Linux, WKWebView on Apple platforms, and the system
WebView on Android. UI reuse therefore means web UI reuse, with differing browser
versions and native integration needs. See [Tauri's overview](https://v2.tauri.app/start/)
and [WebView documentation](https://v2.tauri.app/reference/webview-versions/).

| Consideration | React Native | Rust core with Tauri 2 |
| --- | --- | --- |
| Fedora desktop now | Requires a credible Linux renderer and native integrations | Documented Linux build, tray, and distribution path |
| Later mobile UI | Android/iOS native component system | WebView UI with mobile plugins |
| Mobile JS/assets OTA | Expo provides compatible updates and recovery controls | Official updater does not provide mobile OTA |
| Native recognition | Native module or worker integration | Native library or worker integration |
| Maintenance exposure | RN, renderer, modules, JS dependencies, platform builds | Rust dependencies, frontend, WebView differences, platform adapters |

### Linux support needs scrutiny

The Linux renderer linked by React Native's official platform page is
[`react-native-skia/react-native-skia`](https://github.com/react-native-skia/react-native-skia).
Its own README calls it a proof of concept, documents Ubuntu 18, and uses
Chromium's `depot_tools`, GN, and Ninja. That is insufficient evidence for a
maintained Fedora KDE Wayland production foundation.

This is distinct from the graphics library formerly maintained under Shopify,
now [`wcandillon/react-native-skia`](https://github.com/wcandillon/react-native-skia).
Its current platform table lists Android, iOS, Apple desktop targets, TV targets,
and Web, with no Linux desktop target. Adding this graphics package does not
create a Linux React Native application runtime. Windows/macOS support also
does not supply a Linux backend.

Tauri documents [Fedora prerequisites](https://v2.tauri.app/start/prerequisites/)
and [tray menus](https://v2.tauri.app/learn/system-tray/). Direct tray mouse events
are unsupported on Linux, so recording controls should not depend on those
events. Its global-shortcut plugin uses
[`global-hotkey`](https://github.com/tauri-apps/global-hotkey), which supports
Linux X11 only. Native Wayland hold-to-record needs a separate adapter. The
[GlobalShortcuts portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
defines activation/deactivation signals that are a candidate for hold semantics;
actual KDE bindings, release behavior, and T3 insertion remain unverified.

## Resource use, performance, and quality

Modern React Native should not be judged by the old serialized bridge alone.
Since [React Native 0.82](https://reactnative.dev/blog/2025/10/08/react-native-0.82),
the New Architecture is mandatory. JSI permits direct native references and
calls without the old bridge's serialization. This removes one source of
overhead; JavaScript execution, allocation, rendering, threading, and each module's
data handling still matter. React Native itself says architectural changes do
not guarantee an app improvement. See its
[architecture explanation](https://reactnative.dev/architecture/landing-page).

[Hermes](https://reactnative.dev/docs/hermes) is React Native's default engine.
Its documented startup and memory improvements compare Hermes with
JavaScriptCore, not React Native with Rust or Tauri. A Linux renderer must also
establish which engine and architecture versions it supports.

Rust's [ownership system](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html)
checks memory ownership at compile time without a tracing garbage collector for
ordinary Rust allocations. That helps implement explicit capture and model
lifetimes. It does not make native dependencies infallible or remove Tauri's
WebView processes. Tauri's small download size follows from using the system
WebView instead of bundling a browser engine; it does not prove low resident
memory or power use. See its [process model](https://v2.tauri.app/concept/process-model/).

For either UI, Whisper can execute in the same native
[`whisper.cpp`](https://github.com/ggml-org/whisper.cpp) runtime with the same
large-v3 weights and Vulkan configuration. Inference speed and recognition
quality depend on that configuration, audio preparation, hardware, and model
lifetime. Choosing Rust instead of JavaScript for controls does not alone improve
the raw transcript. Capture readiness, cancellation, scheduling, persistence,
and insertion can still change end-to-end latency and correctness.

No controlled equivalent Dipper apps were found or measured here. Do not assign
RAM numbers or general speed ratios. Compare release builds using identical
audio, runtime, weights, and retention settings. Measure idle/background
resources, recording readiness, cold/warm model startup, release-to-insertion
latency, cancellation, and UI responsiveness. Preserve the pilot's quality and
30-second acceptance rules separately from framework measurements.

## Updates have three different scopes

### Desktop application updates

Tauri's [updater](https://v2.tauri.app/plugin/updater/) downloads signed application
artifacts through a static manifest or server, installs them, and supports
relaunch. This can satisfy the desktop whole-app direction, including native
Rust or recognition changes. It is an application release, not a JavaScript-only
patch. The guide documents AppImage artifacts; released
[`updater-v2.13.3`](https://github.com/tauri-apps/plugins-workspace/releases/tag/updater-v2.13.3)
[source](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.13.3/plugins/updater/src/updater.rs)
also implements Deb and RPM installation. RPM updates can require privilege
prompts. Packaging therefore affects whether installation is unattended.

The official [support matrix](https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/updater/README.md)
excludes Android and iOS. Do not infer mobile OTA from Tauri's mobile build
support. Signing also does not establish automatic health-based rollback or
crash-safe journal migration. Those need application-level acceptance evidence.
React Native's mobile update tooling likewise does not establish a Linux
whole-app updater; an RN desktop foundation needs its own release mechanism.

### Mobile JavaScript and assets

Expo's [runtimeVersion](https://docs.expo.dev/eas-update/runtime-versions/)
matches an update to a compatible installed native binary. UI and JavaScript
changes using existing native APIs can use this route. Adding or changing native
capture, Rust code, whisper.cpp, permissions, or native dependencies requires a
new build. The fingerprint policy helps track native compatibility; it does not
replace release tests. The [updates SDK](https://docs.expo.dev/versions/latest/sdk/updates/)
does not document Linux desktop support.

Expo supports [rollbacks](https://docs.expo.dev/eas-update/rollbacks/) to previous
updates or the embedded bundle and
[end-to-end signing](https://docs.expo.dev/eas-update/code-signing/), currently
available through its Production or Enterprise plans. Its
[error recovery](https://docs.expo.dev/eas-update/error-recovery/) is limited;
rollback can be unsafe after incompatible persistent-state changes. Dipper's
journal compatibility must cover both upgrade and rollback.

Microsoft's hosted App Center CodePush service retired with the relevant App
Center services on March 31, 2025. Microsoft's
[retirement notice](https://learn.microsoft.com/en-us/appcenter/retirement)
points to standalone CodePush source. Self-hosting is a maintenance choice, not
the continuation of that managed service.

OTA tooling is not blanket store permission. Apple's
[App Review Guidelines, 2.5.2](https://developer.apple.com/app-store/review/guidelines/#software-requirements)
restrict downloaded executable code that introduces or changes app functionality.
Google Play's [Device and Network Abuse policy](https://support.google.com/googleplay/android-developer/answer/16559646?hl=en)
prohibits outside-Play self-updates and downloaded native executable code. It
provides an interpreter/VM exception with indirect Android API access, while
requiring runtime-loaded interpreted code to follow Play policies. Evaluate the
actual update's behavior against these rules before release.

### Recognition model data

Downloaded model weights are separate from application binaries and JS bundles.
Both candidates can support model-data delivery, with explicit engine/format
compatibility, integrity checks, disk-space handling, interrupted-download
recovery, and version provenance. Downloading weights does not update the native
recognition engine or prove store acceptance of a changed product behavior.

## Recommendation and remaining decisions

Prioritize the Rust core with Tauri desktop candidate for the present Fedora
workflow. Its documented Linux support is stronger than the RN Linux route
examined here. Keep capture, recognition, journal, and insertion behind the
proposed interfaces, without a speculative plugin system. This was the research
recommendation; the subsequent owner decision is recorded in ADR-0002.

React Native with Expo remains a serious mobile candidate because compatible
JS/assets OTA is an explicit long-term requirement. Tauri mobile alone leaves
that requirement unresolved. A later React Native UI can share a Rust core
through native bindings. React Native supports
[custom native modules](https://reactnative.dev/docs/turbo-native-modules-introduction),
so manual bindings are another route, but binding maintenance and two UI
integrations add cost. For example, the
[UniFFI React Native binding generator](https://jhugman.github.io/uniffi-bindgen-react-native/)
currently warns that it is early development. A hybrid is an option to evaluate
when mobile work becomes concrete, not a required initial dependency.

A second hybrid option could share parts of the UI earlier: React Native for Web
inside a Tauri desktop shell, with React Native/Expo for mobile.
[React Native for Web](https://necolas.github.io/react-native-web/docs/) renders
compatible components through React DOM. [Expo exports static web assets](https://docs.expo.dev/guides/publishing-websites/),
and [Tauri hosts static frontend builds](https://v2.tauri.app/start/frontend/).
This is an architectural inference from those capabilities, not a verified
Dipper integration. It avoids relying on the Linux native RN renderer, but still
requires distinct Tauri and mobile native adapters, UI compatibility work, and
both update paths. Desktop resource behavior would be that of a Tauri WebView
running the exported UI, not mobile RN native views. Evaluate actual reusable
screens before paying for this integration in the minimal first slice.

Before selecting the foundation, resolve the real KDE Wayland hold and insertion
adapter, background lifecycle, recognition process/model lifetime, and desktop
packaging/update behavior. Later mobile work needs its own capture, permissions,
and background contract. Apple's [background-service rule, 2.5.4](https://developer.apple.com/app-store/review/guidelines/#software-requirements)
and Android's [foreground-service policy](https://support.google.com/googleplay/android-developer/answer/16559646?hl=en)
apply regardless of UI framework. A desktop tray workflow does not establish a
mobile background workflow.

Mobile UI portability also does not establish dictation and insertion across
other apps. Apple's
[custom-keyboard guide](https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/CustomKeyboard.html)
excludes microphone access. Its current
[open-access documentation](https://developer.apple.com/documentation/uikit/configuring-open-access-for-a-custom-keyboard)
also lists that restriction and describes added network and container access.
An iOS keyboard must not be assumed to reproduce desktop capture. Select a
feasible mobile interaction before committing to shared UI behavior.
