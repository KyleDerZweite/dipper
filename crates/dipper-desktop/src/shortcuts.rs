use crate::{APP_ID, DesktopEvent, observer};
use anyhow::{Context, Result, ensure};
use ashpd::desktop::{
    Session,
    global_shortcuts::{GlobalShortcuts, NewShortcut},
};
use futures_util::StreamExt;
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use zbus::{Connection, Proxy};

pub(crate) struct HoldSession {
    session: Session<GlobalShortcuts>,
    pub binding: Arc<Mutex<String>>,
    stop: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl HoldSession {
    pub async fn start(
        preferred: &str,
        events: broadcast::Sender<DesktopEvent>,
        state: Arc<Mutex<observer::State>>,
    ) -> Result<Self> {
        Self::open(Some(preferred), events, state)
            .await?
            .context("KDE did not assign a Hold shortcut")
    }

    pub async fn restore(
        events: broadcast::Sender<DesktopEvent>,
        state: Arc<Mutex<observer::State>>,
    ) -> Result<Option<Self>> {
        Self::open(None, events, state).await
    }

    async fn open(
        preferred: Option<&str>,
        events: broadcast::Sender<DesktopEvent>,
        state: Arc<Mutex<observer::State>>,
    ) -> Result<Option<Self>> {
        let connection = Connection::session().await?;
        ashpd::register_host_app_with_connection(
            connection.clone(),
            ashpd::AppID::try_from(APP_ID)?,
        )
        .await?;
        let portal = GlobalShortcuts::with_connection(connection.clone()).await?;
        let session = portal.create_session(Default::default()).await?;
        let path = serde_json::to_value(&session)?
            .as_str()
            .context("Invalid portal session handle")?
            .to_owned();
        // One ordered stream preserves fast press/release ordering across signal kinds.
        let mut signals = portal.receive_all_signals().await?;
        let session_proxy = Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            path.as_str(),
            "org.freedesktop.portal.Session",
        )
        .await?;
        let mut closed = session_proxy.receive_signal("Closed").await?;
        let dbus = Proxy::new(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        let mut owners = dbus.receive_signal("NameOwnerChanged").await?;
        let binding_result = async {
            if preferred.is_none() && listed_binding(&portal, &session).await?.is_none() {
                return Ok(None);
            }
            // KDE loads saved descriptions at CreateSession, but only BindShortcuts
            // registers the action with KGlobalAccel. Existing IDs restore without
            // a configuration dialog on our supported KDE backend. Do not reinterpret
            // localized trigger descriptions or supply a new preferred key on restore.
            tokio::time::timeout(
                Duration::from_secs(if preferred.is_some() { 120 } else { 5 }),
                portal.bind_shortcuts(
                    &session,
                    &[NewShortcut::new("hold", "Hold to dictate").preferred_trigger(preferred)],
                    None,
                    Default::default(),
                ),
            )
            .await
            .context("Shortcut setup timed out")??
            .response()?;
            listed_binding(&portal, &session).await
        }
        .await;
        let effective = match binding_result {
            Ok(Some(binding)) => binding,
            Ok(None) => {
                session.close().await?;
                return Ok(None);
            }
            Err(error) => {
                let _ = session.close().await;
                return Err(error);
            }
        };
        let binding = Arc::new(Mutex::new(effective));
        let task_binding = binding.clone();
        let stop = CancellationToken::new();
        let task_stop = stop.clone();
        let task = tokio::spawn(async move {
            let mut held = false;
            let reason = loop {
                tokio::select! {
                    _ = task_stop.cancelled() => break None,
                    event = signals.next() => match event {
                        Some(message) => {
                            let member = message.header().member().map(|m| m.as_str().to_owned());
                            match member.as_deref() {
                                Some("Activated" | "Deactivated") => {
                                    type KeySignal = (zbus::zvariant::OwnedObjectPath, String, u64,
                                        std::collections::HashMap<String, zbus::zvariant::OwnedValue>);
                                    let Ok((session_path, action, _, _)) = message.body().deserialize::<KeySignal>() else {
                                        break Some("Malformed shortcut signal");
                                    };
                                    if session_path.as_str() == path && action == "hold" {
                                        if member.as_deref() == Some("Activated") && !held {
                                            held = true;
                                            let _ = events.send(DesktopEvent::HoldPressed);
                                        } else if member.as_deref() == Some("Deactivated") && held {
                                            held = false;
                                            let _ = events.send(DesktopEvent::HoldReleased);
                                        }
                                    }
                                }
                                Some("ShortcutsChanged") => {
                                    let Ok((session_path, shortcuts)) = message.body().deserialize::<(
                                        zbus::zvariant::OwnedObjectPath,
                                        Vec<ashpd::desktop::global_shortcuts::Shortcut>
                                    )>() else { break Some("Malformed shortcut configuration signal"); };
                                    if session_path.as_str() == path {
                                        let found = shortcuts.iter().find(|s| s.id() == "hold")
                                            .map(|s| s.trigger_description().to_owned());
                                        if let Some(trigger) = found.filter(|s| !s.trim().is_empty()) {
                                            *task_binding.lock().unwrap() = trigger;
                                        } else { break Some("Hold shortcut was removed"); }
                                    }
                                }
                                _ => {},
                            }
                        }
                        None => break Some("Shortcut event stream ended"),
                    },
                    _ = closed.next() => break Some("Global shortcut session closed"),
                    event = owners.next() => match event {
                        Some(message) => {
                            if let Ok((name, _, _)) = message.body().deserialize::<(String,String,String)>()
                                && name == "org.freedesktop.portal.Desktop" { break Some("Desktop portal owner changed"); }
                        }
                        None => break Some("Shortcut session bus disconnected"),
                    },
                }
            };
            if let Some(reason) = reason {
                state.lock().unwrap().revoke();
                task_binding.lock().unwrap().clear();
                let _ = events.send(DesktopEvent::SessionLost(reason.into()));
            }
        });
        Ok(Some(Self {
            session,
            binding,
            stop,
            task,
        }))
    }

    pub async fn stop(self) -> Result<()> {
        self.stop.cancel();
        // Explicit close is handled locally. KDE does not acknowledge it with Closed.
        let closed = tokio::time::timeout(Duration::from_secs(3), self.session.close()).await;
        self.task.await?;
        closed.context("Shortcut session close timed out")??;
        Ok(())
    }
}

pub(crate) struct PasteFailure {
    pub dispatched: bool,
    pub error: anyhow::Error,
}

pub(crate) async fn paste(
    permit: &dipper_core::target::TargetPermit,
) -> std::result::Result<(), PasteFailure> {
    let mut command = tokio::process::Command::new("ydotool");
    command
        .args(["key", "--key-delay=10", "29:1", "47:1", "47:0", "29:0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if !permit.is_valid() {
        return Err(PasteFailure {
            dispatched: false,
            error: anyhow::anyhow!("Insertion eligibility was revoked before Paste"),
        });
    }
    let mut child = command.spawn().map_err(|error| PasteFailure {
        dispatched: false,
        error: anyhow::anyhow!("Existing ydotool client/daemon is required: {error}"),
    })?;
    permit.revoke(); // One admitted external dispatch consumes the permit.
    let result = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
    match result {
        Ok(Ok(status)) if status.success() => Ok(()),
        other => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            // Only release keys introduced by the owned Paste sequence. This is not a retry.
            let cleanup = async {
                let mut release = tokio::process::Command::new("ydotool")
                    .args(["key", "47:0", "29:0"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()?;
                if tokio::time::timeout(Duration::from_secs(2), release.wait())
                    .await
                    .is_err()
                {
                    let _ = release.kill().await;
                    release.wait().await?;
                }
                Ok::<_, anyhow::Error>(())
            }
            .await;
            Err(PasteFailure {
                dispatched: true,
                error: anyhow::anyhow!(
                    "ydotool Paste failed or timed out: {other:?}; key release: {cleanup:?}"
                ),
            })
        }
    }
}

async fn listed_binding(
    portal: &GlobalShortcuts,
    session: &Session<GlobalShortcuts>,
) -> Result<Option<String>> {
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        portal.list_shortcuts(session, Default::default()),
    )
    .await
    .context("Shortcut listing timed out")??
    .response()?;
    assigned_hold(
        response
            .shortcuts()
            .iter()
            .filter(|s| s.id() == "hold")
            .map(|s| s.trigger_description()),
    )
}

fn assigned_hold<'a>(mut triggers: impl Iterator<Item = &'a str>) -> Result<Option<String>> {
    let first = triggers.next();
    ensure!(triggers.next().is_none(), "KDE Hold shortcut is ambiguous");
    Ok(first.filter(|s| !s.trim().is_empty()).map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_unique_nonempty_saved_hold_is_restored_without_reinterpreting_its_key() {
        assert_eq!(
            assigned_hold(["Strg+F2"].into_iter()).unwrap(),
            Some("Strg+F2".into())
        );
        assert!(assigned_hold([].into_iter()).unwrap().is_none());
        assert!(assigned_hold(["  "].into_iter()).unwrap().is_none());
        assert!(assigned_hold(["F2", "F3"].into_iter()).is_err());
    }
}
