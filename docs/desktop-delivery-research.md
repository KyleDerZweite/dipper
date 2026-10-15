# Dipper desktop delivery research

2026-10-15. Research only. No builds, installation, app restart, settings changes, key creation, or publishing performed.

## Recommendation

Install an optimized, versioned per-user build now on Kyle's Fedora 43 KDE system. Use a stable launcher and desktop entry pointing to an immutable release directory containing `dipper`, `dipper-worker`, `dipper-clipboard`, and the pinned Whisper shared libraries. Keep a previous installed version for explicit rollback. Package the same payload as an archive for a reproducible handoff. This recommendation is an implementation choice for the existing workstation, not a claim of cross-distribution compatibility.

For public Fedora distribution, prefer an RPM once its installed-path and upgrade contract is tested. AppImage remains attractive for later portable distribution and unprivileged in-app updates, but adds WebKitGTK bundling and compatibility verification. A local versioned directory should not be presented as an RPM or an AppImage.

Keep GitHub Releases as the eventual release host. Start with manually promoted preview releases, then stable releases after target acceptance. Prepare the pipeline and release metadata now; signing-key custody, public publication and unattended update behavior are separate decisions. Do not add polling or a silent installer just to finish local installation.

## Existing repository constraints

`apps/desktop/tauri.conf.json` disables bundling, defines `io.github.KyleDerZweite.dipper`, and has version `0.1.0`. The workspace version is also `0.1.0`; release tooling must keep one authoritative version or validate equality.

`apps/desktop/src/paths.rs` resolves both helpers next to the running executable. A package containing only the GUI fails Recognition/Paste. It keeps normal data under `~/.local/share/dipper/app` and diagnostics under `~/.local/state/dipper/app`; install/uninstall must never remove these or model files. Existing hidden generated desktop entries can shadow a system desktop entry. Installation must migrate only recognized generated entries, preserving user-edited entries and the portal identifier.

`Settings::load` preserves existing `native_lib_dir` and model paths. Merely setting `DIPPER_WHISPER_DIR` in a launcher will not migrate saved native-library paths. Resolve the packaged runtime independently or provide an explicit narrowly owned migration, while preserving the model selection and all other settings. The worker receives the selected native directory as `LD_LIBRARY_PATH`.

`crates/dipper-worker/build.rs` pins whisper.cpp `0c7be696371069e0baa2515ed7aa08d5a5473aa6`, links Whisper/GGML/Vulkan shared libraries, and currently embeds an absolute build-directory RPATH. Validate dependency closure and installed-path runtime loading with the development path unavailable. Do not publish build-path-dependent binaries. Do not bundle the owner's recordings, model-comparison corpus, or arbitrary prototype directory. System PipeWire, Vulkan drivers, portal/KWin services and the pre-existing ydotool daemon remain host dependencies.

Sources: inspected local `README.md`, `docs/development.md`, `docs/adr/0002-rust-core-tauri-desktop.md`, `Cargo.toml`, `apps/desktop/src/paths.rs`, `crates/dipper-core/src/settings.rs`, `crates/dipper-worker/build.rs`. `/etc/os-release` confirms Fedora 43 KDE.

## Packaging facts

Tauri supports RPM dependencies, custom files, desktop templates, package scripts and GPG signing. A Linux binary built on a newer base can require a newer glibc; its supported distribution baseline must be stated. For Fedora-only delivery, test the exact RPM on Fedora, including all helper and native library paths. [Tauri RPM documentation](https://v2.tauri.app/distribute/rpm/)

AppImage bundles application dependencies and usually produces a larger artifact. It still has a glibc baseline requirement. Tauri supports custom AppImage files; its media-framework bundling is a distinct option and is not automatically needed for direct PipeWire capture. Test the native Wayland/KWin/portal path on the actual AppImage rather than assuming development-build equivalence. [Tauri AppImage documentation](https://v2.tauri.app/distribute/appimage/)

## Updater facts, including a documentation discrepancy

The general updater guide still describes Linux AppImage artifacts and mandatory signatures. The public verification key is embedded in app configuration; the private signing key must remain protected and backed up. The guide documents HTTPS endpoints, static JSON containing version/URL/signature and a GitHub Releases `latest.json` endpoint. It also documents creating updater artifacts during packaging. [Tauri updater guide](https://v2.tauri.app/plugin/updater/)

Do not conclude that RPM cannot use Tauri's updater. Released updater **2.10.0** added Deb and RPM support, and the inspected **2.12.0 tag** includes it. This is released functionality, not merely unreleased `v2` branch code. [Versioned updater changelog](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.12.0/plugins/updater/CHANGELOG.md)

Versioned source shows RPM install invokes `rpm -U`, attempts `pkexec`, then graphical password/sudo fallbacks. It does not invoke `dnf` to resolve new dependencies. Therefore an RPM updater requires privilege UX and dependency-change handling; using it does not make upgrades automatically unattended. Source supports installer-specific targets. Pin and inspect the exact selected plugin/CLI versions before implementing. [Updater 2.12.0 source](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.12.0/plugins/updater/src/updater.rs)

For Dipper, the application lifecycle must gate install/restart: no active capture, worker, journal finalization or clipboard dispatch. Binary rollback is not automatically data rollback. If schema migrations become irreversible, retain compatibility checks and a journal-aware backup/restore contract before claiming rollback works.

## GitHub release cycle proposal

Tauri Action supports draft releases, prerelease marking, tag/version naming and signing input. It can generate updater JSON. Pin actions to reviewed commits and build from an exact source revision with the pinned native runtime; existing protocol-only CI cannot establish GPU/native-worker acceptance. [Tauri Action](https://github.com/tauri-apps/tauri-action)

GitHub supports creating a draft, attaching all assets, then publishing; this is the recommended order for immutable releases. Include platform/architecture, minimum supported system, source revision, runtime revision, checksums and known limitations in each release. [GitHub release management](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository)

`releases/latest` selects a published non-draft, non-prerelease release. A preview channel therefore needs an explicit channel manifest or selected preview tag, rather than accidentally pointing it at `latest`. Public release reads need no user token. [GitHub release API](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)

Suggested cadence: useful verified slices become preview candidates; promote a stable release after accumulated target acceptance, with prompt patch releases for regressions. No arbitrary weekly release obligation. Initial UI can expose version and a link to release notes/manual download. Later add explicit Check for updates, show version/notes/download size, download only on request, verify signature, and offer Install and restart when idle. Keep preview opt-in and stable as the normal channel. This cadence is a proposal, not existing behavior.

## Acceptance for this local installation

Verify release profile, exact revision, all three executable paths, pinned native libraries and licenses. Launch from the desktop menu without repository cwd or shell exports; configure/reuse Hold; verify tray/close/reopen, model load, capture and worker reaping. Confirm data/settings/Journal/audio hashes are preserved through install and reinstall. Exercise upgrade/rollback in disposable stores, plus a failure before switching the active version. Keep only explicitly owned version directories subject to uninstall; never recursively delete `~/.local/share/dipper`.
