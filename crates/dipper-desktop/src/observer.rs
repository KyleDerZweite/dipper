use crate::DesktopEvent;
use anyhow::{Context, Result, ensure};
use dipper_core::target::{TargetIdentity, TargetPermit, WeakTargetPermit};
use futures_util::StreamExt;
use serde::Deserialize;
use std::{
    collections::HashMap,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, broadcast, oneshot};
use tokio_util::sync::CancellationToken;
use zbus::{Connection, Proxy, message::Header};

const SCRIPT: &str = "org.dipper.Dipper.FocusObserver";
const OBJECT: &str = "/org/dipper/Observer";
const INTERFACE: &str = "org.dipper.Observer";

#[derive(Default)]
pub(crate) struct State {
    pub healthy: bool,
    current: Option<TargetIdentity>,
    permits: Vec<WeakTargetPermit>,
    sequence: u64,
    pending: HashMap<u64, oneshot::Sender<Option<TargetIdentity>>>,
}

impl State {
    fn observe(&mut self, current: Option<TargetIdentity>) {
        self.permits.retain(|weak| {
            if let Some(permit) = weak.upgrade() {
                if current.as_ref() != Some(&permit.identity) { permit.revoke(); }
                permit.is_valid()
            } else { false }
        });
        self.current = current;
    }
    pub(crate) fn revoke(&mut self) {
        for weak in &self.permits { if let Some(permit) = weak.upgrade() { permit.revoke(); } }
    }
}

pub(crate) fn lost(
    state: &Arc<Mutex<State>>,
    events: &broadcast::Sender<DesktopEvent>,
    reason: &str,
) {
    let mut state = state.lock().unwrap();
    state.healthy = false;
    state.revoke();
    drop(state);
    let _ = events.send(DesktopEvent::SessionLost(reason.to_owned()));
}

#[derive(Deserialize)]
struct Window {
    id: String,
    pid: u32,
    app: String,
    normal: bool,
}
fn parse(data: &str) -> Result<Option<TargetIdentity>> {
    ensure!(data.len() <= 4096, "KWin report exceeds bound");
    let window: Option<Window> = serde_json::from_str(data)?;
    Ok(window.filter(|w| w.normal).map(|w| TargetIdentity {
        window_id: w.id,
        pid: w.pid,
        application: w.app,
    }))
}

struct Receiver {
    state: Arc<Mutex<State>>,
    events: broadcast::Sender<DesktopEvent>,
    sender: String,
    initialized: Arc<Notify>,
}

#[zbus::interface(name = "org.dipper.Observer")]
impl Receiver {
    fn report(
        &self,
        sequence: &str,
        data: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        if header.sender().map(|s| s.as_str()) != Some(self.sender.as_str()) {
            return Err(zbus::fdo::Error::AccessDenied(
                "Unexpected observer sender".into(),
            ));
        }
        let sequence: u64 = sequence.parse().map_err(|_| {
            lost(&self.state, &self.events, "Invalid KWin observer sequence");
            zbus::fdo::Error::Failed("Invalid sequence".into())
        })?;
        let current = parse(data).map_err(|e| {
            lost(&self.state, &self.events, "Malformed KWin observer report");
            zbus::fdo::Error::Failed(e.to_string())
        })?;
        let mut state = self.state.lock().unwrap();
        if sequence != state.sequence + 1 {
            state.healthy = false;
            state.revoke();
            let _ = self.events.send(DesktopEvent::SessionLost(
                "KWin observer sequence discontinuity".into(),
            ));
            return Err(zbus::fdo::Error::Failed(
                "Observer sequence discontinuity".into(),
            ));
        }
        state.sequence = sequence;
        state.observe(current);
        if sequence == 1 {
            state.healthy = true;
            self.initialized.notify_one();
        }
        Ok(())
    }

    fn snapshot(
        &self,
        request: &str,
        data: &str,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        if header.sender().map(|s| s.as_str()) != Some(self.sender.as_str()) {
            return Err(zbus::fdo::Error::AccessDenied(
                "Unexpected snapshot sender".into(),
            ));
        }
        let request: u64 = request
            .parse()
            .map_err(|_| zbus::fdo::Error::Failed("Invalid snapshot request".into()))?;
        let current = parse(data).map_err(|e| {
            lost(&self.state, &self.events, "Malformed KWin observer report");
            zbus::fdo::Error::Failed(e.to_string())
        })?;
        let mut state = self.state.lock().unwrap();
        state.observe(current.clone());
        if let Some(reply) = state.pending.remove(&request) {
            let _ = reply.send(current);
        }
        Ok(())
    }
}

