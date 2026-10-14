//! Native KDE Wayland interaction, independent of the Tauri shell.
mod accessibility;
mod clipboard;
mod observer;
mod shortcuts;

use anyhow::{Result, ensure};
use async_trait::async_trait;
use dipper_core::target::{
    DeliveryStatus, DispatchStage, Insertion, InsertionOutcome, TargetPermit,
};
use std::path::PathBuf;
use tokio::sync::{Mutex, broadcast};

pub const APP_ID: &str = "io.github.KyleDerZweite.dipper";

#[derive(Debug, Clone)]
pub enum DesktopEvent {
    HoldPressed,
    HoldReleased,
    SessionLost(String),
}

pub struct DesktopAdapter {
    observer: observer::Observer,
    events: broadcast::Sender<DesktopEvent>,
    shortcuts: Mutex<Option<shortcuts::HoldSession>>,
    clipboard: Mutex<clipboard::ClipboardOwner>,
    dispatch_lock: Mutex<()>,
}

impl DesktopAdapter {
    /// Starts the native focus observer. This never opens a permission dialog.
    pub async fn new(helper_path: PathBuf) -> Result<Self> {
        let (events, _) = broadcast::channel(32);
        let observer = observer::Observer::start(events.clone()).await?;
        Ok(Self {
            observer,
            events,
            shortcuts: Mutex::new(None),
            clipboard: Mutex::new(clipboard::ClipboardOwner::new(helper_path)),
            dispatch_lock: Mutex::new(()),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DesktopEvent> {
        self.events.subscribe()
    }

    /// Explicit settings action. KDE may display its shortcut permission dialog.
    pub async fn setup_hold(&self, preferred: &str) -> Result<String> {
        ensure!(
            !preferred.trim().is_empty() && preferred.len() <= 128,
            "Invalid preferred shortcut"
        );
        self.observer.ensure_healthy()?;
        let mut slot = self.shortcuts.lock().await;
        if let Some(old) = slot.take() {
            self.observer.revoke_all();
            old.stop().await?;
        }
        let session = shortcuts::HoldSession::start(
            preferred,
            self.events.clone(),
            self.observer.state.clone(),
        )
        .await?;
        let binding = session.binding.lock().unwrap().clone();
        *slot = Some(session);
        Ok(binding)
    }

    /// Restore an existing assignment without BindShortcuts or a permission dialog.
    pub async fn restore_hold(&self) -> Result<Option<String>> {
        self.observer.ensure_healthy()?;
        let mut slot = self.shortcuts.lock().await;
        if let Some(session) = slot.as_ref() {
            let binding = session.binding.lock().unwrap().clone();
            if !binding.is_empty() {
                return Ok(Some(binding));
            }
        }
        if let Some(old) = slot.take() {
            old.stop().await?;
        }
        let session =
            shortcuts::HoldSession::restore(self.events.clone(), self.observer.state.clone())
                .await?;
        let binding = session.as_ref().map(|s| s.binding.lock().unwrap().clone());
        *slot = session;
        Ok(binding)
    }

    pub async fn effective_binding(&self) -> Option<String> {
        self.shortcuts
            .lock()
            .await
            .as_ref()
            .map(|s| s.binding.lock().unwrap().clone())
    }

    /// GlobalShortcuts v1 uses KDE's own shortcut configuration UI.
    pub async fn open_shortcut_settings(&self) -> Result<()> {
        let mut launcher = tokio::process::Command::new("systemsettings")
            .arg("kcm_keys")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // Reap the launcher without making settings-window lifetime a command deadline.
        // KDE settings is user-owned; stopping Dipper never kills it.
        tokio::spawn(async move {
            let _ = launcher.wait().await;
        });
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        self.observer.revoke_all();
        let mut failure = None;
        if let Some(session) = self.shortcuts.lock().await.take()
            && let Err(error) = session.stop().await
        {
            failure = Some(error);
        }
        let _guard = self.dispatch_lock.lock().await;
        if let Err(error) = self.clipboard.lock().await.stop().await {
            failure = Some(error);
        }
        if let Err(error) = self.observer.stop().await {
            failure = Some(error);
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(())
    }

    async fn validate(&self, permit: &TargetPermit) -> Result<()> {
        ensure!(
            permit.is_valid(),
            "Automatic insertion eligibility was revoked"
        );
        self.observer.ensure_healthy()?;
        let current = self.observer.snapshot().await?;
        ensure!(
            current.as_ref() == Some(&permit.identity),
            "Original T3 window is no longer active"
        );
        accessibility::check_composer(permit.identity.pid).await?;
        ensure!(
            permit.is_valid(),
            "Target changed during composer validation"
        );
        Ok(())
    }
}

fn outcome(
    stage: DispatchStage,
    status: DeliveryStatus,
    detail: impl Into<String>,
) -> InsertionOutcome {
    InsertionOutcome {
        stage,
        status,
        detail: detail.into(),
    }
}

#[async_trait]
impl Insertion for DesktopAdapter {
    fn reserve(&self) -> Result<TargetPermit> {
        let identity = self.observer.current()?;
        ensure!(
            identity.application == "com.t3tools.T3Code",
            "The active window is not T3"
        );
        let permit = TargetPermit::new(identity);
        self.observer.track(permit.clone())?;
        Ok(permit)
    }

    async fn arm(&self, permit: TargetPermit) -> Result<TargetPermit> {
        if let Err(error) = self.validate(&permit).await {
            permit.revoke();
            return Err(error);
        }
        Ok(permit)
    }

    async fn dispatch(&self, permit: &TargetPermit, text: &str) -> InsertionOutcome {
        let _guard = self.dispatch_lock.lock().await;
        if text.is_empty() {
            return outcome(
                DispatchStage::Unchanged,
                DeliveryStatus::Suppressed,
                "No text to insert",
            );
        }
        if let Err(error) = self.validate(permit).await {
            permit.revoke();
            return outcome(
                DispatchStage::Unchanged,
                DeliveryStatus::Suppressed,
                error.to_string(),
            );
        }
        let mut owner = self.clipboard.lock().await;
        let publication = owner.publish(text, Some(permit)).await;
        if let Err(error) = publication {
            permit.revoke();
            return outcome(error.stage, DeliveryStatus::Failed, error.detail);
        }
        drop(owner);
        if let Err(error) = self.validate(permit).await {
            permit.revoke();
            return outcome(
                DispatchStage::ClipboardPublished,
                DeliveryStatus::Suppressed,
                error.to_string(),
            );
        }
        if let Err(error) = self.clipboard.lock().await.ensure_serving() {
            permit.revoke();
            return outcome(
                DispatchStage::ClipboardPublished,
                DeliveryStatus::Failed,
                error.to_string(),
            );
        }
        if !permit.is_valid() {
            return outcome(
                DispatchStage::ClipboardPublished,
                DeliveryStatus::Suppressed,
                "Insertion was canceled before Paste",
            );
        }
        // The OS does not accept an expected-window condition with these keys.
        // The remaining interval is the explicitly accepted pilot race.
        let result = shortcuts::paste(permit).await;
        match result {
            Ok(()) => outcome(
                DispatchStage::PasteDispatched,
                DeliveryStatus::Unverified,
                "Paste dispatched; T3 acceptance was not verified",
            ),
            Err(failure) => {
                permit.revoke();
                outcome(
                    if failure.dispatched {
                        DispatchStage::PasteDispatched
                    } else {
                        DispatchStage::ClipboardPublished
                    },
                    DeliveryStatus::Failed,
                    failure.error.to_string(),
                )
            }
        }
    }

    async fn copy_explicit(&self, text: &str) -> Result<()> {
        let _guard = self.dispatch_lock.lock().await;
        self.clipboard
            .lock()
            .await
            .publish(text, None)
            .await
            .map_err(|error| anyhow::anyhow!(error.detail))
    }
}

/// Entrypoint for the separate, app-owned clipboard helper executable.
#[doc(hidden)]
pub fn clipboard_helper() -> Result<()> {
    use std::io::{BufRead, Read, Write};
    use wl_clipboard_rs::{copy, paste};
    // The control reader exists before any native call, including preflight.
    let (request_send, request_receive) = std::sync::mpsc::sync_channel(1);
    let (publish_send, publish_receive) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result: Result<()> = (|| {
            let mut input = std::io::BufReader::new(std::io::stdin());
            let mut frame = Vec::new();
            (&mut input)
                .take(clipboard::MAX_REQUEST as u64 + 1)
                .read_until(b'\n', &mut frame)?;
            ensure!(
                frame.len() <= clipboard::MAX_REQUEST && frame.last() == Some(&b'\n'),
                "Invalid clipboard request size"
            );
            let request: clipboard::ClipboardRequest = serde_json::from_slice(&frame)?;
            ensure!(
                request.version == 1
                    && request.text.len() <= clipboard::MAX_TEXT
                    && request.token.len() <= 128,
                "Invalid clipboard request"
            );
            request_send.send(request)?;
            let mut command = Vec::new();
            (&mut input).take(16).read_until(b'\n', &mut command)?;
            ensure!(command == b"publish\n", "Invalid clipboard publish command");
            publish_send.send(())?;
            let mut byte = [0];
            let _ = input.read(&mut byte);
            Ok(())
        })();
        // EOF or deliberate stop terminates all native serving threads.
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    });
    let request = request_receive.recv()?;
    // Check native data-control availability without publishing or reading payloads.
    match paste::get_mime_types(paste::ClipboardType::Regular, paste::Seat::Unspecified) {
        Ok(_) | Err(paste::Error::ClipboardEmpty) => {}
        Err(error) => return Err(error.into()),
    }
    println!("ready");
    std::io::stdout().flush()?;
    publish_receive.recv()?;
    let token = request.token.clone();
    let verifier = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            if let Ok((mut pipe, _)) = paste::get_contents(
                paste::ClipboardType::Regular,
                paste::Seat::Unspecified,
                paste::MimeType::Specific(clipboard::OWNER_MIME),
            ) {
                let mut contents = Vec::new();
                if (&mut pipe).take(129).read_to_end(&mut contents).is_ok()
                    && contents == token.as_bytes()
                {
                    println!("published");
                    let _ = std::io::stdout().flush();
                    return;
                }
            }
            if std::time::Instant::now() >= deadline {
                std::process::exit(1);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });
    let mut options = copy::Options::new();
    options
        .foreground(true)
        .clipboard(copy::ClipboardType::Regular);
    let prepared = options.prepare_copy_multi(vec![
        copy::MimeSource {
            source: copy::Source::Bytes(request.text.into_bytes().into()),
            mime_type: copy::MimeType::Text,
        },
        copy::MimeSource {
            source: copy::Source::Bytes(request.token.into_bytes().into()),
            mime_type: copy::MimeType::Specific(clipboard::OWNER_MIME.into()),
        },
    ])?;
    prepared.serve()?;
    verifier
        .join()
        .map_err(|_| anyhow::anyhow!("Clipboard verifier failed"))?;
    Ok(())
}
