//! Coordinates the accepted raw chain. Native and UI implementations stay outside it.
use crate::{
    capture::{CaptureProvider, CaptureStop, NativeCaptureProvider},
    journal::Journal,
    protocol::{AudioSource, JobIdentity},
    recognition::{
        RecognitionJob, RecognitionPhase, RecognitionStatus, RecognitionSupervisor,
        SupervisorPolicy, WorkerConfig,
    },
    settings::Settings,
    target::{DeliveryStatus, DispatchStage, Insertion, InsertionOutcome, TargetPermit},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct DictationSnapshot {
    pub phase: String,
    pub detail: String,
    pub busy: bool,
    pub recording: bool,
    pub error: bool,
    pub generation: u64,
}

impl Default for DictationSnapshot {
    fn default() -> Self {
        Self {
            phase: "Ready".into(),
            detail: "Focus the T3 composer, then hold your configured key.".into(),
            busy: false,
            recording: false,
            error: false,
            generation: 0,
        }
    }
}

struct TargetTask(tokio::task::JoinHandle<Result<TargetPermit>>);
impl Drop for TargetTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
impl TargetTask {
    fn abort(&self) {
        self.0.abort();
    }
}

struct Active {
    generation: u64,
    stop: watch::Sender<Option<CaptureStop>>,
    canceled: CancellationToken,
    permit: Arc<Mutex<Option<TargetPermit>>>,
    released_at: Option<std::time::Instant>,
}

pub struct Dictation {
    journal: Arc<Journal>,
    capture: Arc<dyn CaptureProvider>,
    insertion: Arc<dyn Insertion>,
    worker: PathBuf,
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    active: Mutex<Option<Active>>,
    state: watch::Sender<DictationSnapshot>,
    next_generation: AtomicU64,
    accepting: AtomicBool,
    maintaining: AtomicBool,
}

impl Dictation {
    pub fn new(
        journal: Arc<Journal>,
        insertion: Arc<dyn Insertion>,
        worker: PathBuf,
        settings_path: PathBuf,
        settings: Settings,
    ) -> Arc<Self> {
        Self::with_capture(
            journal,
            insertion,
            worker,
            settings_path,
            settings,
            Arc::new(NativeCaptureProvider),
        )
    }
    pub fn with_capture(
        journal: Arc<Journal>,
        insertion: Arc<dyn Insertion>,
        worker: PathBuf,
        settings_path: PathBuf,
        settings: Settings,
        capture: Arc<dyn CaptureProvider>,
    ) -> Arc<Self> {
        let (state, _) = watch::channel(DictationSnapshot::default());
        Arc::new(Self {
            journal,
            capture,
            insertion,
            worker,
            settings_path,
            settings: Mutex::new(settings),
            active: Mutex::new(None),
            state,
            next_generation: AtomicU64::new(1),
            accepting: AtomicBool::new(true),
            maintaining: AtomicBool::new(false),
        })
    }
    pub fn snapshot(&self) -> DictationSnapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<DictationSnapshot> {
        self.state.subscribe()
    }
    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
    pub fn journal(&self) -> Arc<Journal> {
        self.journal.clone()
    }
    pub fn save_settings(&self, settings: Settings) -> Result<()> {
        let active = self.active.lock().unwrap();
        ensure!(
            active.is_none() && !self.maintaining.load(Ordering::SeqCst),
            "Settings cannot change during dictation or cleanup."
        );
        settings.save(&self.settings_path)?;
        *self.settings.lock().unwrap() = settings;
        Ok(())
    }
    pub fn reconcile_journal(&self) -> Result<crate::journal::RecoveryReport> {
        {
            let active = self.active.lock().unwrap();
            ensure!(
                active.is_none() && !self.maintaining.load(Ordering::SeqCst),
                "Wait until dictation and cleanup finish before recovery."
            );
            ensure!(
                self.accepting.load(Ordering::SeqCst),
                "Dipper is shutting down."
            );
            self.maintaining.store(true, Ordering::SeqCst);
            self.publish(
                0,
                "Recovering",
                "Reconciling retained data. No automatic recognition or insertion.",
                true,
                false,
                false,
            );
        }
        let result = self.journal.recover();
        let _gate = self.active.lock().unwrap();
        self.maintaining.store(false, Ordering::SeqCst);
        match &result {
            Ok(report) if !report.unpersisted => self.publish(
                0,
                "Ready",
                "Journal recovery reconciled. No text was inserted.",
                false,
                false,
                false,
            ),
            Ok(_) => self.publish(
                0,
                "Needs attention",
                "Recovery is still unpersisted. Check storage, then retry.",
                false,
                false,
                true,
            ),
            Err(error) => {
                self.publish(0, "Needs attention", &error.to_string(), false, false, true)
            }
        }
        result
    }
    pub fn begin(self: &Arc<Self>) -> Result<()> {
        ensure!(
            self.accepting.load(Ordering::SeqCst),
            "Dipper is shutting down."
        );
        let mut active = self.active.lock().unwrap();
        ensure!(
            self.accepting.load(Ordering::SeqCst),
            "Dipper is shutting down."
        );
        // Repeated activation of the admitted Hold cannot create another recording.
        if active.is_some() || self.maintaining.load(Ordering::SeqCst) {
            let mut state = self.snapshot();
            state.detail = "Dipper is busy. The new Hold was ignored.".into();
            self.state.send_replace(state);
            return Ok(());
        }
        ensure!(
            !self.journal.recovery_report().unpersisted,
            "Journal recovery is unpersisted. Retry recovery before recording."
        );
        let settings = self.settings();
        if let Err(error) = settings.ready_for_recognition() {
            self.publish(0, "Setup needed", &error.to_string(), false, false, true);
            return Err(error);
        }
        ensure!(
            self.worker.is_file(),
            "Recognition worker is unavailable. Build the complete workspace."
        );
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst);
        let (stop, receiver) = watch::channel(None);
        let canceled = CancellationToken::new();
        let permit = Arc::new(Mutex::new(self.insertion.reserve().ok()));
        *active = Some(Active {
            generation,
            stop,
            canceled: canceled.clone(),
            permit: permit.clone(),
            released_at: None,
        });
        self.publish(
            generation,
            "Starting",
            "Opening the selected microphone. Wait for Recording before speaking.",
            true,
            false,
            false,
        );
        let this = self.clone();
        tokio::spawn(async move {
            let worker_slot = Arc::new(Mutex::new(None::<RecognitionJob>));
            let result = this
                .run(
                    generation,
                    settings,
                    receiver,
                    canceled,
                    permit,
                    worker_slot.clone(),
                )
                .await;
            let worker = worker_slot.lock().unwrap().take();
            if let Some(worker) = worker {
                worker.cancel();
                let outcome = this
                    .await_worker(generation, &worker, &CancellationToken::new())
                    .await;
                if result.is_err() {
                    let _ = this.record_attempt(outcome.identity.attempt_id, &outcome);
                }
            }
            match result {
                Ok(detail) => this.finish(generation, &detail, false),
                Err(error) => this.finish(generation, &format!("{error:#}"), true),
            }
        });
        Ok(())
    }
    pub fn release(&self) {
        if let Some(active) = self.active.lock().unwrap().as_mut() {
            active
                .released_at
                .get_or_insert_with(std::time::Instant::now);
            active.stop.send_if_modified(|reason| {
                if reason.is_none() {
                    *reason = Some(CaptureStop::Released);
                    true
                } else {
                    false
                }
            });
        }
    }
    pub fn cancel(&self) {
        self.interrupt(CaptureStop::Canceled);
    }
    pub fn session_lost(&self, detail: &str) {
        self.interrupt(CaptureStop::SessionLost);
        if !self.snapshot().busy {
            self.publish(0, "Desktop unavailable", detail, false, false, true);
        }
    }
    fn interrupt(&self, reason: CaptureStop) {
        if let Some(active) = self.active.lock().unwrap().as_ref() {
            if let Some(permit) = active.permit.lock().unwrap().as_ref() {
                permit.revoke();
            }
            active.canceled.cancel();
            active.stop.send_replace(Some(reason));
        }
    }
    pub async fn shutdown(&self) {
        self.accepting.store(false, Ordering::SeqCst);
        self.cancel();
        let mut state = self.subscribe();
        while state.borrow().busy {
            if state.changed().await.is_err() {
                break;
            }
        }
    }
    fn publish(
        &self,
        generation: u64,
        phase: &str,
        detail: &str,
        busy: bool,
        recording: bool,
        error: bool,
    ) {
        self.state.send_replace(DictationSnapshot {
            phase: phase.into(),
            detail: detail.into(),
            busy,
            recording,
            error,
            generation,
        });
    }
    fn finish(&self, generation: u64, detail: &str, error: bool) {
        let mut active = self.active.lock().unwrap();
        if active.as_ref().is_none_or(|a| a.generation != generation) {
            return;
        }
        if let Some(permit) = active
            .as_ref()
            .and_then(|a| a.permit.lock().unwrap().clone())
        {
            permit.revoke();
        }
        *active = None;
        self.publish(
            generation,
            if error { "Needs attention" } else { "Ready" },
            detail,
            false,
            false,
            error,
        );
    }
    async fn run(
        self: &Arc<Self>,
        generation: u64,
        settings: Settings,
        mut stop: watch::Receiver<Option<CaptureStop>>,
        canceled: CancellationToken,
        permit_slot: Arc<Mutex<Option<TargetPermit>>>,
        worker_slot: Arc<Mutex<Option<RecognitionJob>>>,
    ) -> Result<String> {
        let insertion = self.insertion.clone();
        let reserved = permit_slot.lock().unwrap().clone();
        let mut target_task = TargetTask(tokio::spawn(async move {
            let permit = reserved.context("Target unavailable at admission")?;
            insertion.arm(permit).await
        }));
        let config = WorkerConfig::large_v3(
            self.worker.clone(),
            settings.native_lib_dir.clone(),
            settings.model_path.clone(),
        );
        let provider = self.capture.clone();
        let input = tokio::task::spawn_blocking(move || {
            provider.inputs()?.resolve(settings.input_name.as_deref())
        })
        .await??;
        let journal = self.journal.clone();
        let capture_input = input.clone();
        let ticket =
            tokio::task::spawn_blocking(move || journal.begin_capture(&capture_input)).await??;
        let recording_id = ticket.recording_id;
        let attempt_id = self
            .journal
            .begin_attempt(recording_id, serde_json::to_value(&config.settings)?)?;
        let early_stop = stop.borrow().clone();
        if let Some(reason) = early_stop {
            let status = if matches!(reason, CaptureStop::Released) {
                "no_text"
            } else {
                "canceled"
            };
            let outcome = tokio::task::spawn_blocking(move || ticket.finish(reason)).await??;
            self.journal.finish_attempt(
                attempt_id,
                status,
                None,
                json!({"capture":outcome,"reason":"Hold ended before capture readiness"}),
            )?;
            return Ok(
                "Hold ended before capture readiness. No audio was recognized or inserted.".into(),
            );
        }
        let supervisor = RecognitionSupervisor {
            config,
            policy: SupervisorPolicy::default(),
        };
        let worker_result = supervisor
            .prepare(
                JobIdentity {
                    generation,
                    recording_id,
                    attempt_id,
                },
                self.journal.diagnostics_path(attempt_id),
            )
            .await;
        let (worker, worker_start_error) = match worker_result {
            Ok(job) => (Some(job), None),
            Err(error) => (None, Some(format!("{error:#}"))),
        };
        *worker_slot.lock().unwrap() = worker.clone();
        if canceled.is_cancelled()
            && let Some(worker) = &worker
        {
            worker.cancel();
        }
        let early_stop = stop.borrow().clone();
        if let Some(reason) = early_stop {
            if let Some(worker) = &worker {
                worker.cancel();
            }
            let outcome = tokio::task::spawn_blocking(move || ticket.finish(reason)).await??;
            self.journal.finish_attempt(
                attempt_id,
                "canceled",
                None,
                json!({"capture":outcome,"reason":"Hold ended before capture readiness"}),
            )?;
            return Ok(
                "Hold ended before capture readiness. Audio was preserved; no insertion.".into(),
            );
        }
        let provider = self.capture.clone();
        let capture = tokio::task::spawn_blocking(move || provider.start(ticket, &input)).await;
        let capture = match capture {
            Ok(Ok(capture)) => capture,
            other => {
                target_task.abort();
                if let Some(worker) = &worker {
                    worker.cancel();
                    let outcome = self
                        .await_worker(generation, worker, &CancellationToken::new())
                        .await;
                    self.record_attempt(attempt_id, &outcome)?;
                }
                return Err(match other {
                    Ok(Err(error)) => error,
                    Err(error) => error.into(),
                    _ => unreachable!(),
                });
            }
        };
        let mut target_done = false;
        let mut target_error = None;
        let mut timer = tokio::time::interval(Duration::from_millis(25));
        let reason = loop {
            if let Some(reason) = stop.borrow().clone() {
                break reason;
            }
            tokio::select! {
                changed = stop.changed() => { if changed.is_err() { break CaptureStop::Canceled; } }
                _ = canceled.cancelled() => { break stop.borrow().clone().unwrap_or(CaptureStop::Canceled); }
                result = &mut target_task.0, if !target_done => {
                    target_done = true;
                    target_error = settle_target_qualification(
                        &permit_slot, &canceled,
                        result.map_err(anyhow::Error::from).and_then(|result| result),
                    );
                }
                _ = timer.tick() => {
                    let progress = capture.progress();
                    if let Some(error) = progress.error { break CaptureStop::Failed(error); }
                    if progress.ready {
                        let failure = worker.as_ref().and_then(|worker| worker.subscribe().borrow().outcome.clone())
                            .and_then(|outcome| outcome.error).or_else(|| worker_start_error.clone());
                        let current = self.snapshot();
                        if !current.recording || current.error != failure.is_some() {
                            let detail = failure.as_ref().map(|error| format!("Recording continues; recognition failed: {error}. Release to retain audio."))
                                .unwrap_or_else(|| "Release the Hold key to finish recording. Use no other keys.".into());
                            self.publish(generation, "Recording", &detail, true, true, failure.is_some());
                        }
                    }
                }
            }
        };
        if !matches!(reason, CaptureStop::Released) {
            canceled.cancel();
            if let Some(permit) = permit_slot.lock().unwrap().as_ref() {
                permit.revoke();
            }
            if let Some(worker) = &worker {
                worker.cancel();
            }
        }
        self.publish(
            generation,
            "Finalizing",
            "Recording ended. Preserving and synchronizing audio.",
            true,
            false,
            false,
        );
        let capture_result = tokio::task::spawn_blocking(move || capture.stop(reason)).await?;
        let capture_result = match capture_result {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(worker) = &worker {
                    worker.cancel();
                    let outcome = self
                        .await_worker(generation, worker, &CancellationToken::new())
                        .await;
                    self.record_attempt(attempt_id, &outcome)?;
                }
                target_task.abort();
                return Err(error);
            }
        };
        let Some(worker) = worker else {
            target_task.abort();
            self.journal.finish_attempt(
                attempt_id,
                "failed",
                None,
                json!({"error":worker_start_error}),
            )?;
            anyhow::bail!(
                "Recognition worker could not start: {}. Available audio was retained.",
                worker_start_error.unwrap_or_default()
            );
        };
        if capture_result.state != "complete"
            || canceled.is_cancelled()
            || !capture_result.persisted
        {
            worker.cancel();
            let outcome = self
                .await_worker(generation, &worker, &CancellationToken::new())
                .await;
            self.record_attempt(attempt_id, &outcome)?;
            target_task.abort();
            if matches!(capture_result.state.as_str(), "failed" | "interrupted")
                || !capture_result.persisted
            {
                anyhow::bail!(
                    "Capture {}: {}. Available audio was retained; no insertion.",
                    capture_result.state,
                    capture_result
                        .error
                        .unwrap_or_else(|| "durable completion could not be acknowledged".into())
                );
            }
            return Ok(format!(
                "Recording {}. Available audio was retained; no insertion.",
                capture_result.state
            ));
        }
        let audio = capture_result
            .audio
            .context("complete capture has no available audio")?;
        worker
            .transcribe(AudioSource {
                path: audio.path,
                sample_rate: audio.sample_rate,
                channels: audio.channels,
                frames: audio.frames,
                data_offset: audio.data_offset,
            })
            .await?;
        let outcome = self.await_worker(generation, &worker, &canceled).await;
        self.record_attempt(attempt_id, &outcome)?;
        if !target_done {
            let result = tokio::time::timeout(Duration::from_secs(12), &mut target_task.0)
                .await
                .context("T3 admission validation timed out")
                .and_then(|result| result.map_err(anyhow::Error::from))
                .and_then(|result| result);
            target_error = settle_target_qualification(&permit_slot, &canceled, result);
            if target_error.is_some() {
                target_task.abort();
            }
        }
        if outcome.status != RecognitionStatus::Succeeded || canceled.is_cancelled() {
            return if outcome.status == RecognitionStatus::NoText {
                Ok("No text returned. Recording retained; clipboard unchanged.".into())
            } else if canceled.is_cancelled() {
                Ok("Canceled. Available audio and attempt were retained; no insertion.".into())
            } else {
                Err(anyhow::anyhow!(outcome.error.unwrap_or_else(|| {
                    "Recognition failed; audio retained.".into()
                })))
            };
        }
        let text = outcome
            .raw_transcript
            .context("successful recognition has no transcript")?;
        if text.trim().is_empty() {
            return Ok("No text returned. Clipboard unchanged.".into());
        }
        let permit = permit_slot.lock().unwrap().clone();
        let insertion_id = self
            .journal
            .begin_insertion(attempt_id, json!(permit.as_ref().map(|p| &p.identity)))?;
        self.publish(
            generation,
            "Inserting",
            "Checking the original T3 window and current composer.",
            true,
            false,
            false,
        );
        let result = if let Some(permit) = &permit {
            self.insertion.dispatch(permit, &text).await
        } else {
            InsertionOutcome {
                stage: DispatchStage::Unchanged,
                status: DeliveryStatus::Suppressed,
                detail: format!(
                    "{}. Use Copy in Journal.",
                    target_error
                        .as_deref()
                        .unwrap_or("Target unavailable or ambiguous")
                ),
            }
        };
        let delivery = match result.status {
            DeliveryStatus::Suppressed => "suppressed",
            DeliveryStatus::Unverified => "unverified",
            DeliveryStatus::Confirmed => "confirmed",
            DeliveryStatus::Failed => "failed",
        };
        let stage = match result.stage {
            DispatchStage::Unchanged => "unchanged",
            DispatchStage::ClipboardPublished => "clipboard_published",
            DispatchStage::PasteDispatched => "paste_dispatched",
        };
        let mut details = serde_json::to_value(&result)?;
        details.as_object_mut().unwrap().insert(
            "release_to_dispatch_ms".into(),
            json!(self.release_elapsed_ms(generation)),
        );
        self.journal
            .finish_insertion(insertion_id, delivery, stage, details)?;
        Ok(result.detail)
    }
    fn record_attempt(
        &self,
        id: uuid::Uuid,
        outcome: &crate::recognition::RecognitionOutcome,
    ) -> Result<()> {
        let state = match outcome.status {
            RecognitionStatus::Succeeded => "complete",
            RecognitionStatus::NoText => "no_text",
            RecognitionStatus::Failed => "failed",
            RecognitionStatus::Canceled => "canceled",
        };
        let mut details = serde_json::to_value(outcome)?;
        details.as_object_mut().unwrap().insert(
            "release_to_result_ms".into(),
            json!(self.release_elapsed_ms(outcome.identity.generation)),
        );
        self.journal
            .finish_attempt(id, state, outcome.raw_transcript.as_deref(), details)
    }
    fn release_elapsed_ms(&self, generation: u64) -> Option<u64> {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .filter(|active| active.generation == generation)
            .and_then(|active| {
                active
                    .released_at
                    .map(|time| time.elapsed().as_millis().min(u64::MAX as u128) as u64)
            })
    }
    async fn await_worker(
        &self,
        generation: u64,
        worker: &RecognitionJob,
        canceled: &CancellationToken,
    ) -> crate::recognition::RecognitionOutcome {
        let mut state = worker.subscribe();
        let mut cancellation_seen = false;
        loop {
            let snapshot = state.borrow().clone();
            if let Some(outcome) = snapshot.outcome {
                return outcome;
            }
            let (phase, detail) = match snapshot.phase {
                RecognitionPhase::Preparing => {
                    ("Preparing", "Recording ended. Finishing model preparation.")
                }
                RecognitionPhase::Ready | RecognitionPhase::Transcribing => (
                    "Transcribing",
                    "Recording ended. Recognizing the retained audio.",
                ),
                RecognitionPhase::CleaningUp | RecognitionPhase::Finished => {
                    ("Cleaning up", "Waiting for the recognition worker to exit.")
                }
            };
            self.publish(
                generation,
                phase,
                snapshot.cleanup_warning.as_deref().unwrap_or(detail),
                true,
                false,
                snapshot.cleanup_warning.is_some(),
            );
            tokio::select! {
                _ = canceled.cancelled(), if !cancellation_seen => { cancellation_seen = true; worker.cancel(); }
                changed = state.changed() => { if changed.is_err() { return worker.wait().await; } }
            }
        }
    }
}

// Both early and late completion must settle the same admission reservation.
fn settle_target_qualification(
    slot: &Mutex<Option<TargetPermit>>,
    canceled: &CancellationToken,
    result: Result<TargetPermit>,
) -> Option<String> {
    match result {
        Ok(permit) => {
            if canceled.is_cancelled() {
                permit.revoke();
            }
            *slot.lock().unwrap() = Some(permit);
            None
        }
        Err(error) => {
            if let Some(permit) = slot.lock().unwrap().take() {
                permit.revoke();
            }
            Some(error.to_string())
        }
    }
}
