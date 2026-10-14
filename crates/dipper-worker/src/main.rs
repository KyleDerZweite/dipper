mod native;

use anyhow::{Context, Result, ensure};
use dipper_core::protocol::*;
use dipper_worker::artifacts;
use dipper_worker::prepare_model_input;
use native::NativeRecognition;
use std::{
    io::{BufRead, BufReader, Write},
    sync::mpsc,
    time::Instant,
};

fn emit(identity: &JobIdentity, event: WorkerEvent) -> Result<()> {
    let encoded = serde_json::to_vec(&EventFrame::new(identity.clone(), event))?;
    ensure!(encoded.len() <= EVENT_LIMIT, "event exceeds protocol limit");
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&encoded)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

fn read_request(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            ensure!(frame.is_empty(), "unterminated request frame");
            return Ok(None);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(available.len());
        ensure!(
            frame
                .len()
                .checked_add(count)
                .is_some_and(|length| length <= REQUEST_LIMIT),
            "request exceeds protocol limit"
        );
        frame.extend_from_slice(&available[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(Some(frame));
        }
    }
}

fn run(launch: LaunchRequest, observed_runtime: &mut Option<RuntimeProvenance>) -> Result<()> {
    ensure!(
        launch.version == PROTOCOL_VERSION,
        "unknown launch protocol version"
    );
    launch.settings.validate()?;
    let (sender, receiver) = mpsc::sync_channel(1);
    let identity = launch.identity.clone();
    // This thread owns stdin from before native initialization through native inference.
    // EOF terminates the entire process without waiting for an uncancellable native loader.
    std::thread::Builder::new()
        .name("worker-control".into())
        .spawn(move || {
            let mut reader = BufReader::new(std::io::stdin());
            let mut received = false;
            loop {
                let request = read_request(&mut reader).and_then(|bytes| {
                    let Some(bytes) = bytes else {
                        std::process::exit(125);
                    };
                    ensure!(!received, "second transcription request");
                    let request: RequestFrame = serde_json::from_slice(&bytes)
                        .context("malformed transcription request")?;
                    request.validate(&identity)?;
                    Ok(request)
                });
                match request {
                    Ok(request) => {
                        received = true;
                        if sender.send(request).is_err() {
                            std::process::exit(125);
                        }
                    }
                    Err(error) => {
                        let _ = emit(
                            &identity,
                            WorkerEvent::Failed {
                                error: error.to_string(),
                                runtime: None,
                            },
                        );
                        std::process::exit(2);
                    }
                }
            }
        })?;
    let preparation = Instant::now();
    eprintln!("Preparing recognition artifact fingerprints");
    let (runtime, artifact_guards) = artifacts::fingerprint_runtime(&launch.settings.model_path)?;
    *observed_runtime = Some(runtime.clone());
    eprintln!("Artifact fingerprints complete; initializing Whisper/Vulkan");
    let (mut recognition, gpu) = NativeRecognition::prepare(&launch.settings)?;
    for guard in &artifact_guards {
        guard.validate()?;
    }
    let preparation_ms = preparation
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    emit(
        &launch.identity,
        WorkerEvent::Ready {
            gpu,
            preparation_ms,
            runtime: runtime.clone(),
        },
    )?;
    let request = receiver
        .recv()
        .context("worker control reader disappeared")?;
    emit(&launch.identity, WorkerEvent::Started)?;
    let input_start = Instant::now();
    let mut input = prepare_model_input(&request.audio)?;
    input.provenance.runtime = Some(runtime);
    let input_preparation_ms = input_start
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let inference = Instant::now();
    let raw_transcript = recognition.recognize(&input.samples, &launch.settings)?;
    let inference_ms = inference
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    emit(
        &launch.identity,
        WorkerEvent::Result {
            raw_transcript,
            provenance: input.provenance,
            timings: RecognitionTimings {
                preparation_ms,
                input_preparation_ms,
                inference_ms,
            },
        },
    )?;
    // Native context destruction happens before successful process exit.
    drop(recognition);
    Ok(())
}

fn main() {
    let launch = std::env::args()
        .nth(1)
        .context("missing worker launch frame")
        .and_then(|encoded| {
            ensure!(
                encoded.len() <= REQUEST_LIMIT,
                "launch frame exceeds protocol limit"
            );
            serde_json::from_str::<LaunchRequest>(&encoded).context("invalid worker launch frame")
        });
    let launch = match launch {
        Ok(launch) => launch,
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(2);
        }
    };
    let identity = launch.identity.clone();
    let mut observed_runtime = None;
    if let Err(error) = run(launch, &mut observed_runtime) {
        eprintln!("{error:#}");
        let _ = emit(
            &identity,
            WorkerEvent::Failed {
                error: error.to_string(),
                runtime: observed_runtime,
            },
        );
        std::process::exit(2);
    }
}
