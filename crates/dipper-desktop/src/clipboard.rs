use anyhow::{Context, Result, ensure};
use dipper_core::target::{DispatchStage, TargetPermit};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin},
};

pub const MAX_TEXT: usize = 1024 * 1024;
pub const MAX_REQUEST: usize = 8 * MAX_TEXT;
pub const OWNER_MIME: &str = "application/x-dipper-clipboard-owner";

#[derive(Serialize, Deserialize)]
pub struct ClipboardRequest {
    pub version: u8,
    pub text: String,
    pub token: String,
}

pub(crate) struct PublishFailure {
    pub stage: DispatchStage,
    pub detail: String,
}

pub(crate) struct ClipboardOwner {
    path: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    counter: u64,
    retiring: Vec<Child>,
    #[cfg(test)]
    retirement_failure: bool,
}

impl ClipboardOwner {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            child: None,
            stdin: None,
            counter: 0,
            retiring: Vec::new(),
            #[cfg(test)]
            retirement_failure: false,
        }
    }

    pub async fn publish(
        &mut self,
        text: &str,
        permit: Option<&TargetPermit>,
    ) -> std::result::Result<(), PublishFailure> {
        let mut stage = DispatchStage::Unchanged;
        let result: Result<()> = async {
            ensure!(text.len() <= MAX_TEXT, "Transcript exceeds clipboard limit");
            if let Some(permit) = permit {
                ensure!(permit.is_valid(), "Insertion eligibility was revoked");
            }
            self.counter += 1;
            let request = ClipboardRequest {
                version: 1,
                text: text.into(),
                token: format!("{}-{}", std::process::id(), self.counter),
            };
            let mut child = tokio::process::Command::new(&self.path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .context("Clipboard helper executable is unavailable")?;
            let mut stdin = child
                .stdin
                .take()
                .context("Clipboard helper stdin missing")?;
            let stdout = child
                .stdout
                .take()
                .context("Clipboard helper stdout missing")?;
            let mut reader = BufReader::new(stdout);
            let mut frame = serde_json::to_vec(&request)?;
            ensure!(
                frame.len() <= MAX_REQUEST,
                "Clipboard request exceeds protocol limit"
            );
            frame.push(b'\n');
            let handshake: Result<()> = tokio::time::timeout(Duration::from_secs(6), async {
                stdin.write_all(&frame).await?;
                read_ack(&mut reader, b"ready\n").await?;
                if let Some(permit) = permit {
                    ensure!(
                        permit.is_valid(),
                        "Insertion canceled before clipboard publication"
                    );
                }
                // A failed publication acknowledgement cannot prove selection was unchanged.
                stage = DispatchStage::ClipboardPublished;
                stdin.write_all(b"publish\n").await?;
                read_ack(&mut reader, b"published\n").await?;
                Ok(())
            })
            .await
            .context("Clipboard publication timed out")
            .and_then(|result| result);
            if let Err(error) = handshake {
                drop(stdin);
                let cleanup = self.retire_new(child).await;
                return Err(error.context(format!("new helper cleanup: {cleanup:?}")));
            }
            // A new owner has published before the old helper is retired.
            if let Err(error) = self.stop().await {
                drop(stdin);
                let cleanup = self.retire_new(child).await;
                return Err(error.context(format!("published helper cleanup: {cleanup:?}")));
            }
            self.stdin = Some(stdin);
            self.child = Some(child);
            Ok(())
        }
        .await;
        result.map_err(|error| PublishFailure {
            stage,
            detail: format!("{error:#}"),
        })
    }

    pub fn ensure_serving(&mut self) -> Result<()> {
        let child = self
            .child
            .as_mut()
            .context("Clipboard supplier is unavailable")?;
        ensure!(
            child.try_wait()?.is_none(),
            "Clipboard ownership ended before Paste"
        );
        Ok(())
    }

    async fn retire_new(&mut self, mut child: Child) -> Result<()> {
        let result = settle_child(&mut child, true).await;
        if result.is_err() {
            self.retiring.push(child);
        }
        result
    }

    pub async fn stop(&mut self) -> Result<()> {
        #[cfg(test)]
        if std::mem::take(&mut self.retirement_failure) {
            anyhow::bail!("synthetic old helper retirement failure");
        }
        self.stdin.take(); // EOF stops only our helper, including blocked serving.
        let mut failures = Vec::new();
        if let Some(mut child) = self.child.take()
            && let Err(error) = settle_child(&mut child, false).await
        {
            self.child = Some(child); // Retain ownership for another cleanup attempt.
            failures.push(error.to_string());
        }
        for mut child in std::mem::take(&mut self.retiring) {
            if let Err(error) = settle_child(&mut child, true).await {
                self.retiring.push(child);
                failures.push(error.to_string());
            }
        }
        ensure!(
            failures.is_empty(),
            "Clipboard helper cleanup failed: {}",
            failures.join("; ")
        );
        Ok(())
    }
}

