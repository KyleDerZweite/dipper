//! Worker-side immutable RF64 model-input preparation.
pub mod artifacts;
use anyhow::{Context, Result, ensure};
use dipper_core::protocol::{AudioSource, ModelInputProvenance};
use rubato::{FftFixedInOut, Resampler};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub const RF64_DATA_OFFSET: u64 = 92;

#[derive(Debug)]
pub struct PreparedInput {
    pub samples: Vec<f32>,
    pub provenance: ModelInputProvenance,
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn open_source(source: &AudioSource) -> Result<File> {
    ensure!(
        source.sample_rate == 48_000 && source.channels == 1,
        "RF64 source must be mono 48 kHz"
    );
    ensure!(
        source.data_offset == RF64_DATA_OFFSET,
        "unsupported RF64 data offset"
    );
    let output_count = source.frames.div_ceil(3);
    ensure!(
        output_count <= i32::MAX as u64,
        "model sample count exceeds Whisper integer limit"
    );
    let data_bytes = source
        .frames
        .checked_mul(4)
        .context("RF64 data size overflow")?;
    let file_size = data_bytes
        .checked_add(RF64_DATA_OFFSET)
        .context("RF64 file size overflow")?;
    let mut file = File::open(&source.path).context("open finalized capture audio")?;
    ensure!(
        file.metadata()?.len() == file_size,
        "RF64 file length does not match committed frame count"
    );
    let mut header = [0_u8; RF64_DATA_OFFSET as usize];
    file.read_exact(&mut header).context("read RF64 header")?;
    ensure!(
        &header[0..4] == b"RF64" && u32_at(&header, 4) == u32::MAX && &header[8..12] == b"WAVE",
        "invalid RF64 container"
    );
    ensure!(
        &header[12..16] == b"ds64" && u32_at(&header, 16) == 28 && u32_at(&header, 44) == 0,
        "unsupported RF64 size table"
    );
    ensure!(
        u64_at(&header, 20) == file_size - 8
            && u64_at(&header, 28) == data_bytes
            && u64_at(&header, 36) == source.frames,
        "RF64 ds64 counts do not match finalized capture"
    );
    ensure!(
        &header[48..52] == b"fmt "
            && u32_at(&header, 52) == 16
            && u16_at(&header, 56) == 3
            && u16_at(&header, 58) == 1
            && u32_at(&header, 60) == 48_000
            && u32_at(&header, 64) == 192_000
            && u16_at(&header, 68) == 4
            && u16_at(&header, 70) == 32,
        "unsupported RF64 sample format"
    );
    ensure!(
        &header[72..76] == b"fact"
            && u32_at(&header, 76) == 4
            && u32_at(&header, 80) == source.frames.min(u32::MAX as u64) as u32,
        "RF64 fact count mismatch"
    );
    ensure!(
        &header[84..88] == b"data" && u32_at(&header, 88) == u32::MAX,
        "invalid RF64 data chunk"
    );
    file.seek(SeekFrom::Start(source.data_offset))?;
    Ok(file)
}

/// Validates the finalized source and resamples complete audio with known delay removal.
/// Padding flushes the filter tail; it never changes the capture artifact or source frame count.
pub fn prepare_model_input(source: &AudioSource) -> Result<PreparedInput> {
    let mut file = open_source(source)?;
    let output_frames =
        usize::try_from(source.frames.div_ceil(3)).context("model frame count overflow")?;
    let mut resampler = FftFixedInOut::<f32>::new(48_000, 16_000, 1536, 1)?;
    let chunk = resampler.input_frames_next();
    let delay = resampler.output_delay();
    let expected = output_frames
        .checked_add(delay)
        .context("resampler output size overflow")?;
    let mut output = Vec::new();
    if source.frames > 0 {
        output.try_reserve_exact(expected + resampler.output_frames_max())?;
    }
    let mut remaining = source.frames;
    let mut padding = 0_u64;
    let mut input = vec![vec![0_f32; chunk]];
    let mut bytes = vec![0_u8; chunk * 4];
    while remaining > 0 {
        let real_frames = remaining.min(chunk as u64) as usize;
        file.read_exact(&mut bytes[..real_frames * 4])
            .context("read complete RF64 samples")?;
        input[0].fill(0.0);
        for (sample, encoded) in input[0][..real_frames]
            .iter_mut()
            .zip(bytes[..real_frames * 4].chunks_exact(4))
        {
            *sample = f32::from_le_bytes(encoded.try_into().unwrap());
            ensure!(
                sample.is_finite(),
                "RF64 source contains a non-finite sample"
            );
        }
        let produced = resampler.process(&input, None)?;
        output.extend_from_slice(&produced[0]);
        padding += (chunk - real_frames) as u64;
        remaining -= real_frames as u64;
    }
    if source.frames > 0 {
        input[0].fill(0.0);
        while output.len() < expected {
            let produced = resampler.process(&input, None)?;
            output.extend_from_slice(&produced[0]);
            padding += chunk as u64;
        }
        output.drain(..delay);
        output.truncate(output_frames);
        ensure!(
            output.len() == output_frames && output.iter().all(|sample| sample.is_finite()),
            "invalid prepared model input"
        );
    }
    Ok(PreparedInput {
        samples: output,
        provenance: ModelInputProvenance {
            source_frames: source.frames,
            model_frames: output_frames as u64,
            sample_rate: 16_000,
            channels: 1,
            resampler: "rubato 0.16.2 FftFixedInOut<f32>; 48000->16000; input chunk 1536; one channel; zero tail padding".into(),
            input_padding_frames: padding,
            removed_output_delay_frames: if source.frames > 0 { delay as u64 } else { 0 },
            runtime: None,
        },
    })
}
