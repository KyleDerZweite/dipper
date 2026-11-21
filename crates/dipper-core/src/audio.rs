//! Private-store RF64 audio. Capture samples are preserved as IEEE float bits.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

pub const DATA_OFFSET: u64 = 92;
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: u16 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioArtifact {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub data_offset: u64,
    #[serde(default)]
    pub recovered: bool,
}

pub(crate) struct AudioWriter {
    file: File,
    frames: u64,
    path: PathBuf,
}

fn header(frames: u64) -> Result<[u8; DATA_OFFSET as usize]> {
    let bytes = frames.checked_mul(4).context("audio size overflow")?;
    let mut h = [0u8; DATA_OFFSET as usize];
    h[0..4].copy_from_slice(b"RF64");
    h[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"ds64");
    h[16..20].copy_from_slice(&28u32.to_le_bytes());
    h[20..28].copy_from_slice(
        &bytes
            .checked_add(DATA_OFFSET - 8)
            .context("RIFF size overflow")?
            .to_le_bytes(),
    );
    h[28..36].copy_from_slice(&bytes.to_le_bytes());
    h[36..44].copy_from_slice(&frames.to_le_bytes());
    h[48..52].copy_from_slice(b"fmt ");
    h[52..56].copy_from_slice(&16u32.to_le_bytes());
    h[56..58].copy_from_slice(&3u16.to_le_bytes());
    h[58..60].copy_from_slice(&CHANNELS.to_le_bytes());
    h[60..64].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    h[64..68].copy_from_slice(&(SAMPLE_RATE * 4).to_le_bytes());
    h[68..70].copy_from_slice(&4u16.to_le_bytes());
    h[70..72].copy_from_slice(&32u16.to_le_bytes());
    h[72..76].copy_from_slice(b"fact");
    h[76..80].copy_from_slice(&4u32.to_le_bytes());
    h[80..84].copy_from_slice(&u32::try_from(frames).unwrap_or(u32::MAX).to_le_bytes());
    h[84..88].copy_from_slice(b"data");
    h[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    Ok(h)
}

impl AudioWriter {
    pub(crate) fn create(path: &Path) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(&header(0)?)?;
        file.sync_all()?;
        File::open(path.parent().context("audio has no parent directory")?)?.sync_all()?;
        Ok(Self {
            file,
            frames: 0,
            path: path.to_owned(),
        })
    }
    pub(crate) fn write_samples(&mut self, samples: &[f32]) -> Result<()> {
        // The writer thread owns this allocation; the realtime callback never enters here.
        let mut bytes = Vec::with_capacity(samples.len() * 4);
        for sample in samples {
            bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
        }
        self.file.write_all(&bytes)?;
        self.frames = self
            .frames
            .checked_add(samples.len() as u64)
            .context("frame count overflow")?;
        Ok(())
    }
    pub(crate) fn frames(&self) -> u64 {
        self.frames
    }
    pub(crate) fn checkpoint(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.frames)?)?;
        self.file.flush()?;
        self.file.sync_all()?;
        self.file.seek(SeekFrom::End(0))?;
        Ok(())
    }
    pub(crate) fn finalize(mut self) -> Result<AudioArtifact> {
        self.checkpoint()?;
        File::open(self.path.parent().context("audio has no parent")?)?.sync_all()?;
        Ok(AudioArtifact {
            path: self.path,
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS,
            frames: self.frames,
            data_offset: DATA_OFFSET,
            recovered: false,
        })
    }
}

fn validate_open_file(artifact: &AudioArtifact, file: &mut File) -> Result<()> {
    ensure!(
        artifact.sample_rate == SAMPLE_RATE
            && artifact.channels == CHANNELS
            && artifact.data_offset == DATA_OFFSET,
        "unsupported capture format"
    );
    let bytes = artifact
        .frames
        .checked_mul(4)
        .context("audio size overflow")?;
    ensure!(
        file.metadata()?.len()
            == bytes
                .checked_add(DATA_OFFSET)
                .context("file size overflow")?,
        "audio length differs from committed frames"
    );
    let mut stored_header = [0; DATA_OFFSET as usize];
    file.read_exact(&mut stored_header)?;
    ensure!(
        stored_header == header(artifact.frames)?,
        "invalid or stale RF64 header"
    );
    Ok(())
}

/// Validate the complete fixed RF64 header and length without decoding payload.
pub fn validate_artifact(artifact: &AudioArtifact) -> Result<()> {
    validate_open_file(artifact, &mut File::open(&artifact.path)?)
}

/// Read a finalized artifact. Header counts and file length must agree exactly.
pub fn read_f32(artifact: &AudioArtifact) -> Result<Vec<f32>> {
    read_f32_prefix(artifact, artifact.frames)
}

