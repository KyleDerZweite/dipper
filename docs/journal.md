# Journal requirements

These requirements describe the intended journal. Storage design and recovery limits remain open; they do not establish a selected implementation.

Create a durable recording identity at capture start. Preserve complete capture
audio before voice activity detection or trimming and distinguish it from the
actual model input. Record sample rate, channel count, and normalization.

Each recognition attempt should have its own identity, source recording, model
and engine versions, relevant settings, local artifact hashes, timings, result,
and error status. Retries add attempts rather than overwrite earlier results.
Keep raw recognition, processing prompts and results, insertion outcomes, and
user-confirmed corrections separate. For cloud models, record available provider
metadata and identify unknown versions.

Cancellation and failures retain available data and an explicit status. Define
crash-loss limits, recovery behavior, and what happens when storage is full.
Neither a final-text paste hook nor a history importer can recover events that
were never recorded. Silent retention expiry conflicts with a complete journal;
intentional deletion behavior still needs a decision.

SQLite with separate lossless audio files is a candidate. Their writes do not
share one atomic transaction, so recovery and consistent export need a concrete
design. Store private recordings, journals, and credentials outside the Git
checkout. Export should preserve audio, identities, attempts, settings, and
corrections independently of the application UI.
