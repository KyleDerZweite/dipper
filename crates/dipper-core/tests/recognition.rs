use dipper_core::{protocol::*, recognition::*};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, time::Duration};
use tempfile::TempDir;
use uuid::Uuid;

const PROCESS: &str = r#"#!/usr/bin/python3
import json, os, sys, time, threading
launch = json.loads(sys.argv[1])
mode = 'MODE'
identity = launch['identity']
version = launch['version']
print('worker-pid=' + str(os.getpid()), file=sys.stderr, flush=True)
if mode == 'parent_loss':
    with open(os.path.join(os.path.dirname(launch['settings']['model_path']), 'worker.pid'), 'w') as marker:
        marker.write(str(os.getpid()))
def emit(event):
    print(json.dumps({'version':version,'identity':identity,'event':event}), flush=True)
gpu={'backend':'Vulkan','name':'Vulkan1','description':'AMD Radeon RX 7700S','pci_id':'0000:03:00.0','gpu_index':1}
runtime={'engine_version':'whisper.cpp 1.9.5, source-declared','source_revision':launch['settings']['runtime_revision'],
 'model':{'path':launch['settings']['model_path'],'size_bytes':3,'modified_unix_ns':0,'sha256':'0'*64,'unavailable_reason':None},'native_artifacts':[]}
if mode == 'model_failure':
    emit({'type':'failed','error':'native model initialization failed','runtime':runtime})
    sys.exit(2)
if mode == 'wrong_gpu': gpu['backend']='CPU'
if mode == 'missing_hash': runtime['model']['sha256']=None
if mode == 'load_hang': time.sleep(30)
if mode == 'wrong_identity': identity['generation'] += 1
if mode == 'wrong_version': version = 2
if mode == 'malformed':
    print('not-json', flush=True)
    time.sleep(30)
if mode == 'oversized':
    print('x' * (8 * 1024 * 1024 + 1), flush=True)
    time.sleep(30)
if mode == 'started_first': emit({'type':'started'})
if mode == 'load_overlap': time.sleep(.1)
emit({'type':'ready','gpu':gpu,'preparation_ms':3,'runtime':runtime})
if mode == 'parent_loss':
    def control():
        while sys.stdin.buffer.readline(): pass
        os._exit(125)
    threading.Thread(target=control, daemon=True).start()
    time.sleep(30)
request=json.loads(sys.stdin.readline())
if mode == 'request_hang': time.sleep(30)
emit({'type':'started'})
if mode == 'inference_hang': time.sleep(30)
if mode == 'diagnostics':
    sys.stderr.write('a' * (140 * 1024))
    sys.stderr.flush()
text='' if mode == 'no_text' else 'Grüße aus T3.'
if mode == 'different_runtime': runtime['model']['sha256']='1'*64
result={'type':'result','raw_transcript':text,
 'provenance':{'source_frames':request['audio']['frames'],'model_frames':(request['audio']['frames']+2)//3,
  'sample_rate':16000,'channels':1,'resampler':'fixture','input_padding_frames':0,'removed_output_delay_frames':0,'runtime':runtime},
 'timings':{'preparation_ms':3,'input_preparation_ms':2,'inference_ms':1}}
emit(result)
if mode == 'result_cancel':
    open(launch['settings']['model_path'], 'w').write('result-sent')
    time.sleep(30)
if mode == 'duplicate': emit(result)
if mode == 'bad_exit': sys.exit(7)
if mode == 'exit_hang': time.sleep(30)
"#;

struct Fixture {
    directory: TempDir,
    supervisor: RecognitionSupervisor,
    identity: JobIdentity,
}

impl Fixture {
    fn new(mode: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("worker");
        fs::write(&executable, PROCESS.replace("MODE", mode)).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let config = WorkerConfig::large_v3(
            executable,
            directory.path().to_owned(),
            directory.path().join("model.bin"),
        );
        let policy = SupervisorPolicy {
            preparation_timeout: Duration::from_millis(600),
            recognition_base_timeout: Duration::from_millis(300),
            audio_duration_multiplier: 0.0,
            normal_exit_timeout: Duration::from_millis(150),
            cleanup_warning_timeout: Duration::from_millis(100),
        };
        Self {
            directory,
            supervisor: RecognitionSupervisor { config, policy },
            identity: JobIdentity {
                generation: 1,
                recording_id: Uuid::new_v4(),
                attempt_id: Uuid::new_v4(),
            },
        }
    }

    async fn start(&self) -> RecognitionJob {
        self.prepare(self.directory.path().join("diagnostic.log"))
            .await
    }

