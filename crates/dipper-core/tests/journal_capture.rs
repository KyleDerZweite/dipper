use dipper_core::{
    audio::{self, AudioArtifact, DATA_OFFSET},
    capture::{Capture, CaptureStop, InputDevice},
    journal::{CAPTURE_RING_FRAMES, Journal},
};
use serde_json::json;
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    process::Command,
    thread,
    time::Duration,
};

fn input() -> InputDevice {
    InputDevice {
        name: "synthetic-input".into(),
        description: "Synthetic input".into(),
        serial: 42,
    }
}
fn open(root: &std::path::Path) -> Journal {
    Journal::open_with_admission_floor(root.join("data"), root.join("state"), 0).unwrap()
}

#[test]
fn durable_complete_capture_preserves_bits_and_attempts() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    let id = ticket.recording_id;
    let source = [0.0f32, -0.0, 0.125, -0.75, f32::from_bits(0x3f000001)];
    assert!(ticket.push_synthetic(&source));
    let first = journal
        .begin_attempt(id, json!({"model":"synthetic-v1"}))
        .unwrap();
    let result = ticket.finish(CaptureStop::Released).unwrap();
    assert_eq!(result.state, "complete");
    assert!(result.persisted);
    assert_eq!(result.synced_frames, source.len() as u64);
    let artifact = result.audio.unwrap();
    assert_eq!(
        audio::read_f32(&artifact)
            .unwrap()
            .iter()
            .map(|f| f.to_bits())
            .collect::<Vec<_>>(),
        source.iter().map(|f| f.to_bits()).collect::<Vec<_>>()
    );
    journal
        .finish_attempt(
            first,
            "complete",
            Some("synthetic raw transcript"),
            json!({"timing_ms":1}),
        )
        .unwrap();
    let retry = journal
        .begin_attempt(id, json!({"model":"synthetic-v2"}))
        .unwrap();
    journal
        .finish_attempt(retry, "failed", None, json!({"error":"synthetic failure"}))
        .unwrap();
    let insertion = journal
        .begin_insertion(first, json!({"window":"synthetic-target"}))
        .unwrap();
    journal
        .finish_insertion(insertion, "unverified", "paste", json!({}))
        .unwrap();
    let record = journal.recording(id).unwrap().unwrap();
    assert_eq!(record.attempts.len(), 2);
    assert_eq!(record.insertions[0].state, "unverified");
    assert!(
        journal
            .finish_attempt(first, "complete", Some("overwrite"), json!({}))
            .is_err()
    );
    assert_eq!(
        fs::metadata(&artifact.path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    for path in [
        "journal.sqlite3",
        "journal.sqlite3-wal",
        "journal.sqlite3-shm",
        "journal.lock",
    ] {
        assert_eq!(
            fs::metadata(directory.path().join("data").join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    drop(journal);
    let reopened = open(directory.path());
    assert_eq!(reopened.recording(id).unwrap().unwrap().state, "complete");
    assert_eq!(
        audio::read_f32(&reopened.open_audio(id).unwrap())
            .unwrap()
            .len(),
        source.len()
    );
}

#[test]
fn independent_decoder_accepts_rf64_float_capture() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    let source: Vec<f32> = (0..4800)
        .map(|i| ((i as f32) / 100.0).sin() * 0.5)
        .collect();
    assert!(ticket.push_synthetic(&source));
    let artifact = ticket.finish(CaptureStop::Released).unwrap().audio.unwrap();
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&artifact.path)
        .args(["-f", "f32le", "-acodec", "pcm_f32le", "-"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<u8> = source
        .iter()
        .flat_map(|s| s.to_bits().to_le_bytes())
        .collect();
    assert_eq!(output.stdout, expected);
}

#[test]
fn cancel_overflow_and_empty_release_never_complete() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut canceled = journal.begin_capture(&input()).unwrap();
    assert!(canceled.push_synthetic(&[0.25; 128]));
    let canceled = canceled.finish(CaptureStop::Canceled).unwrap();
    assert_eq!(canceled.state, "canceled");
    assert_eq!(canceled.available_frames, 128);
    let mut failed = journal.begin_capture(&input()).unwrap();
    assert!(failed.push_synthetic(&[0.5; 128]));
    assert!(!failed.push_synthetic(&vec![0.0; (CAPTURE_RING_FRAMES + 1) as usize]));
    let failed = failed.finish(CaptureStop::Released).unwrap();
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.accepted_frames, 128);
    assert!(failed.error.unwrap().contains("overflow"));
    let empty = journal
        .begin_capture(&input())
        .unwrap()
        .finish(CaptureStop::Released)
        .unwrap();
    assert_eq!(empty.state, "failed");
}

#[test]
fn preflight_and_store_lock_reject_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let journal = Journal::open_with_admission_floor(
        directory.path().join("data"),
        directory.path().join("state"),
        u64::MAX,
    )
    .unwrap();
    assert!(journal.begin_capture(&input()).is_err());
    assert!(journal.recent(10).unwrap().is_empty());
    assert!(
        Journal::open(
            directory.path().join("data"),
            directory.path().join("state")
        )
        .is_err()
    );
}

#[test]
fn recovery_preserves_originals_reports_orphans_and_uncertain_insertions() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    let id = ticket.recording_id;
    assert!(ticket.push_synthetic(&[0.5; 256]));
    let artifact = ticket.finish(CaptureStop::Canceled).unwrap().audio.unwrap();
    let attempt = journal.begin_attempt(id, json!({})).unwrap();
    journal
        .begin_insertion(attempt, json!({"window":"synthetic"}))
        .unwrap();
    drop(journal);
    let connection =
        rusqlite::Connection::open(directory.path().join("data/journal.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE recordings SET state='capturing',ended_at=NULL,synced_frames=128 WHERE id=?1",
            [id.to_string()],
        )
        .unwrap();
    drop(connection);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(&artifact.path)
        .unwrap();
    file.seek(SeekFrom::Start(28)).unwrap();
    file.write_all(&0u64.to_le_bytes()).unwrap();
    drop(file);
    let before = fs::read(&artifact.path).unwrap();
    let orphan = directory.path().join("data/audio/orphan.wav");
    fs::write(&orphan, b"orphan fixture").unwrap();
    let reopened = open(directory.path());
    let record = reopened.recording(id).unwrap().unwrap();
    assert_eq!(record.state, "interrupted");
    assert_eq!(record.available_frames, 256);
    assert_eq!(record.synced_frames, 128);
    assert_eq!(record.attempts[0].state, "interrupted");
    assert_eq!(record.insertions[0].state, "uncertain");
    assert_eq!(reopened.recovery_report().orphan_files, vec![orphan]);
    assert_eq!(
        audio::read_recovered_f32(&reopened.open_audio(id).unwrap()).unwrap(),
        vec![0.5; 256]
    );
    reopened.recover().unwrap();
    assert_eq!(fs::read(&artifact.path).unwrap(), before);
}

#[test]
fn missing_and_shortened_audio_are_explicit() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut a = journal.begin_capture(&input()).unwrap();
    let aid = a.recording_id;
    a.push_synthetic(&[0.25; 32]);
    let artifact = a.finish(CaptureStop::Released).unwrap().audio.unwrap();
    fs::remove_file(&artifact.path).unwrap();
    let mut b = journal.begin_capture(&input()).unwrap();
    let bid = b.recording_id;
    b.push_synthetic(&[0.25; 32]);
    let artifact = b.finish(CaptureStop::Released).unwrap().audio.unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&artifact.path)
        .unwrap()
        .set_len(DATA_OFFSET + 4)
        .unwrap();
    journal.recover().unwrap();
    assert_eq!(
        journal.recording(aid).unwrap().unwrap().availability,
        "missing"
    );
    assert_eq!(
        journal.recording(bid).unwrap().unwrap().availability,
        "inconsistent"
    );
    assert!(journal.open_audio(aid).is_err());
    assert!(journal.open_audio(bid).is_err());
}

