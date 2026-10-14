use crate::DesktopEvent;
use anyhow::{Context, Result, ensure};
use dipper_core::target::{TargetIdentity, TargetPermit, WeakTargetPermit};
use futures_util::StreamExt;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{Notify, broadcast, oneshot};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
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
    cleanup_scripts: HashSet<String>,
}

impl State {
    fn observe(&mut self, current: Option<TargetIdentity>) {
        self.permits.retain(|weak| {
            if let Some(permit) = weak.upgrade() {
                if current.as_ref() != Some(&permit.identity) {
                    permit.revoke();
                }
                permit.is_valid()
            } else {
                false
            }
        });
        self.current = current;
    }
    pub(crate) fn revoke(&mut self) {
        for weak in &self.permits {
            if let Some(permit) = weak.upgrade() {
                permit.revoke();
            }
        }
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
    snapshot_lock: Arc<tokio::sync::Mutex<()>>,
    snapshots: TaskTracker,
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
        let initialized_result: Result<()> = async {
            let loaded = tokio::time::timeout(Duration::from_secs(2), async {
                let scripting = Proxy::new(
                    &connection,
                    "org.kde.KWin",
                    "/Scripting",
                    "org.kde.kwin.Scripting",
                )
                .await?;
                let loaded: bool = scripting.call("isScriptLoaded", &(SCRIPT,)).await?;
                Ok::<_, anyhow::Error>(loaded)
            })
            .await
            .context("KWin observer setup timed out")??;
            // Single-instance Dipper owns this exact name, including stale runs.
            if loaded {
                unload_script(&connection, SCRIPT).await?;
            }
            load_run(&connection, &path, SCRIPT).await?;
            tokio::time::timeout(Duration::from_secs(3), initialized.notified())
                .await
                .context("KWin observer did not initialize")?;
            Ok(())
        }
        .await;
        if let Err(error) = initialized_result {
            let unload = unload_script(&connection, SCRIPT).await;
            let remove = remove_script_file(&path);
            return Err(error.context(format!(
                "observer setup cleanup: unload={unload:?}, file={remove:?}"
            )));
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
                        let armed = task_state.lock().unwrap().permits.iter().any(|weak| weak.upgrade().is_some_and(|permit| permit.is_valid()));
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
            snapshot_lock: Arc::new(tokio::sync::Mutex::new(())),
            snapshots: TaskTracker::new(),
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
        state
            .permits
            .retain(|weak| weak.upgrade().is_some_and(|permit| permit.is_valid()));
        state.permits.push(permit.downgrade());
        Ok(())
    }
    pub fn revoke_all(&self) {
        self.state.lock().unwrap().revoke();
    }

    pub async fn snapshot(&self) -> Result<Option<TargetIdentity>> {
        let operation = {
            // Registration and stop's admission closure use the same lock.
            let state = self.state.lock().unwrap();
            ensure!(state.healthy, "KWin observer is unavailable");
            let request = self
                .request
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1;
            let snapshot = SnapshotOperation {
                state: self.state.clone(),
                connection: self.connection.clone(),
                runtime: self.runtime.clone(),
                service: self.service.clone(),
                request,
            };
            spawn_serialized(&self.snapshots, self.snapshot_lock.clone(), async move {
                snapshot.run().await
            })
        };
        // Canceling this waiter does not cancel the owned native operation.
        operation.await.context("Owned KWin snapshot task failed")?
    }

    pub async fn stop(&self) -> Result<()> {
        {
            let mut state = self.state.lock().unwrap();
            state.revoke();
            state.healthy = false;
            self.snapshots.close();
        }
        self.stop.cancel();
        // Include queued operations, even if their spawned task was not polled yet.
        self.snapshots.wait().await;
        let _snapshot_guard = self.snapshot_lock.lock().await;
        let task = self.monitor.lock().unwrap().take();
        let mut failures = Vec::new();
        if let Some(task) = task
            && let Err(error) = task.await
        {
            failures.push(format!("observer monitor: {error}"));
        }
        let pending_scripts: Vec<String> = self
            .state
            .lock()
            .unwrap()
            .cleanup_scripts
            .iter()
            .cloned()
            .collect();
        for name in pending_scripts {
            match unload_script(&self.connection, &name).await {
                Ok(()) => {
                    self.state.lock().unwrap().cleanup_scripts.remove(&name);
                }
                Err(error) => failures.push(format!("snapshot script {name}: {error}")),
            }
        }
        if let Err(error) = unload_script(&self.connection, SCRIPT).await {
            failures.push(error.to_string());
        }
        if let Err(error) = remove_script_file(&self.path) {
            failures.push(error.to_string());
        }
        match tokio::time::timeout(
            Duration::from_secs(2),
            self.connection
                .object_server()
                .remove::<Receiver, _>(OBJECT),
        )
        .await
        {
            Ok(Ok(_)) | Ok(Err(zbus::Error::InterfaceNotFound)) => {}
            Ok(Err(error)) => failures.push(format!("observer receiver removal: {error}")),
            Err(_) => failures.push("observer receiver removal timed out".into()),
        }
        match tokio::time::timeout(
            Duration::from_secs(2),
            self.connection.release_name(self.service.as_str()),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => failures.push(format!("observer name release: {error}")),
            Err(_) => failures.push("observer name release timed out".into()),
        }
        ensure!(
            failures.is_empty(),
            "Observer cleanup failed: {}",
            failures.join("; ")
        );
        Ok(())
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.stop.cancel();
        self.snapshots.close();
        let mut state = self.state.lock().unwrap();
        state.healthy = false;
        state.revoke();
        if let Some(task) = self.monitor.get_mut().unwrap().take() {
            task.abort();
        }
        // Native snapshot tasks retain their resources until their bounded cleanup ends.
        // Explicit stop is required to await cleanup and unload the persistent observer.
    }
}

fn spawn_serialized<F, T>(
    tasks: &TaskTracker,
    lock: Arc<tokio::sync::Mutex<()>>,
    operation: F,
) -> tokio::task::JoinHandle<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    tasks.spawn(async move {
        let _guard = lock.lock_owned().await;
        operation.await
    })
}

