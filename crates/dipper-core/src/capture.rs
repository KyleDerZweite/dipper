//! Input discovery and native PipeWire capture, independent of desktop controls.
use crate::journal::{CaptureOutcome, CaptureTicket};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::Read,
    process::{Command, Stdio},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

unsafe extern "C" {
    fn dipper_pipewire_version() -> *const std::ffi::c_char;
    fn dipper_pipewire_init();
    fn dipper_ring_new(capacity: u64) -> *mut std::ffi::c_void;
    fn dipper_ring_free(ring: *mut std::ffi::c_void);
    fn dipper_ring_push(ring: *mut std::ffi::c_void, samples: *const f32, count: u64) -> i32;
    fn dipper_ring_read(ring: *mut std::ffi::c_void, samples: *mut f32, capacity: u64) -> u64;
    fn dipper_ring_fault(ring: *mut std::ffi::c_void) -> i32;
    fn dipper_ring_fail(ring: *mut std::ffi::c_void, code: i32);
    fn dipper_ring_accepted(ring: *mut std::ffi::c_void) -> u64;
    fn dipper_ring_high_water(ring: *mut std::ffi::c_void) -> u64;
    fn dipper_ring_take_level_milli(ring: *mut std::ffi::c_void) -> u32;
    fn dipper_capture_open(serial: u64, ring: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn dipper_capture_close_with_stats(
        capture: *mut std::ffi::c_void,
        stats: *mut NativeCaptureStats,
    );
}

#[repr(C)]
#[derive(Clone, Copy)]
struct NativeCaptureStats {
    callbacks: u64,
    empty_buffers: u64,
    discont_flags: u32,
    states_mask: u32,
    negotiated_rate: u32,
    negotiated_channels: u32,
    core_error_res: i32,
    stream_error: [std::ffi::c_char; 256],
    core_error: [std::ffi::c_char; 256],
}

impl Default for NativeCaptureStats {
    fn default() -> Self {
        Self {
            callbacks: 0,
            empty_buffers: 0,
            discont_flags: 0,
            states_mask: 0,
            negotiated_rate: 0,
            negotiated_channels: 0,
            core_error_res: 0,
            stream_error: [0; 256],
            core_error: [0; 256],
        }
    }
}

pub(crate) struct SampleRing {
    raw: NonNull<std::ffi::c_void>,
    capacity: u64,
    pub written: AtomicU64,
}
// C atomics implement an SPSC ring. Only Capture produces and one writer consumes.
unsafe impl Send for SampleRing {}
unsafe impl Sync for SampleRing {}
impl SampleRing {
    pub(crate) fn new(capacity: u64) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            raw: NonNull::new(unsafe { dipper_ring_new(capacity) })
                .context("cannot allocate lock-free capture ring")?,
            capacity,
            written: AtomicU64::new(0),
        }))
    }
    pub(crate) fn capacity(&self) -> u64 {
        self.capacity
    }
    pub(crate) fn read(&self, samples: &mut [f32]) -> usize {
        unsafe {
            dipper_ring_read(
                self.raw.as_ptr(),
                samples.as_mut_ptr(),
                samples.len() as u64,
            ) as usize
        }
    }
    pub(crate) fn fault(&self) -> Option<String> {
        match unsafe { dipper_ring_fault(self.raw.as_ptr()) } {
            0 => None,
            1 => Some("capture queue overflow".into()),
            2 => Some("input device disappeared".into()),
            3 => Some("PipeWire stream or connection failed".into()),
            4 => Some("invalid capture sample layout".into()),
            5 => Some("capture audio discontinuity".into()),
            6 => Some("capture abandoned".into()),
            _ => Some("capture storage failed".into()),
        }
    }
    pub(crate) fn fail_storage(&self) {
        unsafe { dipper_ring_fail(self.raw.as_ptr(), 7) }
    }
    pub(crate) fn accepted(&self) -> u64 {
        unsafe { dipper_ring_accepted(self.raw.as_ptr()) }
    }
    pub(crate) fn high_water(&self) -> u64 {
        unsafe { dipper_ring_high_water(self.raw.as_ptr()) }
    }
    pub(crate) fn take_audio_level(&self) -> f32 {
        let milli = unsafe { dipper_ring_take_level_milli(self.raw.as_ptr()) };
        ((milli as f32) / 1000.0).sqrt().clamp(0.0, 1.0)
    }
    pub(crate) fn push_synthetic(&self, samples: &[f32]) -> bool {
        unsafe { dipper_ring_push(self.raw.as_ptr(), samples.as_ptr(), samples.len() as u64) != 0 }
    }
}
impl Drop for SampleRing {
    fn drop(&mut self) {
        unsafe { dipper_ring_free(self.raw.as_ptr()) }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputDevice {
    pub name: String,
    pub description: String,
    pub serial: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputCatalog {
    pub default: Option<String>,
    pub devices: Vec<InputDevice>,
}
impl InputCatalog {
    pub fn resolve(&self, selection: Option<&str>) -> Result<InputDevice> {
        let name = selection
            .or(self.default.as_deref())
            .context("no default input is available")?;
        self.devices
            .iter()
            .find(|d| d.name == name)
            .cloned()
            .context("selected input is unavailable")
    }
}
pub struct Capture;
impl Capture {
    pub fn backend_version() -> String {
        // PipeWire returns a static version string; this does not initialize capture.
        unsafe { std::ffi::CStr::from_ptr(dipper_pipewire_version()) }
            .to_string_lossy()
            .into_owned()
    }
    /// Inventory only. This command does not open a microphone stream.
    pub fn inputs() -> Result<InputCatalog> {
        let mut child = Command::new("pw-dump")
            .arg("--no-colors")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("cannot start PipeWire input discovery")?;
        let stdout = child.stdout.take().context("missing pw-dump stdout")?;
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(error.into());
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                bail!("PipeWire input discovery timed out");
            }
            thread::sleep(Duration::from_millis(10));
        };
        let bytes = reader
            .join()
            .map_err(|_| anyhow::anyhow!("PipeWire inventory reader failed"))??;
        ensure!(status.success(), "PipeWire input discovery failed");
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "PipeWire inventory exceeds limit"
        );
        Self::parse_inventory(&serde_json::from_slice(&bytes)?)
    }
    pub fn parse_inventory(value: &Value) -> Result<InputCatalog> {
        let objects = value.as_array().context("invalid PipeWire inventory")?;
        let mut catalog = InputCatalog {
            default: None,
            devices: vec![],
        };
        for object in objects {
            let props = &object["info"]["props"];
            if props["media.class"].as_str() == Some("Audio/Source")
                && let (Some(name), Some(serial)) = (
                    props["node.name"].as_str(),
                    props["object.serial"]
                        .as_u64()
                        .or_else(|| props["object.serial"].as_str()?.parse().ok()),
                )
            {
                catalog.devices.push(InputDevice {
                    name: name.to_owned(),
                    description: props["node.description"]
                        .as_str()
                        .unwrap_or(name)
                        .to_owned(),
                    serial,
                });
            }
            if let Some(metadata) = object["metadata"].as_array() {
                for item in metadata {
                    if item["key"].as_str() == Some("default.audio.source") {
                        let decoded = item["value"]
                            .as_str()
                            .and_then(|s| serde_json::from_str::<Value>(s).ok())
                            .unwrap_or_else(|| item["value"].clone());
                        catalog.default = decoded["name"].as_str().map(str::to_owned);
                    }
                }
            }
        }
        catalog
            .devices
            .sort_by(|a, b| a.description.cmp(&b.description).then(a.name.cmp(&b.name)));
        Ok(catalog)
    }
}