#[test]
fn input_selection_resolves_current_default_without_fallback() {
    let catalog = Capture::parse_inventory(&json!([
        {"info":{"props":{"media.class":"Audio/Source","node.name":"a","object.serial":42}}},
        {"info":{"props":{"media.class":"Audio/Source","node.name":"b","object.serial":"43"}}},
        {"metadata":[{"key":"default.audio.source","value":"{\"name\":\"b\"}"}]}
    ]))
    .unwrap();
    assert_eq!(catalog.resolve(None).unwrap().serial, 43);
    assert_eq!(catalog.resolve(Some("a")).unwrap().serial, 42);
    assert!(catalog.resolve(Some("missing")).is_err());
}

#[test]
fn process_exit_preserves_checkpoint_and_unsynced_tail() {
    let directory = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_fixture", "--ignored"])
        .env("DIPPER_CRASH_FIXTURE", directory.path())
        .status()
        .unwrap();
    assert!(status.success());
    let journal = open(directory.path());
    let record = journal.recent(1).unwrap().remove(0);
    assert_eq!(record.state, "interrupted");
    assert_eq!(record.synced_frames, 4800);
    assert_eq!(record.available_frames, 7200);
    let original = fs::read(record.audio.as_ref().unwrap().path.clone()).unwrap();
    journal.recover().unwrap();
    assert_eq!(fs::read(record.audio.unwrap().path).unwrap(), original);
}