struct SnapshotOperation {
    state: Arc<Mutex<State>>,
    connection: Connection,
    runtime: PathBuf,
    service: String,
    request: u64,
}

impl SnapshotOperation {
    async fn run(self) -> Result<Option<TargetIdentity>> {
        // A queued operation canceled by shutdown has not acquired any native resource.
        ensure!(
            self.state.lock().unwrap().healthy,
            "KWin observer is stopping"
        );
        if let Err(error) = observer_loaded(&self.connection).await {
            self.state.lock().unwrap().revoke();
            return Err(error);
        }
        let request = self.request;
        let name = format!(
            "org.dipper.Dipper.Snapshot.p{}.r{request}",
            std::process::id()
        );
        let path = self.runtime.join(format!("snapshot-{request}.js"));
        let (send, receive) = oneshot::channel();
        self.state.lock().unwrap().pending.insert(request, send);
        let mut file_created = false;
        let mut load_attempted = false;
        let result = async {
            let script = format!(
                "{INFO}\ncallDBus({}, {}, {}, 'Snapshot', '{request}', JSON.stringify(info(workspace.activeWindow)));",
                serde_json::to_string(&self.service)?,
                serde_json::to_string(OBJECT)?,
                serde_json::to_string(INTERFACE)?
            );
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
                .mode(0o600).open(&path)?;
            file_created = true;
            std::io::Write::write_all(&mut file, script.as_bytes())?;
            load_attempted = true;
            self.state.lock().unwrap().cleanup_scripts.insert(name.clone());
            load_run(&self.connection, &path, &name).await?;
            let current = tokio::time::timeout(Duration::from_secs(2), receive).await??;
            ensure!(self.state.lock().unwrap().healthy, "KWin observer is unavailable");
            Ok(current)
        }.await;
        // Finally always runs inside this owned task, including write/load/query failure.
        let unload = if load_attempted {
            unload_script(&self.connection, &name).await
        } else {
            Ok(())
        };
        if unload.is_ok() {
            self.state.lock().unwrap().cleanup_scripts.remove(&name);
        }
        let remove = if file_created {
            remove_script_file(&path)
        } else {
            Ok(())
        };
        self.state.lock().unwrap().pending.remove(&request);
        match (result, unload, remove) {
            (Ok(current), Ok(()), Ok(())) => Ok(current),
            (result, unload, remove) => {
                self.state.lock().unwrap().revoke();
                let errors: Vec<String> = [result.err(), unload.err(), remove.err()]
                    .into_iter()
                    .flatten()
                    .map(|error| error.to_string())
                    .collect();
                anyhow::bail!("KWin snapshot failed: {}", errors.join("; "))
            }
        }
    }
}

