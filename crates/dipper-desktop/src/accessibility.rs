use anyhow::{Result, bail, ensure};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

pub async fn check_composer(pid: u32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(4), inspect_current(|| inspect(pid)))
        .await
        .map_err(|_| anyhow::anyhow!("T3 accessibility validation timed out"))?
}

// A disappearing unrelated chat node can invalidate a traversal. Retry the
// complete proof once within the original deadline, never skip failed nodes.
async fn inspect_current<F, Fut>(mut inspect: F) -> Result<()>
where
    F: FnMut() -> Fut + Send,
    Fut: std::future::Future<Output = Result<()>> + Send,
{
    let result = inspect().await;
    let disappeared = result.as_ref().err().is_some_and(|error| {
        matches!(error.downcast_ref::<zbus::Error>(), Some(zbus::Error::MethodError(name, _, _))
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownObject")
    });
    if disappeared { inspect().await } else { result }
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
    let mut frames = 0;
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
        if role == "frame" {
            let states: Vec<u32> = object.call("GetState", &()).await?;
            if !has_state(&states, 6) {
                frames += 1;
            }
        }
        if role == "entry" {
            let attributes: HashMap<String, String> = object.call("GetAttributes", &()).await?;
            let classes = attributes
                .get("class")
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if classes.contains("tiptap") || classes.contains("prosemirror") {
                let states: Vec<u32> = object.call("GetState", &()).await?;
                if qualifies_composer(&role, &classes, &states) {
                    composers += 1;
                }
            }
        }
        let children: Vec<(String, OwnedObjectPath)> = object.call("GetChildren", &()).await?;
        for child in children {
            if child.1.as_str() != "/org/a11y/atspi/null" {
                queue.push_back(child);
            }
        }
    }
    ensure_unique_composer(composers, frames)
}

fn has_state(states: &[u32], bit: usize) -> bool {
    states
        .get(bit / 32)
        .is_some_and(|word| word & (1 << (bit % 32)) != 0)
}

fn qualifies_composer(role: &str, classes: &str, states: &[u32]) -> bool {
    let classes = classes.to_ascii_lowercase();
    // AT-SPI: DEFUNCT=6, EDITABLE=7, ENABLED=8, FOCUSED=12,
    // MULTI_LINE=17, SENSITIVE=24, SHOWING=25, STALE=27, VISIBLE=30.
    role == "entry"
        && (classes.contains("tiptap") || classes.contains("prosemirror"))
        && !has_state(states, 6)
        && !has_state(states, 27)
        && [7, 8, 12, 17, 24, 25, 30]
            .into_iter()
            .all(|bit| has_state(states, bit))
}

fn ensure_unique_composer(composers: usize, frames: usize) -> Result<()> {
    if frames != 1 {
        bail!("T3 accessibility windows are missing or cannot be mapped uniquely");
    }
    ensure!(composers == 1, "T3 composer is missing or ambiguous");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn disappeared_object() -> anyhow::Error {
        zbus::Error::MethodError(
            "org.freedesktop.DBus.Error.UnknownObject"
                .try_into()
                .unwrap(),
            None,
            zbus::Message::method_call("/", "GetRoleName")
                .unwrap()
                .build(&())
                .unwrap(),
        )
        .into()
    }
    #[tokio::test]
    async fn disappearing_tree_retries_the_full_proof_once_and_preserves_failures() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        inspect_current(|| async {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(disappeared_object())
            } else {
                Ok(())
            }
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        calls.store(0, Ordering::SeqCst);
        let result = inspect_current(|| async {
            if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(disappeared_object())
            } else {
                ensure_unique_composer(0, 1)
            }
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("composer"));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        calls.store(0, Ordering::SeqCst);
        assert!(
            inspect_current(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                Err(disappeared_object())
            })
            .await
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        calls.store(0, Ordering::SeqCst);
        assert!(
            inspect_current(|| async {
                calls.fetch_add(1, Ordering::SeqCst);
                ensure_unique_composer(2, 1)
            })
            .await
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    fn valid_states() -> Vec<u32> {
        vec![
            [7, 8, 12, 17, 24, 25, 30]
                .into_iter()
                .fold(0, |bits, bit| bits | (1 << bit)),
        ]
    }
    #[test]
    fn unfocused_editor_does_not_disqualify_current_unique_composer() {
        let focused = valid_states();
        let mut unfocused = focused.clone();
        unfocused[0] &= !(1 << 12);
        let candidates = [
            (&focused, "entry", "Tiptap ProseMirror"),
            (&unfocused, "entry", "tiptap"),
        ];
        let count = candidates
            .into_iter()
            .filter(|(states, role, classes)| qualifies_composer(role, classes, states))
            .count();
        assert_eq!(count, 1);
        ensure_unique_composer(count, 1).unwrap();
    }
    #[test]
    fn invalid_or_ambiguous_candidates_and_unmapped_windows_fail() {
        let focused = valid_states();
        for missing in [7, 8, 12, 17, 24, 25, 30] {
            let mut states = focused.clone();
            states[0] &= !(1 << missing);
            assert!(!qualifies_composer("entry", "ProseMirror", &states));
        }
        for prohibited in [6, 27] {
            let mut states = focused.clone();
            states[0] |= 1 << prohibited;
            assert!(!qualifies_composer("entry", "Tiptap", &states));
        }
        assert!(!qualifies_composer("password text", "Tiptap", &focused));
        assert!(!qualifies_composer("entry", "search", &focused));
        assert!(ensure_unique_composer(0, 1).is_err());
        assert!(ensure_unique_composer(2, 1).is_err());
        assert!(ensure_unique_composer(1, 2).is_err());
        assert!(ensure_unique_composer(1, 0).is_err());
    }
}
