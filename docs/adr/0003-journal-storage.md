# Store journal metadata in SQLite and audio in separate lossless files

On 2026-10-13, the owner accepted SQLite for Dipper's journal metadata and
separate lossless audio files. This supports recording identities, distinct
recognition attempts, and later search without embedding large audio payloads
in every metadata query. The audio codec and detailed schema remain implementation
choices within the accepted lossless-capture requirement.

Write audio during capture and preserve available data after interruption.
Recovery marks unfinished recordings as interrupted rather than completed.
Do not automatically insert text after restart. SQLite commits and audio-file
writes do not form one atomic transaction; the Journal interface must expose
and reconcile incomplete state rather than claim success from metadata alone.

The pilot storage-full policy remains unchanged: reject new recordings when
storage is insufficient, preserve old data, and never automatically purge it.
No numerical crash-loss bound or permanent retention limit is selected here.