pub(crate) struct Observer {
    pub state: Arc<Mutex<State>>,
    connection: Connection,
    path: PathBuf,
    runtime: PathBuf,
    service: String,
    stop: CancellationToken,
    monitor: Mutex<Option<tokio::task::JoinHandle<()>>>,
    request: std::sync::atomic::AtomicU64,
    snapshot_lock: tokio::sync::Mutex<()>,
}

const INFO: &str = "function info(w){return w ? {id:String(w.internalId),pid:w.pid,app:String(w.desktopFileName),normal:w.normalWindow}:null;}";

impl Observer {
    pub async fn start(events: broadcast::Sender<DesktopEvent>) -> Result<Self> {
        let connection = Connection::session().await?;
        let dbus = Proxy::new(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        let sender: String = dbus.call("GetNameOwner", &("org.kde.KWin",)).await?;
        let mut owners = dbus.receive_signal("NameOwnerChanged").await?;
        let service = format!("org.dipper.Observer.p{}", std::process::id());
        connection.request_name(service.as_str()).await?;
        let state = Arc::new(Mutex::new(State::default()));
        let initialized = Arc::new(Notify::new());
        connection
            .object_server()
            .at(
                OBJECT,
                Receiver {
                    state: state.clone(),
                    events: events.clone(),
                    sender,
                    initialized: initialized.clone(),
                },
            )
            .await?;
        let runtime = PathBuf::from(
            std::env::var_os("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR is unavailable")?,
        )
        .join("dipper");
        std::fs::create_dir_all(&runtime)?;
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))?;
        let path = runtime.join(format!("kwin-observer-{}.js", std::process::id()));
        let script = format!(
            "{INFO}\nvar sequence=0; function report(){{callDBus({}, {}, {}, 'Report', String(++sequence), JSON.stringify(info(workspace.activeWindow)));}}\nworkspace.windowActivated.connect(report); workspace.windowRemoved.connect(report); report();",
            serde_json::to_string(&service)?,
            serde_json::to_string(OBJECT)?,
            serde_json::to_string(INTERFACE)?
        );
        write_script(&path, &script)?;
        let scripting = Proxy::new(
            &connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        // Single-instance Dipper owns precisely this stable script name, including stale runs.
        let loaded: bool = scripting.call("isScriptLoaded", &(SCRIPT,)).await?;
        if loaded {
            let _: bool = scripting.call("unloadScript", &(SCRIPT,)).await?;
        }
        let start = load_run(&connection, &path, SCRIPT).await;
        if let Err(error) = start {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        if tokio::time::timeout(Duration::from_secs(3), initialized.notified())
            .await
            .is_err()
        {
            let _: Result<bool, _> = scripting.call("unloadScript", &(SCRIPT,)).await;
            let _ = std::fs::remove_file(&path);
            anyhow::bail!("KWin observer did not initialize");
        }
        let stop = CancellationToken::new();
        let task_stop = stop.clone();
        let task_state = state.clone();
        let health_connection = connection.clone();
        let monitor = tokio::spawn(async move {
            let mut health_tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = task_stop.cancelled() => break,
                    _ = health_tick.tick() => {
                        // Only armed dictations incur a small script-health check.
                        let armed = task_state.lock().unwrap().permits.iter().any(TargetPermit::is_valid);
                        if armed && observer_loaded(&health_connection).await.is_err() {
                            lost(&task_state, &events, "KWin observer script is unavailable");
                            break;
                        }
                    }
                    owner = owners.next() => {
                        match owner {
                            Some(message) => {
                                if let Ok((name, _, _)) = message.body().deserialize::<(String,String,String)>()
                                    && name == "org.kde.KWin" {
                                    lost(&task_state, &events, "KWin connection or owner changed");
                                    break;
                                }
                            }
                            None => { lost(&task_state, &events, "Desktop session bus disconnected"); break; }
                        }
                    }
                }
            }
        });
        Ok(Self {
            state,
            connection,
            path,
            runtime,
            service,
            stop,
            monitor: Mutex::new(Some(monitor)),
            request: std::sync::atomic::AtomicU64::new(0),
            snapshot_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn ensure_healthy(&self) -> Result<()> {
        ensure!(
            self.state.lock().unwrap().healthy,
            "KWin observer is unavailable"
        );
        Ok(())
    }
    pub fn current(&self) -> Result<TargetIdentity> {
        self.ensure_healthy()?;
        self.state
            .lock()
            .unwrap()
            .current
            .clone()
            .context("No active target window")
    }
    pub fn track(&self, permit: TargetPermit) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        ensure!(
            state.healthy && state.current.as_ref() == Some(&permit.identity),
            "Target changed before admission"
        );
        state.permits.retain(|weak| weak.upgrade().is_some_and(|permit| permit.is_valid()));
        state.permits.push(permit.downgrade());
        Ok(())
    }
    pub fn revoke_all(&self) {
        self.state.lock().unwrap().revoke();
    }

    pub async fn snapshot(&self) -> Result<Option<TargetIdentity>> {
        let _guard = self.snapshot_lock.lock().await;
        self.ensure_healthy()?;
        if let Err(error) = observer_loaded(&self.connection).await {
            self.revoke_all();
            return Err(error);
        }
        let request = self
            .request
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let (send, receive) = oneshot::channel();
        self.state.lock().unwrap().pending.insert(request, send);
        let name = format!(
            "org.dipper.Dipper.Snapshot.p{}.r{request}",
            std::process::id()
        );
        let path = self.runtime.join(format!("snapshot-{request}.js"));
        let script = format!(
            "{INFO}\ncallDBus({}, {}, {}, 'Snapshot', '{request}', JSON.stringify(info(workspace.activeWindow)));",
            serde_json::to_string(&self.service)?,
            serde_json::to_string(OBJECT)?,
            serde_json::to_string(INTERFACE)?
        );
        write_script(&path, &script)?;
        let result = async {
            load_run(&self.connection, &path, &name).await?;
            let current = tokio::time::timeout(Duration::from_secs(2), receive).await??;
            self.ensure_healthy()?;
            Ok(current)
        }
        .await;
        let scripting = Proxy::new(
            &self.connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let _: Result<bool, _> = scripting.call("unloadScript", &(name.as_str(),)).await;
        let _ = std::fs::remove_file(&path);
        self.state.lock().unwrap().pending.remove(&request);
        if result.is_err() {
            self.revoke_all();
        }
        result
    }

    pub async fn stop(&self) -> Result<()> {
        self.revoke_all();
        self.state.lock().unwrap().healthy = false;
        self.stop.cancel();
        let task = self.monitor.lock().unwrap().take();
        if let Some(task) = task {
            task.await?;
        }
        let scripting = Proxy::new(
            &self.connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let _: bool = tokio::time::timeout(
            Duration::from_secs(2),
            scripting.call("unloadScript", &(SCRIPT,)),
        )
        .await??;
        let _ = std::fs::remove_file(&self.path);
        self.connection
            .object_server()
            .remove::<Receiver, _>(OBJECT)
            .await?;
        self.connection.release_name(self.service.as_str()).await?;
        Ok(())
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.cancel();
        self.state.lock().unwrap().revoke();
    }
}

fn write_script(path: &std::path::Path, script: &str) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(script.as_bytes())?;
    Ok(())
}

async fn load_run(connection: &Connection, path: &std::path::Path, name: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let scripting = Proxy::new(
            connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let index: i32 = scripting
            .call(
                "loadScript",
                &(path.to_str().context("Invalid script path")?, name),
            )
            .await?;
        ensure!(index >= 0, "KWin script load failed");
        let script = Proxy::new(
            connection,
            "org.kde.KWin",
            format!("/Scripting/Script{index}"),
            "org.kde.kwin.Script",
        )
        .await?;
        let _: () = script.call("run", &()).await?;
        Ok(())
    })
    .await
    .context("KWin script operation timed out")?
}

async fn observer_loaded(connection: &Connection) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(1), async {
        let scripting = Proxy::new(
            connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let loaded: bool = scripting.call("isScriptLoaded", &(SCRIPT,)).await?;
        ensure!(loaded, "KWin observer script was removed");
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("KWin observer health check timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(id: &str) -> TargetIdentity {
        TargetIdentity {
            window_id: id.into(),
            pid: 123,
            application: "com.t3tools.T3Code".into(),
        }
    }
    #[test]
    fn focus_return_never_rearms_permit() {
        let original = target("original");
        let permit = TargetPermit::new(original.clone());
        let mut state = State {
            healthy: true,
            current: Some(original.clone()),
            permits: vec![permit.downgrade()],
            ..State::default()
        };
        state.observe(Some(target("other")));
        state.observe(Some(original));
        assert!(!permit.is_valid());
    }
    #[test]
    fn disappearance_and_session_loss_revoke() {
        let permit = TargetPermit::new(target("original"));
        let mut state = State {
            permits: vec![permit.downgrade()],
            ..State::default()
        };
        state.observe(None);
        assert!(!permit.is_valid());
    }
}
