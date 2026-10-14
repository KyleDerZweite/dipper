//! Durable capture identities and artifact ordering across SQLite and audio files.
use crate::{
    audio::{self, AudioArtifact, AudioWriter, CHANNELS, DATA_OFFSET, SAMPLE_RATE},
    capture::{CaptureStop, InputDevice, SampleRing},
};
use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs::{self, DirBuilder, File, OpenOptions},
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const ADMISSION_FLOOR: u64 = 64 * 1024 * 1024;
const SCHEMA: &str = r#"
CREATE TABLE recordings (
 id TEXT PRIMARY KEY, started_at TEXT NOT NULL, ended_at TEXT,
 state TEXT NOT NULL CHECK(state IN ('initializing','capturing','complete','canceled','failed','interrupted')),
 input_json TEXT NOT NULL, retention_json TEXT NOT NULL,
 audio_path TEXT NOT NULL UNIQUE, associated INTEGER NOT NULL DEFAULT 0,
 sample_rate INTEGER NOT NULL DEFAULT 48000, channels INTEGER NOT NULL DEFAULT 1,
 data_offset INTEGER NOT NULL DEFAULT 92,
 accepted_frames INTEGER NOT NULL DEFAULT 0 CHECK(accepted_frames>=0),
 available_frames INTEGER NOT NULL DEFAULT 0 CHECK(available_frames>=0),
 synced_frames INTEGER NOT NULL DEFAULT 0 CHECK(synced_frames>=0),
 checkpoint_at TEXT, availability TEXT NOT NULL DEFAULT 'pending', error TEXT
);
CREATE TABLE attempts (
 id TEXT PRIMARY KEY, recording_id TEXT NOT NULL REFERENCES recordings(id),
 started_at TEXT NOT NULL, ended_at TEXT, state TEXT NOT NULL DEFAULT 'preparing',
 config_json TEXT NOT NULL, details_json TEXT NOT NULL DEFAULT '{}', raw_text TEXT,
 log_path TEXT NOT NULL UNIQUE, diagnostic_error TEXT
);
CREATE TABLE insertions (
 id TEXT PRIMARY KEY, attempt_id TEXT NOT NULL REFERENCES attempts(id),
 started_at TEXT NOT NULL, ended_at TEXT, target_json TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', stage TEXT NOT NULL DEFAULT 'none',
 details_json TEXT NOT NULL DEFAULT '{}'
);
PRAGMA user_version=1;
"#;

