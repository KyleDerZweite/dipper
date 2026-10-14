use anyhow::{Context, Result, ensure};
use dipper_core::protocol::{GpuIdentity, RecognitionSettings, TRANSCRIPT_LIMIT};
use std::ffi::{CStr, CString, c_char, c_void};

#[repr(C)]
struct NativeGpu {
    backend: [c_char; 64],
    name: [c_char; 128],
    description: [c_char; 512],
    pci_id: [c_char; 128],
    gpu_index: u32,
}

unsafe extern "C" {
    fn dipper_whisper_prepare(
        model: *const c_char,
        required_gpu: *const c_char,
        gpu: *mut NativeGpu,
        error: *mut c_char,
        error_size: usize,
    ) -> *mut c_void;
    fn dipper_whisper_recognize(
        context: *mut c_void,
        samples: *const f32,
        count: i32,
        language: *const c_char,
        threads: i32,
        beam: i32,
        text: *mut c_char,
        text_size: usize,
        error: *mut c_char,
        error_size: usize,
    ) -> i32;
    fn dipper_whisper_free(context: *mut c_void);
}

pub struct NativeRecognition {
    context: *mut c_void,
}

fn native_text(bytes: &[c_char]) -> Result<String> {
    // Bridge guarantees NUL termination within each provided buffer.
    let value = unsafe { CStr::from_ptr(bytes.as_ptr()) };
    Ok(value
        .to_str()
        .context("native text is not UTF-8")?
        .to_owned())
}

impl NativeRecognition {
    pub fn prepare(settings: &RecognitionSettings) -> Result<(Self, GpuIdentity)> {
        settings.validate()?;
        use std::os::unix::ffi::OsStrExt;
        let model = CString::new(settings.model_path.as_os_str().as_bytes())?;
        let gpu_required = CString::new(settings.required_gpu.as_str())?;
        let mut gpu = NativeGpu {
            backend: [0; 64],
            name: [0; 128],
            description: [0; 512],
            pci_id: [0; 128],
            gpu_index: 0,
        };
        let mut error = [0; 4096];
        // One worker owns one context and invokes it synchronously on this thread.
        let context = unsafe {
            dipper_whisper_prepare(
                model.as_ptr(),
                gpu_required.as_ptr(),
                &mut gpu,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        ensure!(!context.is_null(), "{}", native_text(&error)?);
        let recognition = Self { context };
        let pci_id = native_text(&gpu.pci_id)?;
        let identity = GpuIdentity {
            backend: native_text(&gpu.backend)?,
            name: native_text(&gpu.name)?,
            description: native_text(&gpu.description)?,
            pci_id: if pci_id.is_empty() {
                None
            } else {
                Some(pci_id)
            },
            gpu_index: gpu.gpu_index,
        };
        Ok((recognition, identity))
    }

    pub fn recognize(&mut self, samples: &[f32], settings: &RecognitionSettings) -> Result<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }
        let count = i32::try_from(samples.len())
            .context("model sample count exceeds Whisper integer limit")?;
        let language = CString::new(settings.language.as_str())?;
        let mut text = vec![0; TRANSCRIPT_LIMIT + 1];
        let mut error = [0; 4096];
        let status = unsafe {
            dipper_whisper_recognize(
                self.context,
                samples.as_ptr(),
                count,
                language.as_ptr(),
                i32::try_from(settings.cpu_threads)?,
                i32::try_from(settings.beam_size)?,
                text.as_mut_ptr(),
                text.len(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        ensure!(status == 0, "{}", native_text(&error)?);
        native_text(&text)
    }
}

impl Drop for NativeRecognition {
    fn drop(&mut self) {
        unsafe { dipper_whisper_free(self.context) };
    }
}
