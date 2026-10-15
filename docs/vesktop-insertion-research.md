# Vesktop insertion alternatives

Research date: 2026-10-15. Upstream sources and bounded native probes against installed Vesktop 1.6.7. No restart or global setting change.

## Finding

A standard AT-SPI `GetAttributes` and `GetRelationSet` probe exposed the focused composer in the running Vesktop client. Chromium deliberately uses these ATK requests to activate its renderer Accessibility tree lazily. This can preserve Dipper's focused-editor checks without a client restart, startup flag, global screen-reader setting, or weaker window-only guard. Native editor availability was verified below; automatic insertion still fails the separate ACTIVE-frame requirement.

In Chromium, `AtkGetAttributes` and `AtkRefRelationSet` both call `AXPlatform::OnExtendedPropertiesUsedInWebContent`; the comments explicitly identify these calls as signals to enable Accessibility. The browser implementation enables `kAXModeBasic | kExtendedProperties`. Dipper can discard returned attributes and retry its bounded role/state proof after the provider has populated its tree. This is an activation hint, never proof that the target is editable. [ATK provider implementation](https://github.com/chromium/chromium/blob/main/ui/accessibility/platform/ax_platform_node_auralinux.cc), [browser mode implementation](https://github.com/chromium/chromium/blob/main/content/browser/accessibility/browser_accessibility_state_impl.cc).

Chromium intentionally delays full Accessibility support until it detects assistive technology to avoid the cost of maintaining a renderer DOM accessibility cache. Ordinary keyboard focus and Ctrl+V do not depend on that cache. Therefore a working manual paste is consistent with an absent accessible editor. [Chromium technical documentation](https://www.chromium.org/developers/design-documents/accessibility/).

## Other paths

| Path | Evidence and practical status |
| --- | --- |
| Native AT-SPI lazy activation | Explicit source-supported path above. Live activation succeeded with both requests. Individual-call sufficiency remains untested. No guarantee for every Electron application. |
| Register AT-SPI event listeners | Useful for observing provider events, but no evidence found that listener registration alone activates the renderer in this Chromium implementation. Do not promise it as the fix. |
| Electron `app.setAccessibilitySupportEnabled` | Official API is marked macOS/Windows. It is an application-owned API, not a public Linux external-client control. Not a supported Linux runtime solution. [Electron app API](https://www.electronjs.org/docs/latest/api/app#appsetaccessibilitysupportenabledenabled-macos-windows) |
| Vesktop settings or existing IPC | Current upstream settings and IPC expose no Accessibility toggle or focused-editor insertion bridge. Absence is bounded to the inspected sources, not proof that arbitrary custom builds lack one. [settings](https://github.com/Vencord/Vesktop/blob/main/src/shared/settings.d.ts), [IPC](https://github.com/Vencord/Vesktop/blob/main/src/main/ipc.ts) |
| Input-method integration | A real alternative architecture. An input method can receive active text-input context and content purpose, then commit text without clipboard shortcuts. KWin starts its selected input method with a dedicated `WAYLAND_SOCKET`; it is not a general observer connection available to every ordinary application. Fcitx documents compositor and application setup requirements, including Chromium flags that vary with version. This needs a separate integration contract and actual app compatibility tests. [KWin implementation](https://github.com/KDE/kwin/blob/master/src/inputmethod.cpp), [Fcitx Wayland guide](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland) |
| Cooperative Vencord plugin | Could validate the actual DOM editor and perform insertion in the client, but this is new client-specific integration. Official custom-plugin installation requires a source build and restart. It is not a no-setup drop-in fix and couples Dipper to Discord's changing editor. [Vencord custom plugins](https://docs.vencord.dev/installing/custom-plugins/) |
| Simulate Ctrl+V without editor proof | Technically explains the successful manual test, but cannot independently distinguish an editable composer from password, read-only, or another focused control. Retains window identity checks but changes the accepted guard contract. |

## Recommendation

Review a bounded native activation request and the separate ACTIVE-frame incompatibility before implementation. Test GetRelationSet alone on a fresh lazy provider, then rerun full editor validation. Keep identity, focus-loss, password/read-only, deadline, and complete-tree checks. Do not log attribute values or private text. If it fails, the evidence supports either an app configuration change, a deliberate reduced-proof mode, or a larger cooperative/input-method integration; it does not establish that Ctrl+V itself is unavailable.

## Live result from native probes

The native bounded root/window `GetAttributes` and `GetRelationSet` probe succeeded: the existing Vesktop PID 1665595 exposed a focused editable ENTRY without a restart or global setting change. Because both requests were used, this does not isolate which individual call was sufficient. Chromium source supports testing `GetRelationSet` alone in the implementation to avoid requesting attribute values.

A second incompatibility remains: Vesktop reports its only frame as not ACTIVE even while KWin reports this exact window active. A focus roundtrip did not change that. The frame's component screen extents were `[0, 0, 2048, 1208]` versus KWin `[0, 0, 2048, 1236]`, consistent with a 28-pixel titlebar exclusion; geometry equality is therefore unsuitable as an assumed identity proof.

The current guard still rejects this state. A separately reviewed contract could accept KWin's active window plus an unambiguous single top-level AT-SPI frame in the verified process, while retaining field role/state, ancestor, password/read-only, ownership, traversal bounds, and focus-loss checks. Any process exposing multiple top-level frames must remain rejected unless frame identity can be proven independently. This is a targeted provider-compatibility proposal, not permission to remove editor validation. A complete-tree proof must cover top-level frame ambiguity, including off-screen or inactive frames. No production implementation or automatic Discord acceptance is claimed by this research.

Final native probe: a complete 1,830-node tree contained exactly one frame and one focused editable ENTRY. The entry was enabled, sensitive, showing, and visible; read-only, defunct, and stale states were absent. These facts establish editor availability after activation, but do not satisfy the existing ACTIVE-frame contract. The single-frame/KWin fallback remains a proposal outside accepted implementation scope.