#[test]
#[ignore = "owned child-process fixture"]
fn crash_fixture() {
    let root = std::env::var_os("DIPPER_CRASH_FIXTURE").expect("fixture directory");
    let journal = open(std::path::Path::new(&root));
    let mut ticket = journal.begin_capture(&input()).unwrap();
    let id = ticket.recording_id;
    assert!(ticket.push_synthetic(&[0.5; 4800]));
    for _ in 0..200 {
        if journal.recording(id).unwrap().unwrap().synced_frames == 4800 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(journal.recording(id).unwrap().unwrap().synced_frames, 4800);
    assert!(ticket.push_synthetic(&[0.25; 2400]));
    thread::sleep(Duration::from_millis(30));
    // Exit only this owned synthetic fixture, without Rust destructors/finalization.
    unsafe { libc::_exit(0) }
}

#[test]
fn rf64_reader_rejects_changed_header_or_frame_count() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    ticket.push_synthetic(&[0.0; 32]);
    let artifact = ticket.finish(CaptureStop::Released).unwrap().audio.unwrap();
    let invalid = AudioArtifact {
        frames: 33,
        ..artifact.clone()
    };
    assert!(audio::read_f32(&invalid).is_err());
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(&artifact.path)
        .unwrap();
    file.seek(SeekFrom::Start(36)).unwrap();
    file.write_all(&31u64.to_le_bytes()).unwrap();
    assert!(audio::read_f32(&artifact).is_err());
}

#[test]
fn actual_audio_write_error_preserves_the_written_prefix() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "audio_write_failure_fixture", "--ignored"])
        .env("DIPPER_WRITE_FAILURE_FIXTURE", directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "owned write-failure fixture failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let journal = open(directory.path());
    let record = journal.recent(1).unwrap().remove(0);
    assert_eq!(record.state, "interrupted");
    assert_eq!(record.available_frames, 2);
    assert_eq!(record.synced_frames, 0);
    assert_eq!(
        audio::read_recovered_f32(&journal.open_audio(record.recording_id).unwrap()).unwrap(),
        vec![0.5; 2]
    );
}

#[test]
#[ignore = "owned child-process file limit fixture"]
fn audio_write_failure_fixture() {
    let root = std::env::var_os("DIPPER_WRITE_FAILURE_FIXTURE").expect("fixture directory");
    let journal = open(std::path::Path::new(&root));
    let mut ticket = journal.begin_capture(&input()).unwrap();
    // Restrict only this synthetic child process. No filesystem is filled.
    unsafe {
        libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
        let limit = libc::rlimit {
            rlim_cur: DATA_OFFSET + 8,
            rlim_max: DATA_OFFSET + 8,
        };
        assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
    }
    assert!(ticket.push_synthetic(&[0.5; 128]));
    let result = ticket.finish(CaptureStop::Released).unwrap();
    assert_eq!(result.state, "failed");
    assert_eq!(result.available_frames, 2);
    assert!(result.error.unwrap().contains("audio write failed"));
}

#[test]
fn capture_drains_trailing_ring_frames_pushed_before_stop_even_after_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    let id = ticket.recording_id;
    assert!(ticket.push_synthetic(&[0.25; 256]));
    for _ in 0..300 {
        if journal.recording(id).unwrap().unwrap().synced_frames == 256 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(journal.recording(id).unwrap().unwrap().synced_frames, 256);

    // Push trailing frames right before stop to reproduce the 512-frame stop/checkpoint race.
    assert!(ticket.push_synthetic(&[0.5; 512]));
    let outcome = ticket.finish(CaptureStop::Released).unwrap();
    assert_eq!(outcome.state, "complete");
    assert!(outcome.persisted);
    assert!(outcome.error.is_none());
    assert_eq!(outcome.accepted_frames, 768);
    assert_eq!(outcome.available_frames, 768);
    assert_eq!(outcome.synced_frames, 768);
    let mut expected = vec![0.25; 256];
    expected.extend(vec![0.5; 512]);
    assert_eq!(audio::read_f32(&outcome.audio.unwrap()).unwrap(), expected);
}

