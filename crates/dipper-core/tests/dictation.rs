//! Public raw-chain lifecycle checks with durable Journal/audio and real fixture processes.
//! These adapters exercise coordinator races without opening a microphone or loading ASR.
use anyhow::{Result, ensure};
use async_trait::async_trait;
use dipper_core::{
    audio::read_f32,
    capture::{
        CaptureProgress, CaptureProvider, CaptureRun, CaptureStop, InputCatalog, InputDevice,
    },
    dictation::Dictation,
    journal::{CaptureOutcome, CaptureTicket, Journal, RecordingSnapshot},
    protocol::RecognitionModel,
    settings::Settings,
    target::{
        DeliveryStatus, DispatchStage, Insertion, InsertionOutcome, TargetIdentity, TargetPermit,
    },
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::Semaphore;

const SAMPLES: usize = 4800;
const TEXT: &str = "Grüße aus T3.";
const PROCESS: &str = r#"#!/usr/bin/python3
import json, os, sys, time
launch=json.loads(sys.argv[1])
root=os.path.dirname(launch['settings']['model_path'])
mode='MODE'
def marker(name, content=''):
    temporary=os.path.join(root,name+'.tmp')
    with open(temporary,'w') as output: output.write(content)
    os.replace(temporary,os.path.join(root,name))
def emit(event):
    print(json.dumps({'version':launch['version'],'identity':launch['identity'],'event':event}),flush=True)
marker('pid',str(os.getpid()))
marker('settings',json.dumps(launch['settings']))
print('worker-pid='+str(os.getpid()),file=sys.stderr,flush=True)
if mode=='loading': time.sleep(30)
gpu={'backend':'Vulkan','name':'Vulkan1','description':'AMD Radeon RX 7700S','pci_id':'0000:03:00.0','gpu_index':1}
runtime={'engine_version':'whisper.cpp 1.9.5, source-declared','source_revision':launch['settings']['runtime_revision'],
 'model':{'path':launch['settings']['model_path'],'size_bytes':3,'modified_unix_ns':0,'sha256':'0'*64,'unavailable_reason':None},'native_artifacts':[]}
if mode=='failed_load':
    emit({'type':'failed','error':'fixture model preparation failed','runtime':runtime})
    sys.exit(2)
emit({'type':'ready','gpu':gpu,'preparation_ms':1,'runtime':runtime})
marker('ready')
request=json.loads(sys.stdin.readline())
audio=request['audio']
with open(audio['path'],'rb') as source: captured=source.read()
assert captured[:4]==b'RF64' and len(captured)==audio['data_offset']+audio['frames']*4
assert int.from_bytes(captured[36:44],'little')==audio['frames']
marker('request',str(audio['frames']))
emit({'type':'started'})
marker('started')
if mode=='inference': time.sleep(30)
emit({'type':'result','raw_transcript':'' if mode=='no_text' else 'Grüße aus T3.',
 'provenance':{'source_frames':audio['frames'],'model_frames':(audio['frames']+2)//3,
  'sample_rate':16000,'channels':1,'resampler':'lifecycle fixture','input_padding_frames':0,'removed_output_delay_frames':0,'runtime':runtime},
 'timings':{'preparation_ms':1,'input_preparation_ms':1,'inference_ms':1}})
marker('result')
if mode=='buffered_result': time.sleep(30)
"#;

struct BlockingGate {
    open: Mutex<bool>,
    changed: Condvar,
    entered: AtomicBool,
}
impl BlockingGate {
    fn new(open: bool) -> Self {
        Self {
            open: Mutex::new(open),
            changed: Condvar::new(),
            entered: AtomicBool::new(false),
        }
    }
    fn wait(&self) {
        self.entered.store(true, Ordering::SeqCst);
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct CaptureControl {
    inputs: BlockingGate,
    stop: BlockingGate,
    started: AtomicBool,
    ready: AtomicBool,
    fault: Mutex<Option<String>>,
}
impl CaptureControl {
    fn new() -> Self {
        Self {
            inputs: BlockingGate::new(true),
            stop: BlockingGate::new(true),
            started: AtomicBool::new(false),
            ready: AtomicBool::new(true),
            fault: Mutex::new(None),
        }
    }
}

struct SyntheticCapture {
    control: Arc<CaptureControl>,
}
struct SyntheticRun {
    ticket: CaptureTicket,
    control: Arc<CaptureControl>,
}

impl CaptureProvider for SyntheticCapture {
    fn inputs(&self) -> Result<InputCatalog> {
        self.control.inputs.wait();
        Ok(InputCatalog {
            default: Some("fixture-input".into()),
            devices: vec![InputDevice {
                name: "fixture-input".into(),
                description: "Synthetic lifecycle input".into(),
                serial: 17,
            }],
        })
    }
    fn start(
        &self,
        mut ticket: CaptureTicket,
        _input: &InputDevice,
    ) -> Result<Box<dyn CaptureRun>> {
        ensure!(
            ticket.push_synthetic(&vec![0.25; SAMPLES]),
            "fixture capture rejected samples"
        );
        self.control.started.store(true, Ordering::SeqCst);
        Ok(Box::new(SyntheticRun {
            ticket,
            control: self.control.clone(),
        }))
    }
}
impl CaptureRun for SyntheticRun {
    fn progress(&self) -> CaptureProgress {
        CaptureProgress {
            ready: self.control.ready.load(Ordering::SeqCst),
            accepted_frames: SAMPLES as u64,
            written_frames: 0,
            error: self.control.fault.lock().unwrap().clone(),
        }
    }
    fn stop(self: Box<Self>, reason: CaptureStop) -> Result<CaptureOutcome> {
        self.control.stop.wait();
        self.ticket.finish(reason)
    }
}

struct TargetControl {
    journal: Arc<Journal>,
    reserved: Mutex<Vec<TargetPermit>>,
    delivered: Mutex<Vec<String>>,
    arm_entered: AtomicBool,
    delay_arm: AtomicBool,
    arm_gate: Semaphore,
    arm_failure: Mutex<Option<ArmFailure>>,
    dispatch_entered: AtomicBool,
    delay_dispatch: AtomicBool,
    dispatch_gate: Semaphore,
}

#[derive(Clone, Copy)]
enum ArmFailure {
    Rejected,
    Panicked,
}
impl TargetControl {
    fn new(journal: Arc<Journal>) -> Self {
        Self {
            journal,
            reserved: Mutex::new(Vec::new()),
            delivered: Mutex::new(Vec::new()),
            arm_entered: AtomicBool::new(false),
            delay_arm: AtomicBool::new(false),
            arm_gate: Semaphore::new(0),
            arm_failure: Mutex::new(None),
            dispatch_entered: AtomicBool::new(false),
            delay_dispatch: AtomicBool::new(false),
            dispatch_gate: Semaphore::new(0),
        }
    }
    fn away_and_return(&self) {
        // The real observer revokes the shared permit permanently, even if T3 returns.
        for permit in self.reserved.lock().unwrap().iter() {
            permit.revoke();
        }
    }
    fn allow_arm(&self) {
        self.arm_gate.add_permits(1);
    }
    fn allow_dispatch(&self) {
        self.dispatch_gate.add_permits(1);
    }
    fn delivered(&self) -> Vec<String> {
        self.delivered.lock().unwrap().clone()
    }
}

#[async_trait]
impl Insertion for TargetControl {
    fn reserve(&self) -> Result<TargetPermit> {
        let permit = TargetPermit::new(TargetIdentity {
            window_id: "t3-original".into(),
            pid: 123,
            application: "T3".into(),
        });
        self.reserved.lock().unwrap().push(permit.clone());
        Ok(permit)
    }
    async fn arm(&self, permit: TargetPermit) -> Result<TargetPermit> {
        self.arm_entered.store(true, Ordering::SeqCst);
        if self.delay_arm.load(Ordering::SeqCst) {
            self.arm_gate.acquire().await.unwrap().forget();
        }
        match *self.arm_failure.lock().unwrap() {
            Some(ArmFailure::Rejected) => anyhow::bail!("fixture composer qualification rejected"),
            Some(ArmFailure::Panicked) => panic!("fixture qualification task panicked"),
            None => {}
        }
        Ok(permit)
    }
    async fn dispatch(&self, permit: &TargetPermit, text: &str) -> InsertionOutcome {
        self.dispatch_entered.store(true, Ordering::SeqCst);
        if self.delay_dispatch.load(Ordering::SeqCst) {
            self.dispatch_gate.acquire().await.unwrap().forget();
        }
        if !permit.is_valid() {
            return InsertionOutcome {
                stage: DispatchStage::Unchanged,
                status: DeliveryStatus::Suppressed,
                detail: "Original target eligibility was revoked.".into(),
            };
        }
        // The observable dispatch seam must see both durable capture and settled recognition.
        let record = self.journal.recent(1).unwrap().remove(0);
        assert_eq!(record.state, "complete");
        assert!(record.persisted);
        assert_eq!(record.synced_frames, SAMPLES as u64);
        assert_eq!(record.attempts[0].state, "complete");
        assert_eq!(record.attempts[0].details["reaped"], true);
        assert_worker_gone(&self.journal.data_dir().parent().unwrap().join("pid"));
        self.delivered.lock().unwrap().push(text.to_owned());
        InsertionOutcome {
            stage: DispatchStage::PasteDispatched,
            status: DeliveryStatus::Unverified,
            detail: "Fixture Paste dispatched with unverified target acceptance.".into(),
        }
    }
    async fn copy_explicit(&self, text: &str) -> Result<()> {
        self.delivered.lock().unwrap().push(text.to_owned());
        Ok(())
    }
}

struct Fixture {
    directory: TempDir,
    journal: Arc<Journal>,
    capture: Arc<CaptureControl>,
    target: Arc<TargetControl>,
    dictation: Arc<Dictation>,
}
impl Fixture {
    fn new(worker_mode: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let executable = root.join("worker");
        fs::write(&executable, PROCESS.replace("MODE", worker_mode)).unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("model.bin"), b"abc").unwrap();
        fs::write(root.join("libwhisper.so"), b"fixture availability only").unwrap();
        let journal = Arc::new(
            Journal::open_with_admission_floor(root.join("data"), root.join("state"), 0).unwrap(),
        );
        let capture = Arc::new(CaptureControl::new());
        let target = Arc::new(TargetControl::new(journal.clone()));
        let settings = Settings {
            model_profile: RecognitionModel::LargeV3,
            input_name: None,
            model_path: root.join("model.bin"),
            native_lib_dir: root.to_owned(),
            preferred_hold: "F2".into(),
        };
        let dictation = Dictation::with_capture(
            journal.clone(),
            target.clone(),
            executable,
            root.join("settings.json"),
            settings,
            Arc::new(SyntheticCapture {
                control: capture.clone(),
            }),
        );
        Self {
            directory,
            journal,
            capture,
            target,
            dictation,
        }
    }
    fn marker(&self, name: &str) -> PathBuf {
        self.directory.path().join(name)
    }
    async fn marker_ready(&self, name: &str) {
        wait_until(|| self.marker(name).exists()).await;
    }
    async fn recording(&self) {
        wait_until(|| self.dictation.snapshot().recording).await;
    }
    async fn idle(&self) {
        wait_until(|| !self.dictation.snapshot().busy).await;
    }
    fn record(&self) -> RecordingSnapshot {
        let records = self.journal.recent(10).unwrap();
        assert_eq!(records.len(), 1);
        records.into_iter().next().unwrap()
    }
    fn retained_prefix(&self, record: &RecordingSnapshot) {
        assert_eq!(record.available_frames, SAMPLES as u64);
        assert_eq!(record.accepted_frames, SAMPLES as u64);
        assert!(record.persisted);
        assert_eq!(
            read_f32(&self.journal.open_audio(record.recording_id).unwrap()).unwrap(),
            vec![0.25; SAMPLES]
        );
    }
    fn reaped(&self, record: &RecordingSnapshot) {
        assert_eq!(record.attempts.len(), 1);
        assert_eq!(record.attempts[0].details["reaped"], true);
        assert_worker_gone(&self.marker("pid"));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // A failed assertion must not leave a blocked fixture thread or native process.
        self.capture.inputs.release();
        self.capture.stop.release();
        self.target.allow_arm();
        self.target.allow_dispatch();
        self.dictation.cancel();
    }
}

async fn wait_until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("public lifecycle did not reach expected state");
}

fn assert_worker_gone(marker: &std::path::Path) {
    let pid = fs::read_to_string(marker).unwrap().parse::<i32>().unwrap();
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        -1,
        "worker must be gone before dispatch or idle"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[tokio::test]
async fn target_is_reserved_at_admission_and_away_return_before_qualification_stays_revoked() {
    let fixture = Fixture::new("normal");
    fixture.target.delay_arm.store(true, Ordering::SeqCst);
    fixture.dictation.begin().unwrap();
    assert_eq!(fixture.target.reserved.lock().unwrap().len(), 1);
    assert!(!fixture.target.arm_entered.load(Ordering::SeqCst));
    fixture.target.away_and_return();
    fixture.recording().await;
    fixture.target.allow_arm();
    fixture.dictation.release();
    fixture.idle().await;
    assert!(fixture.target.delivered().is_empty());
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.attempts[0].state, "complete");
    assert_eq!(record.insertions[0].state, "suppressed");
    assert_eq!(record.insertions[0].stage, "unchanged");
}

#[tokio::test]
async fn release_persists_complete_audio_and_attempt_before_unverified_dispatch() {
    let fixture = Fixture::new("normal");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.dictation.release();
    fixture.idle().await;
    assert_eq!(fixture.target.delivered(), vec![TEXT]);
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "complete");
    assert_eq!(record.attempts[0].raw_text.as_deref(), Some(TEXT));
    assert!(record.attempts[0].details["release_to_result_ms"].is_number());
    assert_eq!(record.insertions[0].state, "unverified");
    assert_eq!(record.insertions[0].stage, "paste_dispatched");
    assert!(record.insertions[0].details["release_to_dispatch_ms"].is_number());
}