    async fn prepare(&self, diagnostic_path: PathBuf) -> RecognitionJob {
        // Concurrent test forks can briefly inherit a writer for a newly generated script.
        // Retry only that fixture-specific ETXTBSY; production spawn errors remain unchanged.
        for _ in 0..50 {
            match self
                .supervisor
                .prepare(self.identity.clone(), diagnostic_path.clone())
                .await
            {
                Ok(job) => return job,
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|io| io.raw_os_error() == Some(libc::ETXTBSY)) =>
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("fixture worker spawn failed: {error:#}"),
            }
        }
        panic!("fixture remained executable-busy");
    }

    fn audio(&self) -> AudioSource {
        AudioSource {
            path: self.directory.path().join("capture.rf64.wav"),
            sample_rate: 48_000,
            channels: 1,
            frames: 4800,
            data_offset: 92,
        }
    }

    async fn finish(&self, job: &RecognitionJob) -> RecognitionOutcome {
        let outcome = tokio::time::timeout(Duration::from_secs(4), job.wait())
            .await
            .expect("supervisor must settle");
        assert!(outcome.reaped);
        assert_eq!(outcome.identity, self.identity);
        if let Some(pid) = outcome.diagnostic_tail.lines().find_map(|line| {
            line.strip_prefix("worker-pid=")
                .and_then(|value| value.parse::<i32>().ok())
        }) {
            assert_eq!(
                unsafe { libc::kill(pid, 0) },
                -1,
                "worker must have disappeared before Finished"
            );
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        }
        outcome
    }
}

#[tokio::test]
async fn complete_result_requires_successful_exit_and_supports_release_during_loading() {
    let fixture = Fixture::new("load_overlap");
    let job = fixture.start().await;
    job.transcribe(fixture.audio()).await.unwrap();
    let outcome = fixture.finish(&job).await;
    assert_eq!(outcome.status, RecognitionStatus::Succeeded);
    assert_eq!(outcome.raw_transcript.as_deref(), Some("Grüße aus T3."));
    assert_eq!(outcome.provenance.unwrap().model_frames, 1600);
}

#[tokio::test]
async fn invalid_protocol_never_succeeds() {
    for mode in [
        "malformed",
        "wrong_identity",
        "wrong_version",
        "started_first",
        "duplicate",
        "oversized",
        "wrong_gpu",
        "missing_hash",
        "different_runtime",
    ] {
        let fixture = Fixture::new(mode);
        let job = fixture.start().await;
        let _ = job.transcribe(fixture.audio()).await;
        let outcome = fixture.finish(&job).await;
        assert_eq!(outcome.status, RecognitionStatus::Failed, "{mode}");
        assert!(outcome.error.is_some(), "{mode}");
    }
}

#[tokio::test]
async fn bad_exit_and_missing_exit_preserve_result_but_fail_attempt() {
    for mode in ["bad_exit", "exit_hang"] {
        let fixture = Fixture::new(mode);
        let job = fixture.start().await;
        job.transcribe(fixture.audio()).await.unwrap();
        let outcome = fixture.finish(&job).await;
        assert_eq!(outcome.status, RecognitionStatus::Failed, "{mode}");
        assert_eq!(outcome.raw_transcript.as_deref(), Some("Grüße aus T3."));
    }
}

#[tokio::test]
async fn failed_preparation_preserves_observed_artifact_provenance() {
    let fixture = Fixture::new("model_failure");
    let job = fixture.start().await;
    let result = fixture.finish(&job).await;
    assert_eq!(result.status, RecognitionStatus::Failed);
    assert_eq!(result.runtime.unwrap().model.sha256, Some("0".repeat(64)));
    assert!(result.raw_transcript.is_none());
}

#[tokio::test]
async fn preparation_and_request_to_started_and_inference_deadlines_all_reap() {
    for mode in ["load_hang", "request_hang", "inference_hang"] {
        let fixture = Fixture::new(mode);
        let job = fixture.start().await;
        if mode != "load_hang" {
            job.transcribe(fixture.audio()).await.unwrap();
        }
        let outcome = fixture.finish(&job).await;
        assert_eq!(outcome.status, RecognitionStatus::Failed, "{mode}");
        assert!(outcome.error.unwrap().contains("deadline"), "{mode}");
    }
}

#[tokio::test]
async fn cancel_loading_and_inference_reaps_without_accepting_text() {
    for mode in ["load_hang", "inference_hang"] {
        let fixture = Fixture::new(mode);
        let job = fixture.start().await;
        if mode == "inference_hang" {
            job.transcribe(fixture.audio()).await.unwrap();
            let mut state = job.subscribe();
            while state.borrow().phase != RecognitionPhase::Transcribing {
                state.changed().await.unwrap();
            }
        }
        job.cancel();
        let outcome = fixture.finish(&job).await;
        assert_eq!(outcome.status, RecognitionStatus::Canceled);
        assert!(outcome.raw_transcript.is_none());
        assert!(job.transcribe(fixture.audio()).await.is_err());
    }
}

