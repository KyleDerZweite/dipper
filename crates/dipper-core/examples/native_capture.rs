//! Explicit opt-in native microphone check. Stores audio privately at the supplied path.
use anyhow::{Context, Result, ensure};
use dipper_core::{
    capture::{Capture, CaptureSession, CaptureStop},
    journal::Journal,
};
fn main() -> Result<()> {
    let directory = std::env::args_os()
        .nth(1)
        .context("provide a private disposable store directory")?;
    let directory = std::path::PathBuf::from(directory);
    let journal = Journal::open(directory.join("data"), directory.join("state"))?;
    let input = Capture::inputs()?.resolve(None)?;
    let ticket = journal.begin_capture(&input)?;
    let capture = CaptureSession::start(ticket, &input)?;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(3) {
        if capture.progress().error.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let outcome = capture.stop(CaptureStop::Released)?;
    println!(
        "{}",
        serde_json::json!({"state":outcome.state,"available_frames":outcome.available_frames,"synced_frames":outcome.synced_frames,"accepted_frames":outcome.accepted_frames,"persisted":outcome.persisted,"error":outcome.error})
    );
    ensure!(
        outcome.state == "complete" && outcome.persisted && outcome.available_frames > 0,
        "native capture did not complete"
    );
    ensure!(
        outcome.available_frames == outcome.accepted_frames
            && outcome.synced_frames == outcome.available_frames,
        "native sample counts disagree"
    );
    Ok(())
}