#[tokio::test]
async fn repeat_and_rejected_hold_release_during_processing_do_not_create_or_end_another_recording()
{
    let fixture = Fixture::new("inference");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.dictation.begin().unwrap();
    assert!(fixture.dictation.snapshot().detail.contains("busy"));
    assert!(fixture.dictation.snapshot().recording);
    fixture.dictation.release();
    fixture.marker_ready("started").await;
    fixture.dictation.begin().unwrap();
    fixture.dictation.release();
    assert!(fixture.dictation.snapshot().busy);
    assert_eq!(fixture.journal.recent(10).unwrap().len(), 1);
    assert_eq!(fixture.target.reserved.lock().unwrap().len(), 1);
    assert!(fixture.target.delivered().is_empty());
    fixture.dictation.cancel();
    fixture.idle().await;
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "complete");
    assert_eq!(record.attempts[0].state, "canceled");
}

#[tokio::test]
async fn cancel_capture_keeps_admission_busy_until_audio_finalization_and_worker_reaping() {
    let fixture = Fixture::new("normal");
    *fixture.capture.stop.open.lock().unwrap() = false;
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    assert!(
        fixture
            .dictation
            .with_idle(|| -> anyhow::Result<()> { panic!("busy host action must not run") })
            .is_err()
    );
    fixture.marker_ready("pid").await;
    fixture.dictation.cancel();
    wait_until(|| fixture.capture.stop.entered.load(Ordering::SeqCst)).await;
    assert!(fixture.dictation.snapshot().busy);
    assert!(!fixture.dictation.snapshot().recording);
    assert!(
        fixture
            .dictation
            .with_idle(|| -> anyhow::Result<()> { panic!("cleanup host action must not run") })
            .is_err()
    );
    assert_eq!(fixture.record().state, "capturing");
    fixture.dictation.begin().unwrap();
    fixture.dictation.release();
    assert_eq!(fixture.target.reserved.lock().unwrap().len(), 1);
    assert!(fixture.target.delivered().is_empty());
    fixture.capture.stop.release();
    fixture.idle().await;
    assert_eq!(
        fixture
            .dictation
            .with_idle(|| Ok("idle host action"))
            .unwrap(),
        "idle host action"
    );
    assert_eq!(fixture.dictation.snapshot().phase, "Ready");
    assert!(!fixture.dictation.snapshot().error);
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "canceled");
    assert_eq!(record.attempts[0].state, "canceled");
    assert!(record.insertions.is_empty());
}

