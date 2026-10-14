//! Explicit opt-in native recognition check against a private existing Journal recording.
use anyhow::{Context, Result, ensure};
use dipper_core::{
    journal::Journal,
    protocol::{AudioSource, JobIdentity},
    recognition::{RecognitionStatus, RecognitionSupervisor, SupervisorPolicy, WorkerConfig},
};
#[tokio::main]
async fn main() -> Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .context("provide a private Journal store directory")?;
    let directory = std::path::PathBuf::from(directory);
    let journal = Journal::open(directory.join("data"), directory.join("state"))?;
    let recording = journal
        .recent(1)?
        .into_iter()
        .next()
        .context("no retained recording")?;
    let audio = journal.open_audio(recording.recording_id)?;
    let model = std::env::var_os("DIPPER_MODEL_PATH").context("set DIPPER_MODEL_PATH")?;
    let native = std::path::PathBuf::from(
        std::env::var_os("DIPPER_WHISPER_DIR").context("set DIPPER_WHISPER_DIR")?,
    )
    .join("build/bin");
    let worker = std::env::current_exe()?
        .parent()
        .and_then(|path| path.parent())
        .context("example executable directory")?
        .join("dipper-worker");
    let config = WorkerConfig::large_v3(worker, native, model.into());
    let attempt = journal.begin_attempt(
        recording.recording_id,
        serde_json::to_value(&config.settings)?,
    )?;
    let job = RecognitionSupervisor {
        config,
        policy: SupervisorPolicy::default(),
    }
    .prepare(
        JobIdentity {
            generation: 1,
            recording_id: recording.recording_id,
            attempt_id: attempt,
        },
        journal.diagnostics_path(attempt),
    )
    .await?;
    job.transcribe(AudioSource {
        path: audio.path,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        frames: audio.frames,
        data_offset: audio.data_offset,
    })
    .await?;
    let outcome = job.wait().await;
    let state = match outcome.status {
        RecognitionStatus::Succeeded => "complete",
        RecognitionStatus::NoText => "no_text",
        RecognitionStatus::Failed => "failed",
        RecognitionStatus::Canceled => "canceled",
    };
    journal.finish_attempt(
        attempt,
        state,
        outcome.raw_transcript.as_deref(),
        serde_json::to_value(&outcome)?,
    )?;
    println!(
        "{}",
        serde_json::json!({"status":state,"reaped":outcome.reaped,"gpu":outcome.gpu,"timings":outcome.timings,"model_hash_recorded":outcome.runtime.as_ref().and_then(|runtime| runtime.model.sha256.as_ref()).is_some(),"transcript_bytes":outcome.raw_transcript.as_ref().map(String::len),"error":outcome.error})
    );
    ensure!(
        outcome.reaped
            && matches!(
                outcome.status,
                RecognitionStatus::Succeeded | RecognitionStatus::NoText
            ),
        "native recognition failed"
    );
    Ok(())
}