#[test]
fn capture_overflow_preserves_accepted_frames_and_sets_queue_overflow_diagnostics() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    assert!(ticket.push_synthetic(&[0.25; 1024]));
    // Exceed remaining ring capacity in a single write to force overflow without dropping 1024.
    let overflow_batch = vec![0.1; (CAPTURE_RING_FRAMES + 512) as usize];
    assert!(!ticket.push_synthetic(&overflow_batch));
    let outcome = ticket.finish(CaptureStop::Released).unwrap();
    assert_eq!(outcome.state, "failed");
    assert!(outcome.error.as_deref().unwrap_or("").contains("overflow"));
    assert_eq!(outcome.accepted_frames, 1024);
    assert_eq!(outcome.available_frames, 1024);
    let diag = outcome.diagnostics.unwrap();
    assert_eq!(diag.failure_category.as_deref(), Some("queue_overflow"));
    assert!(diag.ring_capacity_frames >= CAPTURE_RING_FRAMES);

    let snapshot = journal.recording(outcome.recording_id).unwrap().unwrap();
    assert!(snapshot.recoverable);
    let artifact = snapshot.audio.unwrap();
    let raw_before = fs::read(&artifact.path).unwrap();
    let opened = journal.open_audio(outcome.recording_id).unwrap();
    assert!(!opened.recovered);
    assert_eq!(audio::read_f32(&opened).unwrap(), vec![0.25; 1024]);
    assert_eq!(fs::read(&artifact.path).unwrap(), raw_before);
}

#[test]
fn quick_empty_release_classifies_too_short_and_zero_frame_audio_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let ticket = journal.begin_capture(&input()).unwrap();
    let outcome = ticket.finish(CaptureStop::Released).unwrap();
    assert_eq!(outcome.state, "failed");
    assert_eq!(outcome.available_frames, 0);
    let diag = outcome.diagnostics.unwrap();
    assert_eq!(diag.failure_category.as_deref(), Some("too_short"));
    assert!(journal.open_audio(outcome.recording_id).is_err());
    assert!(
        !journal
            .recording(outcome.recording_id)
            .unwrap()
            .unwrap()
            .recoverable
    );
}

#[test]
fn audio_initialization_failure_is_storage_error_not_too_short() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let audio_dir = directory.path().join("data/audio");
    fs::create_dir_all(&audio_dir).unwrap();
    fs::set_permissions(&audio_dir, fs::Permissions::from_mode(0o500)).unwrap();
    let result = journal.begin_capture(&input());
    fs::set_permissions(&audio_dir, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    let snapshot = journal.recent(1).unwrap().remove(0);
    assert_eq!(snapshot.state, "failed");
    let diag = snapshot.capture_diagnostics.unwrap();
    assert_eq!(diag.failure_category.as_deref(), Some("storage_error"));
    assert!(diag.failure_hint.unwrap().contains("writable"));
}

#[test]
fn bounded_prefix_readers_and_wav_preview_validate_container_and_edge_floats() {
    let directory = tempfile::tempdir().unwrap();
    let journal = open(directory.path());
    let mut ticket = journal.begin_capture(&input()).unwrap();
    ticket.push_synthetic(&[0.25; 24]);
    let artifact = ticket.finish(CaptureStop::Released).unwrap().audio.unwrap();
    assert_eq!(audio::read_f32_prefix(&artifact, 9).unwrap(), vec![0.25; 9]);

    let mut recovered = artifact.clone();
    recovered.recovered = true;
    assert_eq!(
        audio::read_recovered_prefix_f32(&recovered, 6).unwrap(),
        vec![0.25; 6]
    );

    let mut corrupt = fs::read(&artifact.path).unwrap();
    corrupt[0..4].copy_from_slice(b"RIFF");
    fs::write(&artifact.path, corrupt).unwrap();
    assert!(audio::read_recovered_prefix_f32(&recovered, 6).is_err());

    let wav = audio::to_16k_mono_pcm16_wav(&[1e38, 1e38, 1e38, f32::NAN, 0.0, 0.0], 1024).unwrap();
    assert_eq!(&wav[0..4], b"RIFF");
    let first_sample = i16::from_le_bytes([wav[44], wav[45]]);
    let second_sample = i16::from_le_bytes([wav[46], wav[47]]);
    assert_eq!(first_sample, i16::MAX);
    assert_eq!(second_sample, 0);
}