#[tokio::test]
async fn cancel_during_loading_inference_and_buffered_result_never_inserts() {
    for mode in ["loading", "inference", "buffered_result"] {
        let fixture = Fixture::new(mode);
        fixture.dictation.begin().unwrap();
        fixture.recording().await;
        fixture.marker_ready("pid").await;
        fixture.dictation.release();
        if mode == "loading" {
            wait_until(|| fixture.dictation.snapshot().phase == "Preparing").await;
        } else {
            fixture
                .marker_ready(if mode == "inference" {
                    "started"
                } else {
                    "result"
                })
                .await;
        }
        assert!(fixture.dictation.snapshot().busy);
        fixture.dictation.cancel();
        fixture.idle().await;
        assert_eq!(fixture.dictation.snapshot().phase, "Ready", "{mode}");
        assert!(!fixture.dictation.snapshot().error, "{mode}");
        assert!(fixture.target.delivered().is_empty(), "{mode}");
        let record = fixture.record();
        fixture.retained_prefix(&record);
        fixture.reaped(&record);
        assert_eq!(record.state, "complete", "{mode}");
        assert_eq!(record.attempts[0].state, "canceled", "{mode}");
        if mode == "buffered_result" {
            assert_eq!(record.attempts[0].raw_text.as_deref(), Some(TEXT));
        }
        assert!(record.insertions.is_empty(), "{mode}");
    }
}