#[derive(Clone, Debug)]
pub enum CaptureStop {
    Released,
    Canceled,
    SessionLost,
    Failed(String),
}
impl CaptureStop {
    pub(crate) fn state(&self) -> &'static str {
        match self {
            Self::Released => "complete",
            Self::Canceled => "canceled",
            Self::SessionLost => "interrupted",
            Self::Failed(_) => "failed",
        }
    }
    pub(crate) fn error(&self) -> Option<String> {
        match self {
            Self::Failed(e) => Some(e.clone()),
            Self::SessionLost => Some("desktop session lost".into()),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureDiagnostics {
    pub duration_ms: u64,
    pub first_frame_ms: Option<u64>,
    pub callbacks: u64,
    pub empty_buffers: u64,
    pub ring_capacity_frames: u64,
    pub ring_high_water_frames: u64,
    pub stream_states: Vec<String>,
    pub negotiated_rate: Option<u32>,
    pub negotiated_channels: Option<u32>,
    pub discontinuity_flags: u32,
    pub pipewire_error: Option<String>,
    pub failure_category: Option<String>,
    pub failure_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaptureProgress {
    pub ready: bool,
    pub accepted_frames: u64,
    pub written_frames: u64,
    pub audio_level: f32,
    pub error: Option<String>,
}
pub struct CaptureSession {
    native: Option<NonNull<std::ffi::c_void>>,
    ticket: Option<CaptureTicket>,
    started_at: Instant,
    first_frame_ms: AtomicU64,
}
// Session owns the native handle. Access is serialized by its mutable stop/Drop.
unsafe impl Send for CaptureSession {}
impl CaptureSession {
    pub fn start(ticket: CaptureTicket, input: &InputDevice) -> Result<Self> {
        static INITIALIZE: std::sync::Once = std::sync::Once::new();
        INITIALIZE.call_once(|| unsafe { dipper_pipewire_init() });
        ensure!(
            ticket.input.serial == input.serial && ticket.input.name == input.name,
            "capture input differs from its durable identity"
        );
        let started_at = Instant::now();
        let ptr = unsafe { dipper_capture_open(input.serial, ticket.ring.raw.as_ptr()) };
        match NonNull::new(ptr) {
            Some(native) => Ok(Self {
                native: Some(native),
                ticket: Some(ticket),
                started_at,
                first_frame_ms: AtomicU64::new(u64::MAX),
            }),
            None => {
                let diag = CaptureDiagnostics {
                    duration_ms: started_at.elapsed().as_millis().min(u64::MAX as u128) as u64,
                    ring_capacity_frames: ticket.ring.capacity(),
                    pipewire_error: Some("cannot open native PipeWire capture".into()),
                    ..Default::default()
                };
                let _ = ticket.finish_with_diagnostics(
                    CaptureStop::Failed("cannot open native PipeWire capture".into()),
                    Some(diag),
                );
                bail!("cannot open native PipeWire capture");
            }
        }
    }
    pub fn recording_id(&self) -> uuid::Uuid {
        self.ticket
            .as_ref()
            .expect("live capture ticket")
            .recording_id
    }
    pub fn progress(&self) -> CaptureProgress {
        let ring = &self.ticket.as_ref().expect("live capture ticket").ring;
        let accepted = ring.accepted();
        let ready = accepted > 0;
        if ready {
            let elapsed = self
                .started_at
                .elapsed()
                .as_millis()
                .min(u64::MAX as u128 - 1) as u64;
            let _ = self.first_frame_ms.compare_exchange(
                u64::MAX,
                elapsed,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        }
        CaptureProgress {
            ready,
            accepted_frames: accepted,
            written_frames: ring.written.load(Ordering::Acquire),
            audio_level: if ready { ring.take_audio_level() } else { 0.0 },
            error: ring.fault(),
        }
    }
    pub fn ready(&self) -> bool {
        self.progress().ready
    }
    pub fn stop(mut self, reason: CaptureStop) -> Result<CaptureOutcome> {
        let mut stats = NativeCaptureStats::default();
        if let Some(native) = self.native.take() {
            unsafe { dipper_capture_close_with_stats(native.as_ptr(), &mut stats) }
        }
        let ticket = self.ticket.take().expect("live capture ticket");
        let first_frame = match self.first_frame_ms.load(Ordering::Acquire) {
            u64::MAX => None,
            value => Some(value),
        };
        let diag = build_diagnostics(self.started_at.elapsed(), first_frame, &ticket.ring, &stats);
        ticket.finish_with_diagnostics(reason, Some(diag))
    }
}
impl Drop for CaptureSession {
    fn drop(&mut self) {
        let mut stats = NativeCaptureStats::default();
        if let Some(native) = self.native.take() {
            unsafe { dipper_capture_close_with_stats(native.as_ptr(), &mut stats) }
        }
        if let Some(ticket) = self.ticket.take() {
            let diag = build_diagnostics(
                self.started_at.elapsed(),
                match self.first_frame_ms.load(Ordering::Acquire) {
                    u64::MAX => None,
                    value => Some(value),
                },
                &ticket.ring,
                &stats,
            );
            let _ = ticket.finish_with_diagnostics(
                CaptureStop::Failed("capture session abandoned".into()),
                Some(diag),
            );
        }
    }
}

fn cstr_to_option(buf: &[std::ffi::c_char]) -> Option<String> {
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    if nul == 0 {
        return None;
    }
    let bytes: Vec<u8> = buf[..nul].iter().map(|&b| b as u8).collect();
    let s = String::from_utf8_lossy(&bytes).trim().to_owned();
    if s.is_empty() { None } else { Some(s) }
}

fn build_diagnostics(
    elapsed: Duration,
    first_frame_ms: Option<u64>,
    ring: &SampleRing,
    stats: &NativeCaptureStats,
) -> CaptureDiagnostics {
    let mut stream_states = Vec::new();
    for (bit, label) in [
        (1u32 << 1, "unconnected"),
        (1u32 << 2, "connecting"),
        (1u32 << 3, "paused"),
        (1u32 << 4, "streaming"),
        (1u32 << 0, "error"),
    ] {
        if stats.states_mask & bit != 0 {
            stream_states.push(label.to_owned());
        }
    }
    let pipewire_error = match (
        cstr_to_option(&stats.stream_error),
        cstr_to_option(&stats.core_error),
        stats.core_error_res,
    ) {
        (Some(s), Some(c), res) => Some(format!("stream: {s}; core ({res}): {c}")),
        (Some(s), None, _) => Some(format!("stream: {s}")),
        (None, Some(c), res) => Some(format!("core ({res}): {c}")),
        (None, None, res) if res < 0 => Some(format!("core error {res}")),
        _ => None,
    };
    CaptureDiagnostics {
        duration_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        first_frame_ms,
        callbacks: stats.callbacks,
        empty_buffers: stats.empty_buffers,
        ring_capacity_frames: ring.capacity(),
        ring_high_water_frames: ring.high_water(),
        stream_states,
        negotiated_rate: (stats.negotiated_rate > 0).then_some(stats.negotiated_rate),
        negotiated_channels: (stats.negotiated_channels > 0).then_some(stats.negotiated_channels),
        discontinuity_flags: stats.discont_flags,
        pipewire_error,
        failure_category: None,
        failure_hint: None,
    }
}

/// Capture's platform seam. The native adapter and deterministic lifecycle fixtures
/// use the same durable ticket and completion contract.
pub trait CaptureProvider: Send + Sync {
    fn inputs(&self) -> Result<InputCatalog>;
    fn start(&self, ticket: CaptureTicket, input: &InputDevice) -> Result<Box<dyn CaptureRun>>;
}
pub trait CaptureRun: Send {
    fn progress(&self) -> CaptureProgress;
    fn stop(self: Box<Self>, reason: CaptureStop) -> Result<CaptureOutcome>;
}
pub struct NativeCaptureProvider;
impl CaptureProvider for NativeCaptureProvider {
    fn inputs(&self) -> Result<InputCatalog> {
        Capture::inputs()
    }
    fn start(&self, ticket: CaptureTicket, input: &InputDevice) -> Result<Box<dyn CaptureRun>> {
        Ok(Box::new(CaptureSession::start(ticket, input)?))
    }
}
impl CaptureRun for CaptureSession {
    fn progress(&self) -> CaptureProgress {
        CaptureSession::progress(self)
    }
    fn stop(self: Box<Self>, reason: CaptureStop) -> Result<CaptureOutcome> {
        CaptureSession::stop(*self, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spsc_wrap_preserves_order_and_overflow_accepts_no_partial_buffer() {
        let ring = SampleRing::new(8).unwrap();
        assert!(ring.push_synthetic(&[0.0, 1.0, 2.0, 3.0]));
        let mut first = [0.0; 2];
        assert_eq!(ring.read(&mut first), 2);
        assert_eq!(first, [0.0, 1.0]);
        assert!(ring.push_synthetic(&[4.0, 5.0, 6.0, 7.0, 8.0]));
        let mut rest = [0.0; 8];
        assert_eq!(ring.read(&mut rest), 7);
        assert_eq!(&rest[..7], &[2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        assert!(ring.push_synthetic(&[9.0; 4]));
        assert!(!ring.push_synthetic(&[10.0; 5]));
        assert_eq!(ring.accepted(), 13);
        assert_eq!(ring.read(&mut rest), 4);
        assert_eq!(&rest[..4], &[9.0; 4]);
        assert_eq!(ring.fault().as_deref(), Some("capture queue overflow"));
    }
}