async fn unload_script(connection: &Connection, name: &str) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(2), async {
        let scripting = Proxy::new(
            connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let _: bool = scripting.call("unloadScript", &(name,)).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("KWin script cleanup timed out")?
}

fn remove_script_file(path: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Remove owned KWin script file"),
    }
}

fn write_script(path: &std::path::Path, script: &str) -> Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    if let Err(error) = file.write_all(script.as_bytes()) {
        drop(file);
        let cleanup = remove_script_file(path);
        return Err(
            anyhow::Error::from(error).context(format!("script write cleanup: {cleanup:?}"))
        );
    }
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
    #[tokio::test]
    async fn canceled_snapshot_waiter_leaves_owned_cleanup_running() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("owned-snapshot.js");
        let state = Arc::new(Mutex::new(State::default()));
        let tasks = TaskTracker::new();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let release = Arc::new(Notify::new());
        let (started, running) = oneshot::channel();
        let operation_path = path.clone();
        let operation_state = state.clone();
        let operation_tasks = tasks.clone();
        let operation_lock = lock.clone();
        let operation_release = release.clone();
        let waiter = tokio::spawn(async move {
            spawn_serialized(&operation_tasks, operation_lock, async move {
                let (send, _receive) = oneshot::channel();
                operation_state.lock().unwrap().pending.insert(1, send);
                write_script(&operation_path, "synthetic snapshot").unwrap();
                started.send(()).unwrap();
                operation_release.notified().await;
                remove_script_file(&operation_path).unwrap();
                operation_state.lock().unwrap().pending.remove(&1);
            })
            .await
            .unwrap();
        });
        running.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        tasks.close();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), tasks.wait())
                .await
                .is_err()
        );
        assert!(path.exists());
        assert!(state.lock().unwrap().pending.contains_key(&1));
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(1), tasks.wait())
            .await
            .unwrap();
        let _cleanup_guard = lock.lock().await;
        assert!(!path.exists());
        assert!(state.lock().unwrap().pending.is_empty());
    }

    #[tokio::test]
    async fn cleanup_waits_for_queued_owned_snapshot_operations() {
        let tasks = TaskTracker::new();
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        let blocker = lock.clone().lock_owned().await;
        let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed = finished.clone();
        let operation = spawn_serialized(&tasks, lock.clone(), async move {
            completed.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        drop(operation); // The caller may disappear before the inner task is polled.
        tasks.close();
        assert!(
            tokio::time::timeout(Duration::from_millis(20), tasks.wait())
                .await
                .is_err()
        );
        assert!(!finished.load(std::sync::atomic::Ordering::SeqCst));
        drop(blocker);
        tokio::time::timeout(Duration::from_secs(1), tasks.wait())
            .await
            .unwrap();
        let _cleanup_guard = lock.lock().await;
        assert!(finished.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn exclusive_script_failure_preserves_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("existing.js");
        std::fs::write(&path, "existing content").unwrap();
        assert!(write_script(&path, "replacement").is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "existing content");
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