#[tokio::test]
async fn capture_fault_preserves_available_audio_cancels_recognition_and_suppresses_insertion() {
    let fixture = Fixture::new("normal");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.marker_ready("pid").await;
    *fixture.capture.fault.lock().unwrap() = Some("fixture input disappeared".into());
    fixture.idle().await;
    assert!(fixture.dictation.snapshot().error);
    assert_eq!(fixture.dictation.snapshot().phase, "Needs attention");
    assert!(
        fixture
            .dictation
            .snapshot()
            .detail
            .contains("fixture input disappeared")
    );
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "failed");
    assert!(record.error.unwrap().contains("input disappeared"));
    assert_eq!(record.attempts[0].state, "canceled");
    assert!(record.insertions.is_empty());
    assert!(fixture.target.delivered().is_empty());
}

#[tokio::test]
async fn session_loss_interrupts_capture_and_permanently_revokes_target_eligibility() {
    let fixture = Fixture::new("normal");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.marker_ready("pid").await;
    fixture
        .dictation
        .session_lost("fixture portal connection lost");
    fixture.idle().await;
    assert!(fixture.dictation.snapshot().error);
    assert_eq!(fixture.dictation.snapshot().phase, "Needs attention");
    assert!(
        fixture
            .dictation
            .snapshot()
            .detail
            .contains("desktop session lost")
    );
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "interrupted");
    assert_eq!(record.attempts[0].state, "canceled");
    assert!(
        fixture
            .target
            .reserved
            .lock()
            .unwrap()
            .iter()
            .all(|permit| !permit.is_valid())
    );
    assert!(fixture.target.delivered().is_empty());
    assert!(record.insertions.is_empty());
}

