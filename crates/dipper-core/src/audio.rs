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
    let mut file = File::open(&artifact.path)?;
    validate_open_file(artifact, &mut file)?;
    let bytes = artifact
        .frames
        .checked_mul(4)
        .context("audio size overflow")?;
    let sample_count =
        usize::try_from(artifact.frames).context("audio exceeds addressable memory")?;
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .context("audio allocation failed")?;
    let mut batch = [0; 16_384];
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

/// A recovered prefix is read without trusting or rewriting an interrupted header.
pub fn read_recovered_f32(artifact: &AudioArtifact) -> Result<Vec<f32>> {
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
    file.seek(SeekFrom::Start(DATA_OFFSET))?;
    let n = usize::try_from(bytes).context("recovered audio exceeds addressable memory")?;
    let mut payload = Vec::new();
    payload.try_reserve_exact(n)?;
    payload.resize(n, 0);
    file.read_exact(&mut payload)?;
    Ok(payload
        .chunks_exact(4)
        .map(|b| f32::from_bits(u32::from_le_bytes(b.try_into().unwrap())))
        .collect())
}

pub(crate) fn available_frames(path: &Path) -> Result<u64> {
    let length = std::fs::metadata(path)?.len();
    if length < DATA_OFFSET {
        bail!("audio is shorter than its stored data offset");
    }
    Ok((length - DATA_OFFSET) / 4)
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