#[derive(Clone)]
pub struct Journal {
    inner: Arc<Inner>,
}
struct Inner {
    connection: Mutex<Connection>,
    _lock: StoreLock,
    data_dir: PathBuf,
    state_dir: PathBuf,
    admission_floor: u64,
    recovery: Mutex<RecoveryReport>,
    active_captures: AtomicUsize,
}
struct StoreLock(File);
impl Drop for StoreLock {
    fn drop(&mut self) {
        // Release explicitly after SQLite drops, including brief fork/exec fd copies.
        let _ = FileExt::unlock(&self.0);
    }
}
struct ActiveCapture(Arc<Inner>);
impl Drop for ActiveCapture {
    fn drop(&mut self) {
        self.0.active_captures.fetch_sub(1, Ordering::AcqRel);
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecoveryReport {
    pub interrupted: Vec<Uuid>,
    pub orphan_files: Vec<PathBuf>,
    pub unpersisted: bool,
    pub error: Option<String>,
    pub records: Vec<RecoveryRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub recording_id: Uuid,
    pub state: String,
    pub available_frames: u64,
    pub availability: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttemptSnapshot {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub state: String,
    pub raw_text: Option<String>,
    pub configuration: Value,
    pub details: Value,
    pub diagnostic_error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InsertionSnapshot {
    pub id: Uuid,
    pub attempt_id: Uuid,
    pub state: String,
    pub stage: String,
    pub target: Value,
    pub details: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecordingSnapshot {
    pub recording_id: Uuid,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub audio: Option<AudioArtifact>,
    pub accepted_frames: u64,
    pub available_frames: u64,
    pub synced_frames: u64,
    pub availability: String,
    pub error: Option<String>,
    pub persisted: bool,
    pub attempts: Vec<AttemptSnapshot>,
    pub insertions: Vec<InsertionSnapshot>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptureOutcome {
    pub recording_id: Uuid,
    pub state: String,
    pub audio: Option<AudioArtifact>,
    pub accepted_frames: u64,
    pub available_frames: u64,
    pub synced_frames: u64,
    pub error: Option<String>,
    pub persisted: bool,
}

pub struct CaptureTicket {
    pub recording_id: Uuid,
    pub(crate) input: InputDevice,
    pub(crate) ring: Arc<SampleRing>,
    stop: Arc<AtomicBool>,
    reason: Arc<Mutex<Option<CaptureStop>>>,
    writer: Option<JoinHandle<Result<CaptureOutcome>>>,
}
impl CaptureTicket {
    /// Synthetic producer for interface fault tests; it does not open a device.
    /// This ticket cannot be used concurrently with a native CaptureSession.
    pub fn push_synthetic(&mut self, samples: &[f32]) -> bool {
        self.ring.push_synthetic(samples)
    }
    pub fn finish(mut self, reason: CaptureStop) -> Result<CaptureOutcome> {
        *self
            .reason
            .lock()
            .map_err(|_| anyhow::anyhow!("capture stop state poisoned"))? = Some(reason);
        self.stop.store(true, Ordering::Release);
        self.writer
            .take()
            .context("capture writer already settled")?
            .join()
            .map_err(|_| anyhow::anyhow!("capture writer panicked"))?
    }
}
impl Drop for CaptureTicket {
    fn drop(&mut self) {
        if let Some(writer) = self.writer.take() {
            if let Ok(mut reason) = self.reason.lock() {
                *reason = Some(CaptureStop::Failed("capture ticket abandoned".into()));
            }
            self.stop.store(true, Ordering::Release);
            let _ = writer.join();
        }
    }
}

fn now() -> String {
    format!(
        "{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}
fn frame_count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let count: i64 = row.get(index)?;
    u64::try_from(count).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}
fn private_dir(path: &Path) -> Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    ensure!(
        fs::metadata(path)?.permissions().mode() & 0o077 == 0,
        "Journal directory must be user-only: {}",
        path.display()
    );
    Ok(())
}
fn free_bytes(path: &Path) -> Result<u64> {
    let encoded = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(encoded.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
}

impl Journal {
    pub fn open(data_dir: impl AsRef<Path>, state_dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_admission_floor(data_dir, state_dir, ADMISSION_FLOOR)
    }
    pub fn open_with_admission_floor(
        data_dir: impl AsRef<Path>,
        state_dir: impl AsRef<Path>,
        admission_floor: u64,
    ) -> Result<Self> {
        let data_dir = data_dir.as_ref();
        let state_dir = state_dir.as_ref();
        private_dir(data_dir)?;
        private_dir(state_dir)?;
        private_dir(&data_dir.join("audio"))?;
        private_dir(&state_dir.join("logs"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(data_dir.join("journal.lock"))?;
        lock.try_lock_exclusive()
            .context("another Dipper instance owns this Journal")?;
        let db_path = data_dir.join("journal.sqlite3");
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&db_path)
        {
            Ok(file) => {
                file.sync_all()?;
                File::open(data_dir)?.sync_all()?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        ensure!(
            fs::metadata(&db_path)?.permissions().mode() & 0o077 == 0,
            "Journal database must be user-only"
        );
        let mut connection = Connection::open(&db_path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        ensure!(
            version <= 1,
            "Journal schema is newer than this application"
        );
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "temp_store", "MEMORY")?;
        let wal: String = connection.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        let sync: u32 = connection.query_row("PRAGMA synchronous", [], |r| r.get(0))?;
        let foreign: u32 = connection.query_row("PRAGMA foreign_keys", [], |r| r.get(0))?;
        ensure!(
            wal == "wal" && sync == 2 && foreign == 1,
            "Journal persistence settings could not be applied"
        );
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(SCHEMA)?;
            transaction.commit()?;
        }
        let journal = Self {
            inner: Arc::new(Inner {
                connection: Mutex::new(connection),
                _lock: StoreLock(lock),
                data_dir: data_dir.to_owned(),
                state_dir: state_dir.to_owned(),
                admission_floor,
                recovery: Mutex::new(RecoveryReport::default()),
                active_captures: AtomicUsize::new(0),
            }),
        };
        journal.recover()?;
        Ok(journal)
    }
    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.inner
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("Journal writer state poisoned"))
    }
    pub fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }
    pub fn recovery_report(&self) -> RecoveryReport {
        self.inner
            .recovery
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default()
    }
    pub fn begin_capture(&self, input: &InputDevice) -> Result<CaptureTicket> {
        ensure!(
            free_bytes(&self.inner.data_dir)? >= self.inner.admission_floor
                && free_bytes(&self.inner.state_dir)? >= self.inner.admission_floor,
            "insufficient storage for a new recording"
        );
        let id = Uuid::new_v4();
        let relative = format!("audio/{id}.wav");
        let input_json = json!({"device":input,"backend":"PipeWire","backend_version":crate::capture::Capture::backend_version(),"delivered_format":"f32le","sample_rate":SAMPLE_RATE,"channels":CHANNELS,"upstream_conversion":"requested 48kHz mono; upstream processing not independently verified"});
        let connection = self.connection()?;
        self.inner.active_captures.fetch_add(1, Ordering::AcqRel);
        let active = ActiveCapture(self.inner.clone());
        connection.execute("INSERT INTO recordings(id,started_at,state,input_json,retention_json,audio_path) VALUES(?1,?2,'initializing',?3,?4,?5)", params![id.to_string(), now(), input_json.to_string(), json!({"policy":"comprehensive","version":1}).to_string(), relative])?;
        drop(connection);
        let path = self.inner.data_dir.join(&relative);
        let audio = match AudioWriter::create(&path) {
            Ok(audio) => audio,
            Err(e) => {
                let _ = self.connection()?.execute("UPDATE recordings SET state='failed',ended_at=?2,error=?3,availability='incomplete' WHERE id=?1", params![id.to_string(),now(),format!("audio initialization: {e:#}")]);
                return Err(e);
            }
        };
        self.connection()?.execute("UPDATE recordings SET associated=1,state='capturing',availability='available' WHERE id=?1", [id.to_string()])?;
        let ring = SampleRing::new(SAMPLE_RATE as u64 * 2)?;
        let stop = Arc::new(AtomicBool::new(false));
        let reason = Arc::new(Mutex::new(None));
        let journal = self.clone();
        let writer_ring = ring.clone();
        let writer_stop = stop.clone();
        let writer_reason = reason.clone();
        let writer = thread::Builder::new()
            .name("dipper-audio-writer".into())
            .spawn(move || {
                let _active = active;
                journal.write_capture(id, audio, writer_ring, writer_stop, writer_reason)
            })?;
        Ok(CaptureTicket {
            recording_id: id,
            input: input.clone(),
            ring,
            stop,
            reason,
            writer: Some(writer),
        })
    }
    fn checkpoint(&self, id: Uuid, frames: u64) -> Result<()> {
        self.connection()?.execute("UPDATE recordings SET available_frames=?2,synced_frames=?2,checkpoint_at=?3 WHERE id=?1 AND state='capturing'", params![id.to_string(), i64::try_from(frames)?, now()])?;
        Ok(())
    }
    fn write_capture(
        &self,
        id: Uuid,
        mut audio: AudioWriter,
        ring: Arc<SampleRing>,
        stop: Arc<AtomicBool>,
        reason: Arc<Mutex<Option<CaptureStop>>>,
    ) -> Result<CaptureOutcome> {
        let mut samples = [0.0; 4096];
        let mut sync_time = Instant::now();
        let mut synced = 0;
        let mut storage_error = None;
        let mut audio_failed = false;
        let mut checkpoint_failed = false;
        loop {
            let count = ring.read(&mut samples);
            // Metadata failure invalidates completion, but the audio file can
            // still preserve buffers accepted before Capture observes the fault.
            if count > 0 && !audio_failed {
                if let Err(e) = audio.write_samples(&samples[..count]) {
                    audio_failed = true;
                    storage_error.get_or_insert_with(|| format!("audio write failed: {e:#}"));
                    ring.fail_storage();
                }
                ring.written.store(audio.frames(), Ordering::Release);
            }
            if !audio_failed
                && !checkpoint_failed
                && audio.frames() > synced
                && sync_time.elapsed() >= Duration::from_secs(1)
            {
                match audio
                    .checkpoint()
                    .map_err(|e| {
                        // A failed audio checkpoint can leave the file cursor
                        // in its header. Only metadata failure permits appending.
                        audio_failed = true;
                        format!("audio checkpoint failed: {e:#}")
                    })
                    .and_then(|_| {
                        self.checkpoint(id, audio.frames())
                            .map_err(|e| format!("Journal checkpoint failed: {e:#}"))
                    }) {
                    Ok(()) => {
                        synced = audio.frames();
                        sync_time = Instant::now();
                    }
                    Err(e) => {
                        checkpoint_failed = true;
                        storage_error.get_or_insert(e);
                        ring.fail_storage();
                    }
                }
            }
            if count == 0 {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
        let reason = reason
            .lock()
            .map_err(|_| anyhow::anyhow!("capture stop state poisoned"))?
            .take()
            .unwrap_or(CaptureStop::Failed("capture abandoned".into()));
        let mut error = match ring.fault() {
            Some(error) if error != "capture storage failed" => Some(error),
            _ => storage_error,
        }
        .or_else(|| reason.error());
        let frames = audio.frames();
        let path = self.inner.data_dir.join(format!("audio/{id}.wav"));
        let finalized = audio.finalize();
        let audio = match finalized {
            Ok(artifact) => Some(artifact),
            Err(e) => {
                error.get_or_insert(format!("audio finalization failed: {e:#}"));
                None
            }
        };
        let mut state = reason.state().to_owned();
        let accepted = ring.accepted();
        if error.is_some() && state != "interrupted" {
            state = "failed".into();
        }
        if state == "complete" && (accepted != frames || frames == 0) {
            state = "failed".into();
            error = Some("capture did not produce a complete nonempty frame sequence".into());
        }
        let available = audio::available_frames(&path).unwrap_or(frames);
        let candidate_sync = if audio.is_some() { frames } else { synced };
        let availability = if audio.is_some() && available == frames {
            "available"
        } else if available > 0 {
            "recovered"
        } else {
            "incomplete"
        };
        let commit = self.connection()?.execute("UPDATE recordings SET state=?2,ended_at=?3,accepted_frames=?4,available_frames=?5,synced_frames=?6,checkpoint_at=?3,availability=?7,error=?8 WHERE id=?1", params![id.to_string(), state, now(), i64::try_from(accepted)?, i64::try_from(available)?, i64::try_from(candidate_sync)?, availability, error]);
        let persisted = commit.is_ok();
        if let Err(e) = commit {
            state = "failed".into();
            error = Some(match error {
                Some(original) => format!("{original}; terminal Journal commit failed: {e:#}"),
                None => format!("terminal Journal commit failed: {e:#}"),
            });
        }
        Ok(CaptureOutcome {
            recording_id: id,
            state,
            audio,
            accepted_frames: accepted,
            available_frames: available,
            synced_frames: if persisted { candidate_sync } else { synced },
            error,
            persisted,
        })
    }
    pub fn begin_attempt(&self, recording_id: Uuid, configuration: Value) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let relative = format!("logs/{id}.log");
        self.connection()?.execute("INSERT INTO attempts(id,recording_id,started_at,config_json,log_path) VALUES(?1,?2,?3,?4,?5)", params![id.to_string(),recording_id.to_string(),now(),configuration.to_string(),relative])?;
        Ok(id)
    }
    pub fn finish_attempt(
        &self,
        id: Uuid,
        outcome: &str,
        raw_text: Option<&str>,
        details: Value,
    ) -> Result<()> {
        ensure!(
            ["complete", "failed", "canceled", "interrupted", "no_text"].contains(&outcome),
            "invalid recognition outcome"
        );
        ensure!(
            raw_text.is_none_or(|s| s.len() <= 1024 * 1024),
            "raw transcript exceeds limit"
        );
        let changed = self.connection()?.execute("UPDATE attempts SET state=?2,ended_at=?3,raw_text=?4,details_json=?5 WHERE id=?1 AND ended_at IS NULL", params![id.to_string(),outcome,now(),raw_text,details.to_string()])?;
        ensure!(
            changed == 1,
            "recognition attempt is missing or already terminal"
        );
        Ok(())
    }
    pub fn diagnostics_path(&self, id: Uuid) -> PathBuf {
        self.inner.state_dir.join(format!("logs/{id}.log"))
    }
    pub fn open_diagnostics(&self, id: Uuid) -> Result<File> {
        let present: bool = self.connection()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM attempts WHERE id=?1)",
            [id.to_string()],
            |r| r.get(0),
        )?;
        ensure!(present, "diagnostic attempt is unknown");
        OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .open(self.diagnostics_path(id))
            .map_err(Into::into)
    }
    pub fn record_diagnostic_failure(&self, id: Uuid, error: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE attempts SET diagnostic_error=?2 WHERE id=?1",
            params![id.to_string(), error],
        )?;
        Ok(())
    }
    pub fn begin_insertion(&self, attempt_id: Uuid, target: Value) -> Result<Uuid> {
        let id = Uuid::new_v4();
        self.connection()?.execute(
            "INSERT INTO insertions(id,attempt_id,started_at,target_json) VALUES(?1,?2,?3,?4)",
            params![
                id.to_string(),
                attempt_id.to_string(),
                now(),
                target.to_string()
            ],
        )?;
        Ok(id)
    }
    pub fn finish_insertion(
        &self,
        id: Uuid,
        outcome: &str,
        stage: &str,
        details: Value,
    ) -> Result<()> {
        ensure!(
            [
                "unverified",
                "confirmed",
                "failed",
                "suppressed",
                "canceled",
                "uncertain"
            ]
            .contains(&outcome),
            "invalid insertion outcome"
        );
        let changed = self.connection()?.execute("UPDATE insertions SET state=?2,stage=?3,ended_at=?4,details_json=?5 WHERE id=?1 AND ended_at IS NULL", params![id.to_string(),outcome,stage,now(),details.to_string()])?;
        ensure!(changed == 1, "insertion is missing or already terminal");
        Ok(())
    }
    /// Idempotent startup reconciliation. No recognition or insertion side effect.
    pub fn recover(&self) -> Result<RecoveryReport> {
        let mut connection = self.connection()?;
        ensure!(
            self.inner.active_captures.load(Ordering::Acquire) == 0,
            "cannot recover while capture is active"
        );
        let rows: Vec<(String,String,String,u64,bool,u64)> = connection.prepare("SELECT id,state,audio_path,synced_frames,associated,available_frames FROM recordings")?.query_map([], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,frame_count(r,3)?,r.get(4)?,frame_count(r,5)?)))?.collect::<rusqlite::Result<_>>()?;
        let mut report = RecoveryReport::default();
        let mut known = HashSet::new();
        let mut updates = vec![];
        for (id, state, relative, synced, associated, previous_available) in rows {
            let relative_path = Path::new(&relative);
            ensure!(
                relative_path
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                    && relative.starts_with("audio/"),
                "invalid Journal artifact path"
            );
            known.insert(relative.clone());
            let path = self.inner.data_dir.join(&relative);
            let unfinished = state == "initializing" || state == "capturing";
            let observed = audio::available_frames(&path);
            let (available, availability, error) = match observed {
                Ok(frames) if !associated => (
                    frames,
                    "uncertain",
                    Some("audio association was not committed".to_owned()),
                ),
                Ok(frames) if frames < synced => (
                    frames,
                    "inconsistent",
                    Some("audio is shorter than its synchronized checkpoint".to_owned()),
                ),
                Ok(frames) if state == "complete" => {
                    let artifact = AudioArtifact {
                        path: path.clone(),
                        sample_rate: SAMPLE_RATE,
                        channels: CHANNELS,
                        frames: previous_available,
                        data_offset: DATA_OFFSET,
                    };
                    // Header/size validation without allocating or hashing historical payload.
                    let valid = validate_header_size(&artifact);
                    if valid.is_ok() {
                        (frames, "available", None)
                    } else {
                        (
                            frames,
                            "inconsistent",
                            Some("finalized audio header or size changed".into()),
                        )
                    }
                }
                Ok(frames) => (
                    frames,
                    if unfinished || state == "interrupted" {
                        "recovered"
                    } else {
                        "available"
                    },
                    None,
                ),
                Err(e) => (
                    0,
                    if path.exists() {
                        "inconsistent"
                    } else {
                        "missing"
                    },
                    Some(format!("audio unavailable: {e:#}")),
                ),
            };
            if unfinished {
                report.interrupted.push(Uuid::parse_str(&id)?);
            }
            updates.push((
                id,
                if unfinished {
                    "interrupted".to_owned()
                } else {
                    state
                },
                available,
                availability.to_owned(),
                error,
            ));
        }
        for entry in fs::read_dir(self.inner.data_dir.join("audio"))? {
            let entry = entry?;
            let relative = format!("audio/{}", entry.file_name().to_string_lossy());
            if !known.contains(&relative) {
                report.orphan_files.push(entry.path());
            }
        }
        for (id, state, available_frames, availability, error) in &updates {
            report.records.push(RecoveryRecord {
                recording_id: Uuid::parse_str(id)?,
                state: state.clone(),
                available_frames: *available_frames,
                availability: availability.clone(),
                error: error.clone(),
            });
        }
        let update_result = (|| -> Result<()> {
            let transaction = connection.transaction()?;
            for (id, state, frames, availability, error) in &updates {
                transaction.execute("UPDATE recordings SET state=?2,available_frames=?3,availability=?4,error=COALESCE(error,?5),ended_at=CASE WHEN ?2='interrupted' THEN COALESCE(ended_at,?6) ELSE ended_at END WHERE id=?1", params![id,state,i64::try_from(*frames)?,availability,error,now()])?;
            }
            transaction.execute(
                "UPDATE attempts SET state='interrupted',ended_at=?1 WHERE ended_at IS NULL",
                [now()],
            )?;
            transaction.execute(
                "UPDATE insertions SET state='uncertain',ended_at=?1 WHERE ended_at IS NULL",
                [now()],
            )?;
            transaction.commit()?;
            Ok(())
        })();
        if let Err(e) = update_result {
            report.unpersisted = true;
            report.error = Some(format!("recovery reconciliation not committed: {e:#}"));
        }
        *self
            .inner
            .recovery
            .lock()
            .map_err(|_| anyhow::anyhow!("recovery state poisoned"))? = report.clone();
        Ok(report)
    }
    pub fn recording(&self, id: Uuid) -> Result<Option<RecordingSnapshot>> {
        let connection = self.connection()?;
        let mut record = connection.query_row("SELECT state,started_at,ended_at,audio_path,accepted_frames,available_frames,synced_frames,availability,error FROM recordings WHERE id=?1", [id.to_string()], |r| Ok(RecordingSnapshot { recording_id:id,state:r.get(0)?,started_at:r.get(1)?,ended_at:r.get(2)?,audio:Some(AudioArtifact {path:self.inner.data_dir.join(r.get::<_,String>(3)?),sample_rate:SAMPLE_RATE,channels:CHANNELS,frames:frame_count(r,5)?,data_offset:DATA_OFFSET}),accepted_frames:frame_count(r,4)?,available_frames:frame_count(r,5)?,synced_frames:frame_count(r,6)?,availability:r.get(7)?,error:r.get(8)?,persisted:true,attempts:vec![],insertions:vec![] })).optional()?;
        if let Some(r) = &mut record {
            if !["available", "recovered"].contains(&r.availability.as_str())
                || matches!(r.state.as_str(), "initializing" | "capturing")
            {
                r.audio = None;
            }
            let report = self.recovery_report();
            if report.unpersisted {
                r.persisted = false;
                if let Some(derived) = report.records.iter().find(|d| d.recording_id == id) {
                    r.state = derived.state.clone();
                    r.available_frames = derived.available_frames;
                    r.availability = derived.availability.clone();
                    r.error = r.error.clone().or(derived.error.clone());
                    r.audio = None;
                }
            }
            r.attempts = connection.prepare("SELECT id,state,raw_text,config_json,details_json,diagnostic_error FROM attempts WHERE recording_id=?1 ORDER BY started_at,id")?.query_map([id.to_string()], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,Option<String>>(5)?)))?.map(|row| { let (attempt_id,state,raw_text,config,details,diagnostic_error) = row?; Ok(AttemptSnapshot {id:Uuid::parse_str(&attempt_id)?,recording_id:id,state,raw_text,configuration:serde_json::from_str(&config)?,details:serde_json::from_str(&details)?,diagnostic_error}) }).collect::<Result<_>>()?;
            r.insertions = connection.prepare("SELECT i.id,i.attempt_id,i.state,i.stage,i.target_json,i.details_json FROM insertions i JOIN attempts a ON a.id=i.attempt_id WHERE a.recording_id=?1 ORDER BY i.started_at,i.id")?.query_map([id.to_string()], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?)))?.map(|row| {let (insertion_id,attempt_id,state,stage,target,details)=row?; Ok(InsertionSnapshot {id:Uuid::parse_str(&insertion_id)?,attempt_id:Uuid::parse_str(&attempt_id)?,state,stage,target:serde_json::from_str(&target)?,details:serde_json::from_str(&details)?})}).collect::<Result<_>>()?;
            if report.unpersisted {
                for attempt in &mut r.attempts {
                    if ["preparing", "running"].contains(&attempt.state.as_str()) {
                        attempt.state = "interrupted".into();
                    }
                }
                for insertion in &mut r.insertions {
                    if insertion.state == "pending" {
                        insertion.state = "uncertain".into();
                    }
                }
            }
        }
        Ok(record)
    }
    pub fn recent(&self, limit: usize) -> Result<Vec<RecordingSnapshot>> {
        let ids: Vec<String> = self.connection()?.prepare("SELECT id FROM recordings ORDER BY CAST(started_at AS INTEGER) DESC,id DESC LIMIT ?1")?.query_map([i64::try_from(limit.min(1000))?], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        ids.into_iter()
            .map(|id| {
                self.recording(Uuid::parse_str(&id)?)
                    .and_then(|r| r.context("recording disappeared during query"))
            })
            .collect()
    }
    pub fn open_audio(&self, id: Uuid) -> Result<AudioArtifact> {
        let record = self.recording(id)?.context("unknown recording")?;
        ensure!(record.persisted, "recovery status is unpersisted");
        let artifact = record.audio.context("audio is missing or unavailable")?;
        if record.state == "complete" {
            validate_header_size(&artifact)?;
        } else {
            ensure!(
                audio::available_frames(&artifact.path)? >= artifact.frames,
                "recovered prefix changed"
            );
        }
        Ok(artifact)
    }
}

