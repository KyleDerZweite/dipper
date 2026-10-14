//! Owns native worker lifetime, bounded transport, deadlines and durable diagnostics.
//! A terminal snapshot is published only after the child is reaped and readers joined.
use crate::protocol::*;
use anyhow::{Context, Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, path::PathBuf, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    process::{Child, Command},
    sync::{mpsc, watch},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug)]
pub struct WorkerConfig {
    pub executable: PathBuf,
    pub native_lib_dir: PathBuf,
    pub settings: RecognitionSettings,
}

impl WorkerConfig {
    pub fn large_v3(executable: PathBuf, native_lib_dir: PathBuf, model_path: PathBuf) -> Self {
        Self::for_model(
            executable,
            native_lib_dir,
            RecognitionModel::LargeV3,
            model_path,
        )
    }

    pub fn for_model(
        executable: PathBuf,
        native_lib_dir: PathBuf,
        model_profile: RecognitionModel,
        model_path: PathBuf,
    ) -> Self {
        Self {
            executable,
            native_lib_dir,
            settings: RecognitionSettings::for_model(model_profile, model_path),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SupervisorPolicy {
    pub preparation_timeout: Duration,
    pub recognition_base_timeout: Duration,
    pub audio_duration_multiplier: f64,
    pub normal_exit_timeout: Duration,
    pub cleanup_warning_timeout: Duration,
}

impl Default for SupervisorPolicy {
    fn default() -> Self {
        Self {
            preparation_timeout: Duration::from_secs(60),
            recognition_base_timeout: Duration::from_secs(60),
            audio_duration_multiplier: 2.0,
            normal_exit_timeout: Duration::from_secs(5),
            cleanup_warning_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionPhase {
    Preparing,
    Ready,
    Transcribing,
    CleaningUp,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecognitionStatus {
    Succeeded,
    NoText,
    Failed,
    Canceled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecognitionOutcome {
    pub status: RecognitionStatus,
    pub identity: JobIdentity,
    pub raw_transcript: Option<String>,
    pub gpu: Option<GpuIdentity>,
    pub provenance: Option<ModelInputProvenance>,
    pub timings: Option<RecognitionTimings>,
    pub runtime: Option<RuntimeProvenance>,
    pub error: Option<String>,
    pub diagnostic_path: PathBuf,
    pub diagnostic_tail: String,
    pub diagnostics_truncated: bool,
    pub diagnostic_error: Option<String>,
    pub reaped: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecognitionSnapshot {
    pub phase: RecognitionPhase,
    pub cleanup_warning: Option<String>,
    pub outcome: Option<RecognitionOutcome>,
}

#[derive(Clone)]
pub struct RecognitionJob {
    commands: mpsc::Sender<AudioSource>,
    cancellation: CancellationToken,
    state: watch::Receiver<RecognitionSnapshot>,
}

impl RecognitionJob {
    /// Sends only a finalized immutable artifact. This does not wait for inference.
    pub async fn transcribe(&self, audio: AudioSource) -> Result<()> {
        ensure!(!self.cancellation.is_cancelled(), "recognition canceled");
        self.commands
            .send(audio)
            .await
            .map_err(|_| anyhow!("recognition worker already settled"))
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn subscribe(&self) -> watch::Receiver<RecognitionSnapshot> {
        self.state.clone()
    }

    pub async fn wait(&self) -> RecognitionOutcome {
        let mut state = self.state.clone();
        loop {
            if let Some(outcome) = state.borrow().outcome.clone() {
                return outcome;
            }
            state
                .changed()
                .await
                .expect("worker supervisor must publish a terminal outcome");
        }
    }
}

#[derive(Clone)]
pub struct RecognitionSupervisor {
    pub config: WorkerConfig,
    pub policy: SupervisorPolicy,
}

impl RecognitionSupervisor {
    pub async fn prepare(
        &self,
        identity: JobIdentity,
        diagnostic_path: PathBuf,
    ) -> Result<RecognitionJob> {
        self.config.settings.validate()?;
        ensure!(
            self.config.executable.is_absolute(),
            "worker executable must be absolute"
        );
        let launch = LaunchRequest {
            version: PROTOCOL_VERSION,
            identity: identity.clone(),
            settings: self.config.settings.clone(),
        };
        let encoded = serde_json::to_string(&launch)?;
        ensure!(
            encoded.len() <= REQUEST_LIMIT,
            "worker launch exceeds request limit"
        );
        let mut child = Command::new(&self.config.executable)
            .arg(encoded)
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("APPDIR")
            .env_remove("APPIMAGE")
            .env("LD_LIBRARY_PATH", &self.config.native_lib_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .context("spawn recognition worker")?;
        let stdin = child.stdin.take().context("worker stdin missing")?;
        let stdout = child.stdout.take().context("worker stdout missing")?;
        let stderr = child.stderr.take().context("worker stderr missing")?;
        let (commands, command_rx) = mpsc::channel(1);
        let cancellation = CancellationToken::new();
        let initial = RecognitionSnapshot {
            phase: RecognitionPhase::Preparing,
            cleanup_warning: None,
            outcome: None,
        };
        let (state_tx, state) = watch::channel(initial);
        let (events_tx, events_rx) = mpsc::channel(8);
        let stdout_task = tokio::spawn(read_events(stdout, events_tx));
        let log_path = diagnostic_path.clone();
        let stderr_task = tokio::spawn(async move { drain_diagnostics(stderr, log_path).await });
        let policy = self.policy.clone();
        let token = cancellation.clone();
        tokio::spawn(async move {
            supervise(
                child,
                stdin,
                command_rx,
                events_rx,
                stdout_task,
                stderr_task,
                identity,
                diagnostic_path,
                policy,
                token,
                state_tx,
            )
            .await;
        });
        Ok(RecognitionJob {
            commands,
            cancellation,
            state,
        })
    }
}

/// Read one newline-delimited frame without ever buffering beyond the chosen limit.
pub async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            ensure!(frame.is_empty(), "unterminated protocol frame");
            return Ok(None);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(available.len());
        ensure!(
            frame
                .len()
                .checked_add(count)
                .is_some_and(|length| length <= limit),
            "protocol frame exceeds limit"
        );
        frame.extend_from_slice(&available[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

async fn read_events(
    stdout: tokio::process::ChildStdout,
    sender: mpsc::Sender<Result<EventFrame>>,
) {
    let mut reader = tokio::io::BufReader::new(stdout);
    loop {
        let event = match read_frame(&mut reader, EVENT_LIMIT).await {
            Ok(Some(bytes)) => serde_json::from_slice(&bytes).context("malformed worker event"),
            Ok(None) => break,
            Err(error) => {
                let _ = sender.send(Err(error)).await;
                break;
            }
        };
        let failed = event.is_err();
        if sender.send(event).await.is_err() || failed {
            break;
        }
    }
}

#[derive(Default)]
struct DiagnosticReport {
    tail: String,
    truncated: bool,
    error: Option<String>,
}

async fn drain_diagnostics(
    mut stderr: tokio::process::ChildStderr,
    path: PathBuf,
) -> DiagnosticReport {
    use std::os::unix::fs::OpenOptionsExt;
    let opened = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path);
    let mut error = opened
        .as_ref()
        .err()
        .map(|e| format!("create diagnostic file: {e}"));
    let mut file = opened.ok().map(tokio::fs::File::from_std);
    let mut tail = VecDeque::with_capacity(DIAGNOSTIC_TAIL_LIMIT);
    let mut truncated = false;
    let mut bytes = [0_u8; 8192];
    loop {
        let count = match stderr.read(&mut bytes).await {
            Ok(0) => break,
            Ok(count) => count,
            Err(e) => {
                error.get_or_insert_with(|| format!("read worker diagnostics: {e}"));
                break;
            }
        };
        for byte in &bytes[..count] {
            if tail.len() == DIAGNOSTIC_TAIL_LIMIT {
                tail.pop_front();
                truncated = true;
            }
            tail.push_back(*byte);
        }
        if let Some(output) = file.as_mut()
            && let Err(e) = output.write_all(&bytes[..count]).await
        {
            error.get_or_insert_with(|| format!("write diagnostics: {e}"));
            file = None;
        }
    }
    if let Some(output) = file.as_mut()
        && let Err(e) = output.sync_all().await
    {
        error.get_or_insert_with(|| format!("sync diagnostics: {e}"));
    }
    let prefix = if truncated {
        "[diagnostic tail incomplete]\n"
    } else {
        ""
    };
    DiagnosticReport {
        tail: format!(
            "{prefix}{}",
            String::from_utf8_lossy(tail.make_contiguous())
        ),
        truncated,
        error,
    }
}

async fn deadline(value: Option<Instant>) {
    match value {
        Some(value) => tokio::time::sleep_until(value).await,
        None => std::future::pending().await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn supervise(
    mut child: Child,
    mut stdin: tokio::process::ChildStdin,
    mut commands: mpsc::Receiver<AudioSource>,
    mut events: mpsc::Receiver<Result<EventFrame>>,
    stdout_task: tokio::task::JoinHandle<()>,
    stderr_task: tokio::task::JoinHandle<DiagnosticReport>,
    identity: JobIdentity,
    diagnostic_path: PathBuf,
    policy: SupervisorPolicy,
    cancellation: CancellationToken,
    state: watch::Sender<RecognitionSnapshot>,
) {
    let mut preparation_deadline = Some(Instant::now() + policy.preparation_timeout);
    let mut recognition_deadline = None;
    let mut exit_deadline = None;
    let mut cleanup_deadline = None;
    let mut gpu = None;
    let mut runtime = None;
    let mut ready = false;
    let mut started = false;
    let mut request_sent = false;
    let mut source_frames = None;
    let mut terminal = None;
    let mut failure = None;
    let mut canceled = false;
    let mut killed = false;
    let mut exited = None;
    let mut output_closed = false;
    let mut commands_closed = false;
    let mut snapshot = RecognitionSnapshot {
        phase: RecognitionPhase::Preparing,
        cleanup_warning: None,
        outcome: None,
    };
    loop {
        if (failure.is_some() || canceled) && !killed && exited.is_none() {
            snapshot.phase = RecognitionPhase::CleaningUp;
            state.send_replace(snapshot.clone());
            if let Err(e) = child.start_kill() {
                failure.get_or_insert_with(|| format!("terminate worker: {e}"));
            }
            killed = true;
            cleanup_deadline = Some(Instant::now() + policy.cleanup_warning_timeout);
            preparation_deadline = None;
            recognition_deadline = None;
            exit_deadline = None;
        }
        if exited.is_some() && output_closed {
            break;
        }
        tokio::select! {
            biased;
            _ = cancellation.cancelled(), if !canceled => { canceled = true; }
            command = commands.recv(), if !commands_closed && exited.is_none() && !killed => {
                match command {
                    None => { commands_closed = true; canceled = true; }
                    Some(audio) => {
                        if request_sent { failure.get_or_insert_with(|| "second transcription request".into()); continue; }
                        let request = RequestFrame { version: PROTOCOL_VERSION, identity: identity.clone(), audio };
                        let encoded = request.validate(&identity).and_then(|_| Ok(serde_json::to_vec(&request)?));
                        match encoded {
                            Err(e) => { failure.get_or_insert_with(|| e.to_string()); }
                            Ok(mut encoded) => {
                                if encoded.len() > REQUEST_LIMIT { failure.get_or_insert_with(|| "request frame exceeds limit".into()); continue; }
                                let audio_seconds = request.audio.frames as f64 / 48_000.0;
                                let timeout = Duration::try_from_secs_f64(audio_seconds * policy.audio_duration_multiplier)
                                    .ok().and_then(|duration| duration.checked_add(policy.recognition_base_timeout));
                                let Some(timeout) = timeout else { failure.get_or_insert_with(|| "invalid recognition timeout".into()); continue; };
                                recognition_deadline = Instant::now().checked_add(timeout);
                                if recognition_deadline.is_none() { failure.get_or_insert_with(|| "recognition deadline overflow".into()); continue; }
                                encoded.push(b'\n');
                                source_frames = Some(request.audio.frames);
                                // A request is bounded below pipe capacity on ordinary systems, but IO still needs a deadline/cancel guard.
                                let write = tokio::select! {
                                    result = stdin.write_all(&encoded) => result.map_err(anyhow::Error::from),
                                    _ = cancellation.cancelled() => { canceled = true; Err(anyhow!("recognition canceled")) },
                                    _ = deadline(recognition_deadline) => Err(anyhow!("recognition request deadline exceeded")),
                                    _ = deadline(preparation_deadline) => Err(anyhow!("preparation deadline exceeded")),
                                };
                                if let Err(e) = write { failure.get_or_insert_with(|| e.to_string()); }
                                request_sent = true;
                            }
                        }
                    }
                }
            }
            event = events.recv(), if !output_closed => {
                match event {
                    None => { output_closed = true; if terminal.is_none() && !canceled { failure.get_or_insert_with(|| "worker output ended without terminal event".into()); } }
                    Some(Err(e)) => { failure.get_or_insert_with(|| e.to_string()); }
                    Some(Ok(frame)) => {
                        if frame.version != PROTOCOL_VERSION || frame.identity != identity { failure.get_or_insert_with(|| "worker event version or identity mismatch".into()); continue; }
                        if terminal.is_some() { failure.get_or_insert_with(|| "event after terminal result".into()); continue; }
                        match frame.event {
                            WorkerEvent::Ready { gpu: actual_gpu, runtime: actual_runtime, .. } if !ready && !started => {
                                if actual_gpu.backend != "Vulkan" || !actual_gpu.description.contains("RX 7700S") {
                                    failure.get_or_insert_with(|| "required RX 7700S Vulkan identity not confirmed".into()); continue;
                                }
                                if actual_runtime.source_revision != WHISPER_REVISION || !actual_runtime.model.sha256.as_ref().is_some_and(|hash|
                                    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())) {
                                    failure.get_or_insert_with(|| "pinned runtime or model hash provenance missing".into()); continue;
                                }
                                ready = true; gpu = Some(actual_gpu); runtime = Some(actual_runtime); preparation_deadline = None;
                                if !killed { snapshot.phase = RecognitionPhase::Ready; state.send_replace(snapshot.clone()); }
                            }
                            WorkerEvent::Started if ready && request_sent && !started => {
                                started = true;
                                if !killed { snapshot.phase = RecognitionPhase::Transcribing; state.send_replace(snapshot.clone()); }
                            }
                            result @ WorkerEvent::Result { .. } if ready && started && request_sent => {
                                if let WorkerEvent::Result { raw_transcript, provenance, .. } = &result {
                                    if raw_transcript.len() > TRANSCRIPT_LIMIT { failure.get_or_insert_with(|| "transcript exceeds limit".into()); }
                                    if Some(provenance.source_frames) != source_frames || provenance.model_frames != provenance.source_frames.div_ceil(3)
                                        || provenance.sample_rate != 16_000 || provenance.channels != 1 {
                                        failure.get_or_insert_with(|| "model-input provenance does not match finalized source".into());
                                    }
                                    if provenance.runtime != runtime { failure.get_or_insert_with(|| "runtime provenance differs between preparation and result".into()); }
                                }
                                terminal = Some(result); recognition_deadline = None;
                                exit_deadline = Some(Instant::now() + policy.normal_exit_timeout);
                            }
                            failed @ WorkerEvent::Failed { .. } => {
                                if let WorkerEvent::Failed { error, runtime: failed_runtime } = &failed {
                                    failure.get_or_insert_with(|| error.clone());
                                    if runtime.is_none() { runtime = failed_runtime.clone(); }
                                }
                                terminal = Some(failed); recognition_deadline = None; preparation_deadline = None;
                                exit_deadline = Some(Instant::now() + policy.normal_exit_timeout);
                            }
                            _ => { failure.get_or_insert_with(|| "invalid worker event order".into()); }
                        }
                    }
                }
            }
            result = child.wait(), if exited.is_none() => {
                match result {
                    Ok(status) => { if !status.success() && !canceled { failure.get_or_insert_with(|| format!("worker exited unsuccessfully: {status}")); } exited = Some(status); cleanup_deadline = None; }
                    Err(e) => {
                        failure.get_or_insert_with(|| format!("reap worker: {e}"));
                        snapshot.cleanup_warning = Some("worker reaping not confirmed".into());
                        state.send_replace(snapshot.clone());
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            _ = deadline(preparation_deadline) => { failure.get_or_insert_with(|| "preparation deadline exceeded".into()); }
            _ = deadline(recognition_deadline) => { failure.get_or_insert_with(|| "recognition deadline exceeded".into()); }
            _ = deadline(exit_deadline), if exited.is_none() => { failure.get_or_insert_with(|| "worker did not exit after terminal event".into()); }
            _ = deadline(cleanup_deadline) => {
                snapshot.cleanup_warning = Some("worker cleanup unconfirmed; still waiting for reaping".into());
                state.send_replace(snapshot.clone()); cleanup_deadline = None;
            }
        }
    }
    // Keep stdin owned until reaping to distinguish normal completion from parent loss.
    drop(stdin);
    if let Err(e) = stdout_task.await {
        failure.get_or_insert_with(|| format!("stdout reader task failed: {e}"));
    }
    let diagnostic = stderr_task.await.unwrap_or_else(|e| DiagnosticReport {
        error: Some(format!("stderr reader task failed: {e}")),
        ..Default::default()
    });
    let mut raw_transcript = None;
    let mut provenance = None;
    let mut timings = None;
    if let Some(WorkerEvent::Result {
        raw_transcript: text,
        provenance: input,
        timings: measured,
    }) = terminal
    {
        raw_transcript = Some(text);
        provenance = Some(input);
        timings = Some(measured);
    }
    canceled |= cancellation.is_cancelled();
    let status = if canceled {
        RecognitionStatus::Canceled
    } else if failure.is_some() || raw_transcript.is_none() {
        RecognitionStatus::Failed
    } else if raw_transcript
        .as_ref()
        .is_some_and(|text| text.trim().is_empty())
    {
        RecognitionStatus::NoText
    } else {
        RecognitionStatus::Succeeded
    };
    let outcome = RecognitionOutcome {
        status,
        identity,
        raw_transcript,
        gpu,
        provenance,
        timings,
        runtime,
        error: if canceled {
            Some("recognition canceled".into())
        } else {
            failure
        },
        diagnostic_path,
        diagnostic_tail: diagnostic.tail,
        diagnostics_truncated: diagnostic.truncated,
        diagnostic_error: diagnostic.error,
        reaped: true,
    };
    snapshot.phase = RecognitionPhase::Finished;
    snapshot.outcome = Some(outcome);
    state.send_replace(snapshot);
}
