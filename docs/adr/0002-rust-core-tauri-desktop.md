# Use a Rust core with Tauri for desktop

On 2026-10-13, the owner accepted a Rust core with Tauri 2 for Dipper's desktop
app, following the [React Native and Rust research](../toolchain-research.md).
Tauri's documented Linux and desktop-update paths fit the initial Fedora KDE
workflow better than the examined React Native Linux port. Core behavior and
platform integrations remain separate from the UI so later mobile work can
reuse validated logic without inheriting desktop controls.

Mobile UI remains undecided; React Native/Expo is an option for the long-term
compatible-JS update requirement. React Native for Web and mobile bindings are
not initial dependencies. Tauri's official updater does not solve mobile OTA,
and its stock shortcut plugin does not establish native Wayland hold-to-record.
These need separate platform contracts and actual integration evidence.

The choice does not claim a measured RAM or latency advantage, select a frontend
framework or production license, or add updater delivery to the first slice.
