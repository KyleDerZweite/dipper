# Build Dipper as its own app

On 2026-10-13, the owner chose an independent Dipper app using established audio
and ASR runtimes, rather than extending an existing desktop app. This allows
Dipper's dictation workflow, journal, and configurable retention to define the
app while accepting responsibility for desktop integration and maintenance.
The subsequent [ADR-0002](0002-rust-core-tauri-desktop.md) selects Rust and Tauri
for desktop. Runtime lifetime, storage, and license remain separate decisions; the throwaway prototype is evidence, not the production foundation.