#[tokio::test]
async fn cancel_after_buffered_result_does_not_accept_success() {
    let fixture = Fixture::new("result_cancel");
    let job = fixture.start().await;
    job.transcribe(fixture.audio()).await.unwrap();
    let marker = &fixture.supervisor.config.settings.model_path;
    tokio::time::timeout(Duration::from_secs(1), async {
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    job.cancel();
    let result = fixture.finish(&job).await;
    assert_eq!(result.status, RecognitionStatus::Canceled);
    assert_eq!(result.raw_transcript.as_deref(), Some("Grüße aus T3."));
}

#[tokio::test]
async fn no_text_is_explicit_and_full_diagnostics_survive_tail_rollover() {
    let empty = Fixture::new("no_text");
    let job = empty.start().await;
    job.transcribe(empty.audio()).await.unwrap();
    assert_eq!(empty.finish(&job).await.status, RecognitionStatus::NoText);
    let verbose = Fixture::new("diagnostics");
    let job = verbose.start().await;
    job.transcribe(verbose.audio()).await.unwrap();
    let result = verbose.finish(&job).await;
    assert_eq!(result.status, RecognitionStatus::Succeeded);
    assert!(result.diagnostics_truncated);
    assert!(
        result
            .diagnostic_tail
            .starts_with("[diagnostic tail incomplete]")
    );
    assert!(fs::metadata(result.diagnostic_path).unwrap().len() > DIAGNOSTIC_TAIL_LIMIT as u64);
    assert!(result.diagnostic_error.is_none());
}

#[tokio::test]
async fn diagnostic_failure_is_visible_while_transport_still_settles() {
    let fixture = Fixture::new("normal");
    let job = fixture
        .prepare(fixture.directory.path().join("missing/log"))
        .await;
    job.transcribe(fixture.audio()).await.unwrap();
    let result = fixture.finish(&job).await;
    assert_eq!(result.status, RecognitionStatus::Succeeded);
    assert!(result.diagnostic_error.is_some());
}

#[tokio::test]
async fn second_request_fails_instead_of_queuing_another_job() {
    let fixture = Fixture::new("request_hang");
    let job = fixture.start().await;
    job.transcribe(fixture.audio()).await.unwrap();
    let _ = job.transcribe(fixture.audio()).await;
    let result = fixture.finish(&job).await;
    assert_eq!(result.status, RecognitionStatus::Failed);
    assert!(result.error.unwrap().contains("second"));
}

#[test]
fn spawn_failure_is_reported_without_a_job_handle() {
    let fixture = Fixture::new("normal");
    fs::remove_file(&fixture.supervisor.config.executable).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert!(
        runtime
            .block_on(fixture.supervisor.prepare(
                fixture.identity.clone(),
                fixture.directory.path().join("log")
            ))
            .is_err()
    );
}

// Invoked as a separate parent process to test ownership loss, not an in-memory substitute.
#[test]
#[ignore]
fn parent_process_helper() {
    let root = PathBuf::from(std::env::var_os("DIPPER_PARENT_LOSS_FIXTURE").unwrap());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let supervisor = RecognitionSupervisor {
            config: WorkerConfig::large_v3(
                root.join("worker"),
                root.clone(),
                root.join("model.bin"),
            ),
            policy: SupervisorPolicy::default(),
        };
        let job = supervisor
            .prepare(
                JobIdentity {
                    generation: 1,
                    recording_id: Uuid::new_v4(),
                    attempt_id: Uuid::new_v4(),
                },
                root.join("parent.log"),
            )
            .await
            .unwrap();
        let mut state = job.subscribe();
        while state.borrow().phase != RecognitionPhase::Ready {
            state.changed().await.unwrap();
        }
        std::process::exit(0);
    });
}

#[test]
fn abruptly_lost_parent_closes_control_pipe() {
    let fixture = Fixture::new("parent_loss");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "parent_process_helper",
            "--nocapture",
        ])
        .env("DIPPER_PARENT_LOSS_FIXTURE", fixture.directory.path())
        .status()
        .unwrap();
    assert!(status.success());
    // The fixture writes this before Ready. Abrupt parent exit deliberately does
    // not wait for the independent diagnostics reader to flush its log.
    let pid = fs::read_to_string(fixture.directory.path().join("worker.pid"))
        .unwrap()
        .parse::<i32>()
        .unwrap();
    for _ in 0..100 {
        let state = fs::read_to_string(format!("/proc/{pid}/stat"));
        if state.is_err() || state.unwrap().split_whitespace().nth(2) == Some("Z") {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("worker kept running after parent EOF");
}