#[tokio::test]
async fn cancellation_at_dispatch_boundary_keeps_clipboard_unchanged_and_records_suppression() {
    let fixture = Fixture::new("normal");
    fixture.target.delay_dispatch.store(true, Ordering::SeqCst);
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.dictation.release();
    wait_until(|| fixture.target.dispatch_entered.load(Ordering::SeqCst)).await;
    fixture.dictation.cancel();
    assert!(fixture.dictation.snapshot().busy);
    assert!(fixture.target.delivered().is_empty());
    fixture.target.allow_dispatch();
    fixture.idle().await;
    let record = fixture.record();
    fixture.reaped(&record);
    assert_eq!(record.attempts[0].state, "complete");
    assert_eq!(record.insertions[0].state, "suppressed");
    assert_eq!(record.insertions[0].stage, "unchanged");
    assert!(fixture.target.delivered().is_empty());
}

#[tokio::test]
async fn early_release_before_input_readiness_starts_no_worker_or_insertion() {
    let fixture = Fixture::new("normal");
    *fixture.capture.inputs.open.lock().unwrap() = false;
    fixture.dictation.begin().unwrap();
    wait_until(|| fixture.capture.inputs.entered.load(Ordering::SeqCst)).await;
    fixture.dictation.release();
    fixture.capture.inputs.release();
    fixture.idle().await;
    assert!(!fixture.capture.started.load(Ordering::SeqCst));
    assert!(!fixture.marker("pid").exists());
    assert!(fixture.target.delivered().is_empty());
    let record = fixture.record();
    assert_eq!(record.available_frames, 0);
    assert_eq!(record.attempts[0].state, "no_text");
    assert!(record.insertions.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_racing_admission_settles_and_rejects_all_later_jobs() {
    for _ in 0..4 {
        let fixture = Fixture::new("normal");
        let barrier = Arc::new(tokio::sync::Barrier::new(3));
        let begin = {
            let dictation = fixture.dictation.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                dictation.begin()
            })
        };
        let shutdown = {
            let dictation = fixture.dictation.clone();
            let barrier = barrier.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                dictation.shutdown().await
            })
        };
        barrier.wait().await;
        let _ = begin.await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), shutdown)
            .await
            .unwrap()
            .unwrap();
        assert!(!fixture.dictation.snapshot().busy);
        let records = fixture.journal.recent(10).unwrap();
        assert!(records.len() <= 1);
        assert!(
            records
                .iter()
                .all(|record| !["capturing", "initializing"].contains(&record.state.as_str()))
        );
        for _ in 0..8 {
            assert!(fixture.dictation.begin().is_err());
        }
        assert_eq!(fixture.journal.recent(10).unwrap().len(), records.len());
        assert!(fixture.target.delivered().is_empty());
        if fixture.marker("pid").exists() {
            let pid = fs::read_to_string(fixture.marker("pid"))
                .unwrap()
                .parse::<i32>()
                .unwrap();
            assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        }
    }
}

