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
    fn dipper_capture_open(serial: u64, ring: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn dipper_capture_close(capture: *mut std::ffi::c_void);
}

pub(crate) struct SampleRing {
    raw: NonNull<std::ffi::c_void>,
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
            written: AtomicU64::new(0),
        }))
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
#[derive(Clone, Debug, Serialize)]
pub struct CaptureProgress {
    pub ready: bool,
    pub accepted_frames: u64,
    pub written_frames: u64,
    pub error: Option<String>,
}
pub struct CaptureSession {
    native: Option<NonNull<std::ffi::c_void>>,
    ticket: Option<CaptureTicket>,
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
        let ptr = unsafe { dipper_capture_open(input.serial, ticket.ring.raw.as_ptr()) };
        match NonNull::new(ptr) {
            Some(native) => Ok(Self {
                native: Some(native),
                ticket: Some(ticket),
            }),
            None => {
                let _ = ticket.finish(CaptureStop::Failed(
                    "cannot open native PipeWire capture".into(),
                ));
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
        CaptureProgress {
            ready: ring.accepted() > 0,
            accepted_frames: ring.accepted(),
            written_frames: ring.written.load(Ordering::Acquire),
            error: ring.fault(),
        }
    }
    pub fn ready(&self) -> bool {
        self.progress().ready
    }
    pub fn stop(mut self, reason: CaptureStop) -> Result<CaptureOutcome> {
        if let Some(native) = self.native.take() {
            unsafe { dipper_capture_close(native.as_ptr()) }
        }
        self.ticket
            .take()
            .expect("live capture ticket")
            .finish(reason)
    }
}
impl Drop for CaptureSession {
    fn drop(&mut self) {
        if let Some(native) = self.native.take() {
            unsafe { dipper_capture_close(native.as_ptr()) }
        }
        if let Some(ticket) = self.ticket.take() {
            let _ = ticket.finish(CaptureStop::Failed("capture session abandoned".into()));
        }
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
