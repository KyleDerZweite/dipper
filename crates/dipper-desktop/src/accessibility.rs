use anyhow::{Result, bail, ensure};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

pub async fn check_composer(pid: u32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(4), inspect(pid))
        .await
        .map_err(|_| anyhow::anyhow!("T3 accessibility validation timed out"))?
}

async fn inspect(pid: u32) -> Result<()> {
    let session = Connection::session().await?;
    let bus = Proxy::new(&session, "org.a11y.Bus", "/org/a11y/bus", "org.a11y.Bus").await?;
    let address: String = bus.call("GetAddress", &()).await?;
    let connection = zbus::connection::Builder::address(address.as_str())?
        .build()
        .await?;
    let dbus = Proxy::new(
        &connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await?;
    let registry = Proxy::new(
        &connection,
        "org.a11y.atspi.Registry",
        "/org/a11y/atspi/accessible/root",
        "org.a11y.atspi.Accessible",
    )
    .await?;
    let roots: Vec<(String, OwnedObjectPath)> = registry.call("GetChildren", &()).await?;
    let mut queue = VecDeque::new();
    for (destination, path) in roots {
        let owner_pid: u32 = dbus
            .call("GetConnectionUnixProcessID", &(destination.as_str(),))
            .await?;
        if owner_pid == pid {
            queue.push_back((destination, path));
        }
    }
    ensure!(
        queue.len() == 1,
        "T3 accessibility process is missing or ambiguous"
    );
    let mut seen = HashSet::new();
    let mut composers = 0;
    while let Some((destination, path)) = queue.pop_front() {
        if !seen.insert((destination.clone(), path.clone())) {
            continue;
        }
        ensure!(
            seen.len() <= 650,
            "T3 accessibility tree exceeds the validation bound"
        );
        let object = Proxy::new(
            &connection,
            destination.as_str(),
            path.as_str(),
            "org.a11y.atspi.Accessible",
        )
        .await?;
        let role: String = object.call("GetRoleName", &()).await?;
        if role == "entry" {
            let attributes: HashMap<String, String> = object.call("GetAttributes", &()).await?;
            let classes = attributes
                .get("class")
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if classes.contains("tiptap") || classes.contains("prosemirror") {
                let states: Vec<u32> = object.call("GetState", &()).await?;
                let has = |bit: usize| {
                    states
                        .get(bit / 32)
                        .is_some_and(|word| word & (1 << (bit % 32)) != 0)
                };
                // AT-SPI StateType: DEFUNCT=6, EDITABLE=7, ENABLED=8,
                // FOCUSED=12, MULTI_LINE=17, SENSITIVE=24, SHOWING=25, VISIBLE=30.
                ensure!(
                    !has(6) && !has(27) && [7, 8, 12, 17, 24, 25, 30].into_iter().all(has),
                    "T3 composer is not focused, editable, and visible"
                );
                composers += 1;
            }
        }
        let children: Vec<(String, OwnedObjectPath)> = object.call("GetChildren", &()).await?;
        for child in children {
            if child.1.as_str() != "/org/a11y/atspi/null" {
                queue.push_back(child);
            }
        }
    }
    if composers != 1 {
        bail!("T3 composer is missing or ambiguous");
    }
    Ok(())
}