/// Read up to `max_frames` from a finalized artifact without buffering excess frames.
pub fn read_f32_prefix(artifact: &AudioArtifact, max_frames: u64) -> Result<Vec<f32>> {
    let mut file = File::open(&artifact.path)?;
    validate_open_file(artifact, &mut file)?;
    stream_f32_samples(&mut file, artifact.frames.min(max_frames))
}

/// A recovered prefix is read without trusting stale header counts or mutating the file.
pub fn read_recovered_f32(artifact: &AudioArtifact) -> Result<Vec<f32>> {
    read_recovered_prefix_f32(artifact, artifact.frames)
}

/// Read up to `max_frames` from a recovered prefix after validating container/format metadata.
pub fn read_recovered_prefix_f32(artifact: &AudioArtifact, max_frames: u64) -> Result<Vec<f32>> {
    ensure!(
        artifact.sample_rate == SAMPLE_RATE
            && artifact.channels == CHANNELS
            && artifact.data_offset == DATA_OFFSET,
        "unsupported recovered format"
    );
    let mut file = File::open(&artifact.path)?;
    let bytes = artifact
        .frames
        .checked_mul(4)
        .context("audio size overflow")?;
    ensure!(
        file.metadata()?.len()
            >= DATA_OFFSET
                .checked_add(bytes)
                .context("audio size overflow")?,
        "recovered prefix is shortened"
    );
    let mut stored_header = [0_u8; DATA_OFFSET as usize];
    file.read_exact(&mut stored_header)
        .context("read recovered RF64 header")?;
    let canonical = header(0)?;
    ensure!(
        stored_header[0..20] == canonical[0..20]
            && stored_header[44..80] == canonical[44..80]
            && stored_header[84..92] == canonical[84..92],
        "invalid recovered RF64 container or format"
    );
    stream_f32_samples(&mut file, artifact.frames.min(max_frames))
}

fn stream_f32_samples(file: &mut File, frames: u64) -> Result<Vec<f32>> {
    let bytes = frames.checked_mul(4).context("audio size overflow")?;
    let sample_count = usize::try_from(frames).context("audio exceeds addressable memory")?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .context("audio allocation failed")?;
    let mut batch = [0_u8; 16_384];
    let mut remaining = bytes;
    while remaining > 0 {
        let n = usize::try_from(remaining.min(batch.len() as u64))?;
        file.read_exact(&mut batch[..n])?;
        for b in batch[..n].chunks_exact(4) {
            samples.push(f32::from_bits(u32::from_le_bytes(b.try_into().unwrap())));
        }
        remaining -= n as u64;
    }
    Ok(samples)
}

pub(crate) fn available_frames(path: &Path) -> Result<u64> {
    let length = std::fs::metadata(path)?.len();
    if length < DATA_OFFSET {
        bail!("audio is shorter than its stored data offset");
    }
    Ok((length - DATA_OFFSET) / 4)
}

/// Convert 48 kHz mono float32 capture samples into a compact 16 kHz mono PCM16 RIFF WAV
/// for browser/WebView preview playback without touching the retained capture artifact.
pub fn to_16k_mono_pcm16_wav(samples_48k: &[f32], max_bytes: usize) -> Result<Vec<u8>> {
    let out_frames = samples_48k.len() / 3;
    let data_bytes = out_frames
        .checked_mul(2)
        .context("preview WAV size overflow")?;
    let total_bytes = 44_usize
        .checked_add(data_bytes)
        .context("preview WAV total overflow")?;
    ensure!(
        total_bytes <= max_bytes,
        "recording exceeds audio preview limit"
    );
    let data_len = u32::try_from(data_bytes).context("preview WAV exceeds RIFF size limit")?;
    let riff_len = 36_u32
        .checked_add(data_len)
        .context("preview WAV exceeds RIFF size limit")?;
    let mut wav = Vec::with_capacity(total_bytes);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_len.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&16_000_u32.to_le_bytes());
    wav.extend_from_slice(&(16_000_u32 * 2).to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for chunk in samples_48k.chunks_exact(3) {
        let clamped_avg = chunk
            .iter()
            .copied()
            .map(|s| {
                if s.is_finite() {
                    s.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            })
            .sum::<f32>()
            / 3.0;
        let pcm = (clamped_avg * i16::MAX as f32).round() as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_uses_64_bit_sizes() {
        let frames = u32::MAX as u64 + 42;
        let h = header(frames).unwrap();
        assert_eq!(
            u64::from_le_bytes(h[28..36].try_into().unwrap()),
            frames * 4
        );
        assert_eq!(u64::from_le_bytes(h[36..44].try_into().unwrap()), frames);
        assert_eq!(&h[88..92], &u32::MAX.to_le_bytes());
    }
}