fn validate_header_size(artifact: &AudioArtifact) -> Result<()> {
    audio::validate_artifact(artifact)
}

#[cfg(test)]
mod fault_tests {
    use super::*;
    fn input() -> InputDevice {
        InputDevice {
            name: "synthetic".into(),
            description: "Synthetic".into(),
            serial: 42,
        }
    }
    #[test]
    fn sqlite_full_preserves_previously_committed_audio_and_rows() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open_with_admission_floor(
            directory.path().join("data"),
            directory.path().join("state"),
            0,
        )
        .unwrap();
        let mut ticket = journal.begin_capture(&input()).unwrap();
        let id = ticket.recording_id;
        ticket.push_synthetic(&[0.25; 128]);
        let audio = ticket.finish(CaptureStop::Released).unwrap().audio.unwrap();
        let original = fs::read(&audio.path).unwrap();
        let connection = journal.connection().unwrap();
        let pages: u32 = connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        drop(connection);
        let failure = journal
            .begin_attempt(id, json!({"synthetic": "x".repeat(1024*1024)}))
            .unwrap_err();
        assert!(format!("{failure:#}").contains("full"));
        assert_eq!(journal.recording(id).unwrap().unwrap().state, "complete");
        assert_eq!(fs::read(&audio.path).unwrap(), original);
    }
    #[test]
    fn terminal_commit_failure_does_not_acknowledge_complete_audio() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open_with_admission_floor(
            directory.path().join("data"),
            directory.path().join("state"),
            0,
        )
        .unwrap();
        let mut ticket = journal.begin_capture(&input()).unwrap();
        let id = ticket.recording_id;
        assert!(ticket.push_synthetic(&[0.5; 128]));
        journal
            .connection()
            .unwrap()
            .pragma_update(None, "query_only", true)
            .unwrap();
        let outcome = ticket.finish(CaptureStop::Released).unwrap();
        assert_eq!(outcome.state, "failed");
        assert!(!outcome.persisted);
        assert_eq!(outcome.synced_frames, 0);
        assert_eq!(
            audio::read_f32(&outcome.audio.unwrap()).unwrap(),
            vec![0.5; 128]
        );
        journal
            .connection()
            .unwrap()
            .pragma_update(None, "query_only", false)
            .unwrap();
        journal.recover().unwrap();
        let recovered = journal.recording(id).unwrap().unwrap();
        assert_eq!(recovered.state, "interrupted");
        assert_eq!(recovered.available_frames, 128);
        assert_eq!(recovered.synced_frames, 0);
    }
    #[test]
    fn checkpoint_sqlite_full_preserves_all_already_accepted_audio() {
        use std::io::Read;
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open_with_admission_floor(
            directory.path().join("data"),
            directory.path().join("state"),
            0,
        )
        .unwrap();
        let mut ticket = journal.begin_capture(&input()).unwrap();
        let id = ticket.recording_id;
        let path = journal.data_dir().join(format!("audio/{id}.wav"));
        let connection = journal.connection().unwrap();
        connection.execute_batch("CREATE TABLE checkpoint_pressure(payload BLOB); CREATE TRIGGER fill_checkpoint AFTER UPDATE OF synced_frames ON recordings WHEN NEW.state='capturing' BEGIN INSERT INTO checkpoint_pressure VALUES(zeroblob(1048576)); END;").unwrap();
        let pages: u32 = connection
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        connection
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let initial = vec![0.5; 4096];
        let queued = vec![0.25; 8192];
        assert!(ticket.push_synthetic(&initial));
        // The initial audio checkpoint patches its header before it waits for
        // this SQLite lock. The queued tail is accepted before metadata fails.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut header = [0; DATA_OFFSET as usize];
            File::open(&path).unwrap().read_exact(&mut header).unwrap();
            if u64::from_le_bytes(header[36..44].try_into().unwrap()) == initial.len() as u64 {
                break;
            }
            assert!(Instant::now() < deadline, "audio checkpoint did not run");
            thread::sleep(Duration::from_millis(2));
        }
        assert!(ticket.push_synthetic(&queued));
        drop(connection);
        let deadline = Instant::now() + Duration::from_secs(5);
        while ticket.ring.fault().is_none() {
            assert!(
                Instant::now() < deadline,
                "checkpoint SQLite failure was not observed"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let outcome = ticket.finish(CaptureStop::Released).unwrap();
        assert_eq!(outcome.state, "failed");
        assert!(outcome.error.as_deref().unwrap().contains("full"));
        assert_eq!(
            outcome.accepted_frames,
            (initial.len() + queued.len()) as u64
        );
        assert_eq!(outcome.available_frames, outcome.accepted_frames);
        let samples = audio::read_f32(&outcome.audio.unwrap()).unwrap();
        let mut expected = initial;
        expected.extend(queued);
        assert_eq!(samples, expected);
        let record = journal.recording(id).unwrap().unwrap();
        assert_eq!(record.state, "failed");
        assert_eq!(record.available_frames, expected.len() as u64);
    }
    #[test]
    fn unwritable_recovery_exposes_derived_state_then_reconciles() {
        let directory = tempfile::tempdir().unwrap();
        let journal = Journal::open_with_admission_floor(
            directory.path().join("data"),
            directory.path().join("state"),
            0,
        )
        .unwrap();
        let mut ticket = journal.begin_capture(&input()).unwrap();
        let id = ticket.recording_id;
        ticket.push_synthetic(&[0.25; 128]);
        ticket.finish(CaptureStop::Canceled).unwrap();
        journal
            .connection()
            .unwrap()
            .execute(
                "UPDATE recordings SET state='capturing',ended_at=NULL WHERE id=?1",
                [id.to_string()],
            )
            .unwrap();
        journal
            .connection()
            .unwrap()
            .pragma_update(None, "query_only", true)
            .unwrap();
        assert!(journal.recover().unwrap().unpersisted);
        let record = journal.recording(id).unwrap().unwrap();
        assert_eq!(record.state, "interrupted");
        assert!(!record.persisted);
        assert_eq!(record.available_frames, 128);
        journal
            .connection()
            .unwrap()
            .pragma_update(None, "query_only", false)
            .unwrap();
        assert!(!journal.recover().unwrap().unpersisted);
        assert!(journal.recording(id).unwrap().unwrap().persisted);
    }
}