#[tokio::test]
async fn recovery_cannot_reconcile_an_active_capture_and_is_safe_again_after_settlement() {
    let fixture = Fixture::new("normal");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    assert!(fixture.dictation.reconcile_journal().is_err());
    assert!(fixture.journal.recover().is_err());
    assert_eq!(fixture.record().state, "capturing");
    fixture.dictation.cancel();
    fixture.idle().await;
    let before = fixture.record();
    fixture.retained_prefix(&before);
    let report = fixture.dictation.reconcile_journal().unwrap();
    assert!(!report.unpersisted);
    assert_eq!(fixture.record().state, "canceled");
    assert!(fixture.target.delivered().is_empty());
}

#[tokio::test]
async fn recognition_failure_during_capture_keeps_recording_and_retains_audio_after_release() {
    let fixture = Fixture::new("failed_load");
    fixture.dictation.begin().unwrap();
    wait_until(|| {
        let snapshot = fixture.dictation.snapshot();
        snapshot.recording && snapshot.error
    })
    .await;
    assert!(fixture.dictation.snapshot().busy);
    assert!(
        fixture
            .dictation
            .snapshot()
            .detail
            .contains("recognition failed")
    );
    assert_eq!(fixture.record().state, "capturing");
    fixture.dictation.release();
    fixture.idle().await;
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "complete");
    assert_eq!(record.attempts[0].state, "failed");
    assert!(fixture.dictation.snapshot().error);
    assert!(record.insertions.is_empty());
    assert!(fixture.target.delivered().is_empty());
}

#[tokio::test]
async fn empty_recognition_result_retains_complete_audio_without_mutating_clipboard() {
    let fixture = Fixture::new("no_text");
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.dictation.release();
    fixture.idle().await;
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "complete");
    assert_eq!(record.attempts[0].state, "no_text");
    assert_eq!(record.attempts[0].raw_text.as_deref(), Some(""));
    assert!(record.insertions.is_empty());
    assert!(fixture.target.delivered().is_empty());
}

