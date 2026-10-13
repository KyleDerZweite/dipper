# Journal and data retention

Data retention is configurable. Kyle wants comprehensive history for personal
analysis and model comparison; other users must be able to retain less data.
Retention defaults, available controls, storage design, and recovery limits
remain open. These requirements do not select an implementation.

## Retention policy

Define controls for recordings, recognition attempts, transcripts, processing
prompts and results, corrections, insertion outcomes, and diagnostic logs.
Specify how the policy governs collection and retention, including duration,
limits, and intentional deletion. Do not assume every user wants indefinite
storage or comprehensive logging.

Comprehensive retention is Kyle's "data goblin" use case and is required for his
pilot. It retains the complete history described below, including available
capture audio and diagnostic logs for canceled or failed dictations. Automatic
expiry must not silently remove data in this mode. Other policies may expire or
omit data according to the user's configuration.

Show when playback, replay, export, or analysis is limited by data that was not
retained or has been deleted. Missing history must not appear to be a complete
record. A retention policy change does not resolve what happens to existing
data; that behavior must be specified explicitly.

## Pilot comparison

The pilot compares comprehensive retention with a reduced-retention configuration
that does not persist audio or detailed diagnostic logs, but retains text and
explicit corrections. This comparison does not choose a product default. The
exact text artifacts to retain and transient collection behavior still need a
contract. Privately stored evaluation recordings remain separate from the
application's retained history.

## Comprehensive history

Create a durable recording identity at capture start. Preserve complete capture
audio before voice activity detection or trimming and distinguish it from the
actual model input. Record sample rate, channel count, and normalization.

Each recognition attempt has its own identity, source recording, model and
engine versions, relevant settings, local artifact hashes, timings, result,
and error status. Retries add attempts rather than overwrite earlier results.
Keep raw transcripts, processing prompts and results, insertion outcomes, and
confirmed wording separate. For cloud models, record available provider
metadata and identify unknown versions.

Cancellation and failures retain available data and an explicit status. Define
crash-loss limits, recovery behavior, and storage-full behavior. A final-text
paste hook or history importer cannot recover events that were never recorded.

## Pilot storage-full behavior

Kyle accepted clear rejection of new recordings when storage is insufficient,
with existing data preserved and no automatic deletion of older records. This
is the pilot policy, not a permanent product default. Report failures during
capture or processing separately and preserve available data without claiming
completeness.

Later storage limits must be configurable. Comprehensive retention does not
mean unlimited disk growth or a commitment to storing hundreds of gigabytes.
Use measured storage growth to inform limits and policy choices in issue #4.

## Storage and export

SQLite with separate lossless audio files remains a candidate. Their writes do
not share one atomic transaction, so recovery and consistent export need a
concrete design. Store private recordings, journals, logs, and credentials
outside the Git checkout. Credentials remain in protected stores and must not
be copied into diagnostic logs.

Export preserves the retained audio, identities, attempts, settings, and
corrections independently of the application UI. Measure the storage and
runtime cost of each retention configuration alongside recognition and
processing performance.
