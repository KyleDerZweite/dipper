use dipper_core::protocol::AudioSource;
use dipper_worker::prepare_model_input;
use std::fs;

fn source(directory: &tempfile::TempDir, samples: &[f32]) -> AudioSource {
    let count = samples.len() as u64;
    let mut header = Vec::new();
    header.extend(b"RF64");
    header.extend(u32::MAX.to_le_bytes());
    header.extend(b"WAVEds64");
    header.extend(28_u32.to_le_bytes());
    header.extend((84 + count * 4).to_le_bytes());
    header.extend((count * 4).to_le_bytes());
    header.extend(count.to_le_bytes());
    header.extend(0_u32.to_le_bytes());
    header.extend(b"fmt ");
    header.extend(16_u32.to_le_bytes());
    header.extend(3_u16.to_le_bytes());
    header.extend(1_u16.to_le_bytes());
    header.extend(48_000_u32.to_le_bytes());
    header.extend(192_000_u32.to_le_bytes());
    header.extend(4_u16.to_le_bytes());
    header.extend(32_u16.to_le_bytes());
    header.extend(b"fact");
    header.extend(4_u32.to_le_bytes());
    header.extend((count as u32).to_le_bytes());
    header.extend(b"data");
    header.extend(u32::MAX.to_le_bytes());
    assert_eq!(header.len(), 92);
    for sample in samples {
        header.extend(sample.to_le_bytes());
    }
    let path = directory.path().join("capture.rf64.wav");
    fs::write(&path, header).unwrap();
    AudioSource {
        path,
        sample_rate: 48_000,
        channels: 1,
        frames: count,
        data_offset: 92,
    }
}

#[test]
fn empty_short_and_partial_blocks_have_exact_counts_and_preserve_original() {
    for frames in [0, 1, 2, 3, 1535, 1536, 1537, 3071] {
        let directory = tempfile::tempdir().unwrap();
        let source = source(&directory, &vec![0.25; frames]);
        let original = fs::read(&source.path).unwrap();
        let prepared = prepare_model_input(&source).unwrap();
        assert_eq!(prepared.samples.len(), frames.div_ceil(3));
        assert_eq!(prepared.provenance.source_frames, frames as u64);
        assert_eq!(prepared.provenance.model_frames, frames.div_ceil(3) as u64);
        if frames > 0 {
            assert_eq!(prepared.provenance.removed_output_delay_frames, 256);
        }
        assert_eq!(fs::read(&source.path).unwrap(), original);
    }
}

#[test]
fn beginning_and_end_impulses_survive_delay_removal_and_tail_flush() {
    for position in [0, 6002] {
        let directory = tempfile::tempdir().unwrap();
        let mut samples = vec![0.0; 6003];
        samples[position] = 1.0;
        let prepared = prepare_model_input(&source(&directory, &samples)).unwrap();
        let peak = prepared
            .samples
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.abs().total_cmp(&right.1.abs()))
            .unwrap();
        assert!(peak.1.abs() > 0.1);
        assert!(peak.0.abs_diff(position / 3) <= 1);
    }
}

#[test]
fn nonfinite_corrupt_counts_and_unsupported_format_fail_without_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let nan = source(&directory, &[f32::NAN]);
    assert!(
        prepare_model_input(&nan)
            .unwrap_err()
            .to_string()
            .contains("non-finite")
    );
    let mut valid = source(&directory, &[0.0; 4]);
    valid.frames = 3;
    assert!(prepare_model_input(&valid).is_err());
    valid.frames = 4;
    let mut bytes = fs::read(&valid.path).unwrap();
    bytes[36..44].copy_from_slice(&99_u64.to_le_bytes());
    fs::write(&valid.path, bytes).unwrap();
    assert!(prepare_model_input(&valid).is_err());
    let mut valid = source(&directory, &[0.0; 4]);
    valid.sample_rate = 44_100;
    assert!(prepare_model_input(&valid).is_err());
    let mut valid = source(&directory, &[0.0; 4]);
    valid.frames = (i32::MAX as u64 + 1) * 3;
    assert!(prepare_model_input(&valid).is_err());
}