async fn finish_recognition_before_delayed_qualification(fixture: &Fixture) {
    fixture.target.delay_arm.store(true, Ordering::SeqCst);
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    fixture.dictation.release();
    wait_until(|| {
        fixture
            .journal
            .recent(1)
            .unwrap()
            .first()
            .is_some_and(|record| {
                record
                    .attempts
                    .first()
                    .is_some_and(|attempt| attempt.state == "complete")
            })
    })
    .await;
    assert!(fixture.dictation.snapshot().busy);
    assert!(!fixture.target.dispatch_entered.load(Ordering::SeqCst));
    fixture.reaped(&fixture.record());
}

fn assert_failed_qualification_suppressed_delivery(fixture: &Fixture) {
    assert!(!fixture.dictation.snapshot().busy);
    assert!(
        !fixture.target.dispatch_entered.load(Ordering::SeqCst),
        "unqualified target must not reach dispatch"
    );
    assert!(fixture.target.delivered().is_empty());
    let reserved = fixture.target.reserved.lock().unwrap();
    assert_eq!(reserved.len(), 1);
    assert!(
        !reserved[0].is_valid(),
        "failed qualification must permanently revoke its reserved permit"
    );
    drop(reserved);
    let record = fixture.record();
    fixture.retained_prefix(&record);
    fixture.reaped(&record);
    assert_eq!(record.state, "complete");
    assert_eq!(record.attempts[0].state, "complete");
    assert_eq!(record.attempts[0].raw_text.as_deref(), Some(TEXT));
    assert_eq!(record.insertions[0].state, "suppressed");
    assert_eq!(record.insertions[0].stage, "unchanged");
}

#[tokio::test]
async fn delayed_qualification_rejection_after_recognition_revokes_original_permit() {
    let fixture = Fixture::new("normal");
    *fixture.target.arm_failure.lock().unwrap() = Some(ArmFailure::Rejected);
    finish_recognition_before_delayed_qualification(&fixture).await;
    fixture.target.allow_arm();
    fixture.idle().await;
    assert_failed_qualification_suppressed_delivery(&fixture);
}

#[tokio::test]
async fn delayed_qualification_panic_after_recognition_revokes_original_permit() {
    let fixture = Fixture::new("normal");
    *fixture.target.arm_failure.lock().unwrap() = Some(ArmFailure::Panicked);
    finish_recognition_before_delayed_qualification(&fixture).await;
    fixture.target.allow_arm();
    fixture.idle().await;
    assert_failed_qualification_suppressed_delivery(&fixture);
}

#[tokio::test]
async fn delayed_qualification_timeout_after_recognition_revokes_original_permit() {
    let fixture = Fixture::new("normal");
    finish_recognition_before_delayed_qualification(&fixture).await;
    tokio::time::timeout(Duration::from_secs(16), async {
        while fixture.dictation.snapshot().busy {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("qualification deadline must settle the Dictation");
    assert_failed_qualification_suppressed_delivery(&fixture);
}

#[tokio::test]
async fn turbo_settings_reach_worker_and_journal_as_one_snapshot() {
    let fixture = Fixture::new("normal");
    let mut settings = fixture.dictation.settings();
    settings.model_profile = RecognitionModel::LargeV3Turbo;
    // An arbitrary filename must not determine the profile.
    settings.model_path = fixture.directory.path().join("renamed-model.bin");
    fs::write(&settings.model_path, b"abc").unwrap();
    fixture.dictation.save_settings(settings.clone()).unwrap();
    fixture.dictation.begin().unwrap();
    fixture.recording().await;
    let mut changed = settings.clone();
    changed.model_profile = RecognitionModel::LargeV3;
    assert!(fixture.dictation.save_settings(changed).is_err());
    fixture.dictation.release();
    fixture.idle().await;
    let launch: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.marker("settings")).unwrap()).unwrap();
    let record = fixture.record();
    assert_eq!(launch["model_profile"], "large_v3_turbo");
    assert_eq!(launch["model_path"], serde_json::json!(settings.model_path));
    assert_eq!(record.attempts[0].configuration, launch);
    assert_eq!(record.attempts[0].raw_text.as_deref(), Some(TEXT));
    assert_eq!(fixture.target.delivered(), vec![TEXT]);
    fixture.reaped(&record);
}
