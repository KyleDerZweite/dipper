//! Versioned, single-job transport shared by Recognition and its native worker.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const REQUEST_LIMIT: usize = 64 * 1024;
pub const EVENT_LIMIT: usize = 8 * 1024 * 1024;
pub const TRANSCRIPT_LIMIT: usize = 1024 * 1024;
pub const DIAGNOSTIC_TAIL_LIMIT: usize = 128 * 1024;
pub const WHISPER_REVISION: &str = "0c7be696371069e0baa2515ed7aa08d5a5473aa6";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobIdentity {
    pub generation: u64,
    pub recording_id: Uuid,
    pub attempt_id: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioSource {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
    pub data_offset: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionSettings {
    pub model_path: PathBuf,
    pub language: String,
    pub beam_size: u32,
    pub cpu_threads: u32,
    pub required_gpu: String,
    pub runtime_revision: String,
}

impl RecognitionSettings {
    pub fn large_v3(model_path: PathBuf) -> Self {
        Self {
            model_path,
            language: "de".into(),
            beam_size: 5,
            cpu_threads: 8,
            required_gpu: "AMD Radeon RX 7700S".into(),
            runtime_revision: WHISPER_REVISION.into(),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.runtime_revision == WHISPER_REVISION,
            "unsupported native runtime revision"
        );
        anyhow::ensure!(
            self.language == "de" && self.beam_size == 5 && self.cpu_threads == 8,
            "first-slice recognition settings must use German, beam size five and eight threads"
        );
        anyhow::ensure!(
            self.required_gpu.contains("RX 7700S"),
            "first-slice GPU must be RX 7700S"
        );
        anyhow::ensure!(self.model_path.is_absolute(), "model path must be absolute");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    pub version: u32,
    pub identity: JobIdentity,
    pub settings: RecognitionSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestFrame {
    pub version: u32,
    pub identity: JobIdentity,
    pub audio: AudioSource,
}

impl RequestFrame {
    pub fn validate(&self, identity: &JobIdentity) -> anyhow::Result<()> {
        anyhow::ensure!(self.version == PROTOCOL_VERSION, "unknown protocol version");
        anyhow::ensure!(&self.identity == identity, "request identity mismatch");
        anyhow::ensure!(self.audio.path.is_absolute(), "audio path must be absolute");
        anyhow::ensure!(
            self.audio.sample_rate == 48_000 && self.audio.channels == 1,
            "model input source must be 48 kHz mono"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GpuIdentity {
    pub backend: String,
    pub name: String,
    pub description: String,
    pub pci_id: Option<String>,
    pub gpu_index: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecognitionTimings {
    pub preparation_ms: u64,
    pub input_preparation_ms: u64,
    pub inference_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactFingerprint {
    pub path: PathBuf,
    pub size_bytes: Option<u64>,
    pub modified_unix_ns: Option<u64>,
    pub sha256: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeProvenance {
    /// Source-declared version. Native binary identity is established by artifact hashes.
    pub engine_version: String,
    pub source_revision: String,
    pub model: ArtifactFingerprint,
    pub native_artifacts: Vec<ArtifactFingerprint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelInputProvenance {
    pub source_frames: u64,
    pub model_frames: u64,
    pub sample_rate: u32,
    pub channels: u16,
    pub resampler: String,
    pub input_padding_frames: u64,
    pub removed_output_delay_frames: u64,
    pub runtime: Option<RuntimeProvenance>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerEvent {
    Ready {
        gpu: GpuIdentity,
        preparation_ms: u64,
        runtime: RuntimeProvenance,
    },
    Started,
    Result {
        raw_transcript: String,
        provenance: ModelInputProvenance,
        timings: RecognitionTimings,
    },
    Failed {
        error: String,
        runtime: Option<RuntimeProvenance>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventFrame {
    pub version: u32,
    pub identity: JobIdentity,
    pub event: WorkerEvent,
}

impl EventFrame {
    pub fn new(identity: JobIdentity, event: WorkerEvent) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            identity,
            event,
        }
    }
}
