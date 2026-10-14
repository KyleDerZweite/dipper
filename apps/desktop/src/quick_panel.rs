//! KDE placement for the app-owned quick panel; independent of target validation.
use anyhow::{Context, Result, ensure};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, sync::Mutex, time::Duration};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use zbus::{Connection, Proxy, message::Header};

struct Receiver {
    sender: String,
    done: Mutex<Option<oneshot::Sender<bool>>>,
}
#[zbus::interface(name = "org.dipper.QuickPlacement", spawn = false)]
impl Receiver {
    fn report(&self, placed: bool, #[zbus(header)] header: Header<'_>) -> zbus::fdo::Result<()> {
        if header.sender().map(|s| s.as_str()) != Some(self.sender.as_str()) {
            return Err(zbus::fdo::Error::AccessDenied(
                "Unexpected placement sender".into(),
            ));
        }
        if let Some(done) = self.done.lock().unwrap().take() {
            let _ = done.send(placed);
        }
        Ok(())
    }
}

pub async fn place(x: f64, y: f64, cancel: CancellationToken) -> Result<()> {
    let (done, received) = oneshot::channel();
    let setup = tokio::time::timeout(Duration::from_secs(2), async {
        let connection = Connection::session().await?;
        let bus = Proxy::new(
            &connection,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
        )
        .await?;
        let sender: String = bus.call("GetNameOwner", &("org.kde.KWin",)).await?;
        connection
            .object_server()
            .at(
                "/org/dipper/QuickPlacement",
                Receiver {
                    sender,
                    done: Mutex::new(Some(done)),
                },
            )
            .await?;
        Ok::<_, anyhow::Error>(connection)
    });
    let connection = tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("Quick placement canceled"),
        result = setup => result.context("Quick placement setup timed out")??,
    };
    let service = serde_json::to_string(
        connection
            .unique_name()
            .context("placement bus identity")?
            .as_str(),
    )?;
    let hint = if x.is_finite() && y.is_finite() {
        (x, y)
    } else {
        (0.0, 0.0)
    };
    let script = include_str!("quick_placement.js")
        .replace("__PID__", &std::process::id().to_string())
        .replace("__SERVICE__", &service)
        .replace("__POINT__", &serde_json::to_string(&hint)?);
    let name = format!("org.dipper.QuickPlacement.{}", uuid::Uuid::new_v4());
    let path =
        std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").context("runtime directory")?)
            .join(format!("{name}.js"));
    if cancel.is_cancelled() {
        anyhow::bail!("Quick placement canceled");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    // From successful create_new onward, every result passes through cleanup.
    let operation = tokio::time::timeout(Duration::from_secs(2), async {
        file.write_all(script.as_bytes())?;
        drop(file);
        let scripting = Proxy::new(
            &connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let index: i32 = scripting
            .call(
                "loadScript",
                &(path.to_str().context("script path")?, name.as_str()),
            )
            .await?;
        ensure!(index >= 0, "Quick placement script load failed");
        let runner = Proxy::new(
            &connection,
            "org.kde.KWin",
            format!("/Scripting/Script{index}"),
            "org.kde.kwin.Script",
        )
        .await?;
        let _: () = runner.call("run", &()).await?;
        ensure!(
            received.await.context("quick placement reply")?,
            "Quick panel could not be positioned"
        );
        Ok::<_, anyhow::Error>(())
    });
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(anyhow::anyhow!("Quick placement canceled")),
        result = operation => result.context("Quick placement timed out").and_then(|value| value),
    };
    // A canceled load can complete later. The ordered unload is awaited even
    // when canceled, before the next placement can acquire its host gate.
    let cleanup = tokio::time::timeout(Duration::from_secs(2), async {
        let scripting = Proxy::new(
            &connection,
            "org.kde.KWin",
            "/Scripting",
            "org.kde.kwin.Scripting",
        )
        .await?;
        let _: bool = scripting.call("unloadScript", &(name.as_str(),)).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Quick placement cleanup timed out")
    .and_then(|value| value);
    let removed = std::fs::remove_file(&path).context("Remove owned quick placement script");
    cleanup?;
    removed?;
    result
}