async fn settle_child(child: &mut Child, force: bool) -> Result<()> {
    if force {
        let _ = child.start_kill();
    }
    match tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        Ok(result) => {
            result.context("Reap clipboard helper")?;
        }
        Err(_) => {
            child.start_kill().context("Terminate clipboard helper")?;
            tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .context("Clipboard helper reaping remains unconfirmed")??;
        }
    }
    Ok(())
}

async fn read_ack(
    reader: &mut BufReader<tokio::process::ChildStdout>,
    expected: &[u8],
) -> Result<()> {
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let available = reader.fill_buf().await?;
            ensure!(
                !available.is_empty(),
                "Clipboard helper exited before acknowledgement"
            );
            let length = available
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(available.len());
            ensure!(
                bytes.len() + length <= 128,
                "Clipboard helper acknowledgement exceeds bound"
            );
            bytes.extend_from_slice(&available[..length]);
            reader.consume(length);
            if bytes.last() == Some(&b'\n') {
                break;
            }
        }
        ensure!(bytes == expected, "Clipboard helper rejected publication");
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Clipboard helper acknowledgement timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn helper(directory: &tempfile::TempDir, content: &str) -> PathBuf {
        let path = directory.path().join("helper");
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    #[tokio::test]
    async fn owned_helper_stops_and_is_reaped() {
        let directory = tempfile::tempdir().unwrap();
        let path = helper(
            &directory,
            "#!/bin/sh\nread -r request\nprintf 'ready\\n'\nread -r publish\nprintf 'published\\n'\ncat >/dev/null\n",
        );
        let mut owner = ClipboardOwner::new(path);
        owner
            .publish("Grüße\nsecond line", None)
            .await
            .unwrap_or_else(|error| panic!("{}", error.detail));
        assert!(owner.child.is_some());
        owner.stop().await.unwrap();
        assert!(owner.child.is_none());
        assert!(owner.stdin.is_none());
    }
    #[tokio::test]
    async fn old_retirement_failure_reaps_new_published_helper() {
        let directory = tempfile::tempdir().unwrap();
        let pids = directory.path().join("pids");
        let quoted = format!("'{}'", pids.display().to_string().replace('\'', "'\\''"));
        let path = helper(
            &directory,
            &format!(
                "#!/bin/sh\nread -r request\nprintf 'ready\\n'\nread -r publish\nprintf '%s\\n' \"$$\" >> {}\nprintf 'published\\n'\nexec cat >/dev/null\n",
                quoted
            ),
        );
        let mut owner = ClipboardOwner::new(path);
        owner
            .publish("first", None)
            .await
            .unwrap_or_else(|error| panic!("{}", error.detail));
        let original_pid = owner.child.as_ref().unwrap().id().unwrap();
        owner.retirement_failure = true;
        let failed = owner.publish("second", None).await.err().unwrap();
        assert_eq!(failed.stage, DispatchStage::ClipboardPublished);
        assert!(failed.detail.contains("retirement failure"));
        assert_eq!(owner.child.as_ref().unwrap().id(), Some(original_pid));
        assert!(owner.retiring.is_empty());
        let recorded: Vec<u32> = std::fs::read_to_string(pids)
            .unwrap()
            .lines()
            .map(|line| line.parse().unwrap())
            .collect();
        assert_eq!(recorded.len(), 2);
        assert_ne!(recorded[1], original_pid);
        // The newly published helper was waited/reaped rather than dropped with kill_on_drop.
        assert_eq!(
            unsafe { libc::waitpid(recorded[1] as i32, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
        owner.stop().await.unwrap();
        assert!(owner.child.is_none());
        assert!(owner.retiring.is_empty());
    }

    #[tokio::test]
    async fn publication_failure_retains_irreversible_stage() {
        let directory = tempfile::tempdir().unwrap();
        let path = helper(
            &directory,
            "#!/bin/sh\nread -r request\nprintf 'ready\\n'\nread -r publish\nexit 1\n",
        );
        let mut owner = ClipboardOwner::new(path);
        let error = owner.publish("test", None).await.err().unwrap();
        assert_eq!(error.stage, DispatchStage::ClipboardPublished);
        assert!(owner.child.is_none());
    }
    #[tokio::test]
    async fn revoked_permit_and_oversize_never_start_helper() {
        let mut owner = ClipboardOwner::new(PathBuf::from("/missing/helper"));
        let permit = TargetPermit::new(dipper_core::target::TargetIdentity {
            window_id: "window".into(),
            pid: 1,
            application: "T3".into(),
        });
        permit.revoke();
        let revoked = owner.publish("test", Some(&permit)).await.err().unwrap();
        assert_eq!(revoked.stage, DispatchStage::Unchanged);
        let oversized = owner
            .publish(&"x".repeat(MAX_TEXT + 1), None)
            .await
            .err()
            .unwrap();
        assert_eq!(oversized.stage, DispatchStage::Unchanged);
        assert!(owner.child.is_none());
    }
    #[tokio::test]
    async fn cancellation_during_preparation_preserves_clipboard_stage() {
        let directory = tempfile::tempdir().unwrap();
        let started = directory.path().join("started");
        let gate = directory.path().join("gate");
        let quote = |path: &std::path::Path| {
            format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
        };
        let path = helper(
            &directory,
            &format!(
                "#!/bin/sh\nread -r request\ntouch {}\nwhile [ ! -e {} ]; do sleep .01; done\nprintf 'ready\\n'\nread -r publish\nprintf 'published\\n'\n",
                quote(&started),
                quote(&gate)
            ),
        );
        let permit = TargetPermit::new(dipper_core::target::TargetIdentity {
            window_id: "window".into(),
            pid: 1,
            application: "T3".into(),
        });
        let child_permit = permit.clone();
        let task = tokio::spawn(async move {
            let mut owner = ClipboardOwner::new(path);
            let result = owner.publish("test", Some(&child_permit)).await;
            (owner, result)
        });
        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        permit.revoke();
        std::fs::write(&gate, []).unwrap();
        let (owner, result) = task.await.unwrap();
        assert_eq!(result.err().unwrap().stage, DispatchStage::Unchanged);
        assert!(owner.child.is_none());
    }

    #[tokio::test]
    async fn excessive_helper_output_is_bounded() {
        let directory = tempfile::tempdir().unwrap();
        let path = helper(
            &directory,
            "#!/bin/sh\nread -r request\nhead -c 1024 /dev/zero\n",
        );
        let mut owner = ClipboardOwner::new(path);
        let error = owner.publish("test", None).await.err().unwrap();
        assert_eq!(error.stage, DispatchStage::Unchanged);
        assert!(error.detail.contains("exceeds bound"));
    }
}
