use anyhow::{Context, Result, ensure};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

pub async fn check_editor(pid: u32) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(4), inspect_current(|| inspect(pid)))
        .await
        .map_err(|_| anyhow::anyhow!("Text field accessibility validation timed out"))?
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
    let roots: Vec<Reference> = registry.call("GetChildren", &()).await?;
    let mut owned_roots = Vec::new();
    for (destination, path) in roots {
        let owner_pid: u32 = dbus
            .call("GetConnectionUnixProcessID", &(destination.as_str(),))
            .await?;
        if owner_pid == pid {
            owned_roots.push((destination, path));
        }
    }
    ensure!(
        owned_roots.len() == 1,
        "Target accessibility process is missing or ambiguous"
    );
    let (destination, path) = owned_roots.pop().unwrap();
    let owner: String = dbus.call("GetNameOwner", &(destination.as_str(),)).await?;
    let owner_pid: u32 = dbus
        .call("GetConnectionUnixProcessID", &(owner.as_str(),))
        .await?;
    ensure!(
        owner_pid == pid,
        "Target accessibility process owner changed"
    );
    let root = (owner.clone(), path);
    let collection = Proxy::new(
        &connection,
        owner.as_str(),
        root.1.as_str(),
        "org.a11y.atspi.Collection",
    )
    .await?;
    let candidates = bounded_candidates(&owner, |rule, count| {
        let collection = &collection;
        async move {
            collection.call("GetMatches", &(rule, 1_u32, count, true)).await
                .context("Target accessibility Collection query failed; this app may not support guarded insertion")
        }
    }).await?;
    prove_editor(&root, candidates, |reference| {
        let connection = &connection;
        async move {
            let object = Proxy::new(
                connection,
                reference.0.as_str(),
                reference.1.as_str(),
                "org.a11y.atspi.Accessible",
            )
            .await?;
            Ok(Metadata {
                role: object.call("GetRole", &()).await?,
                states: object.call("GetState", &()).await?,
                parent: object.get_property("Parent").await?,
            })
        }
    })
    .await
}

// AT-SPI numeric roles and states are protocol values, independent of translated
// role names, CSS, or toolkit interfaces. Never inspect private field contents.
const FRAME: u32 = 23;
const PASSWORD: u32 = 40;
const TERMINAL: u32 = 60;
const TEXT: u32 = 61;
const APPLICATION: u32 = 75;
const ENTRY: u32 = 79;
const TREE_SENTINEL: i32 = 4097;
const CANDIDATE_SENTINEL: i32 = 65;
const MAX_PARENT_EDGES: usize = 64;
const NULL_PATH: &str = "/org/a11y/atspi/null";

type Reference = (String, OwnedObjectPath);
// AT-SPI Collection MatchRule signature: (aiia{ss}iaiiasib).
type MatchRule = (
    Vec<i32>,
    i32,
    HashMap<String, String>,
    i32,
    Vec<i32>,
    i32,
    Vec<String>,
    i32,
    bool,
);
#[derive(Clone, Copy)]
enum Query {
    All,
    Editors,
    ActiveFrames,
}

fn match_rule(query: Query) -> MatchRule {
    let (states, roles, role_match) = match query {
        Query::All => (vec![0, 0], vec![0; 4], 1),
        Query::Editors => (
            vec![(1 << 7) | (1 << 12), 0],
            vec![0, 1 << (TEXT - 32), 1 << (ENTRY - 64), 0],
            2,
        ),
        Query::ActiveFrames => (vec![1 << 1, 0], vec![1 << FRAME, 0, 0, 0], 1),
    };
    (
        states,
        1,
        HashMap::new(),
        1,
        roles,
        role_match,
        Vec::new(),
        1,
        false,
    )
}

struct Candidates {
    all: HashSet<Reference>,
    editors: Vec<Reference>,
    frames: Vec<Reference>,
}

async fn bounded_candidates<F, Fut>(owner: &str, mut query: F) -> Result<Candidates>
where
    F: FnMut(MatchRule, i32) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<Reference>>>,
{
    // The unfiltered sentinel proves completeness below the provider's child
    // clamp. Filtering only focused fields avoids enumerating ordinary text.
    let all = query(match_rule(Query::All), TREE_SENTINEL).await?;
    ensure!(
        all.len() < TREE_SENTINEL as usize,
        "Target accessibility tree exceeds the validation bound"
    );
    let mut seen = HashSet::new();
    for reference in all {
        ensure!(
            reference.0 == owner && reference.1.as_str() != NULL_PATH && seen.insert(reference),
            "Target accessibility tree contains invalid or duplicate references"
        );
    }
    let mut unique = HashSet::new();
    let mut results = Vec::new();
    for filter in [Query::Editors, Query::ActiveFrames] {
        let candidates = query(match_rule(filter), CANDIDATE_SENTINEL).await?;
        ensure!(
            candidates.len() < CANDIDATE_SENTINEL as usize,
            "Target accessibility candidate limit exceeded"
        );
        for reference in &candidates {
            ensure!(
                seen.contains(reference) && unique.insert(reference.clone()),
                "Target accessibility tree changed or candidates are ambiguous"
            );
        }
        results.push(candidates);
    }
    let frames = results.pop().unwrap();
    let editors = results.pop().unwrap();
    Ok(Candidates {
        all: seen,
        editors,
        frames,
    })
}

#[derive(Clone)]
struct Metadata {
    role: u32,
    states: Vec<u32>,
    parent: Reference,
}

fn has_state(states: &[u32], bit: usize) -> bool {
    states
        .get(bit / 32)
        .is_some_and(|word| word & (1 << (bit % 32)) != 0)
}
fn live(states: &[u32]) -> bool {
    !has_state(states, 6) && !has_state(states, 27)
}
fn qualifies_editor(node: &Metadata) -> bool {
    matches!(node.role, ENTRY | TEXT)
        && live(&node.states)
        && !has_state(&node.states, 43)
        && [7, 8, 12, 24, 25, 30]
            .into_iter()
            .all(|bit| has_state(&node.states, bit))
}
fn active_frame(node: &Metadata) -> bool {
    node.role == FRAME && live(&node.states) && has_state(&node.states, 1)
}
fn ensure_unique_editor(editors: usize, frames: usize) -> Result<()> {
    ensure!(
        frames == 1,
        "Target active window is missing or ambiguous in accessibility"
    );
    ensure!(
        editors == 1,
        "Focused editable text field is missing or ambiguous"
    );
    Ok(())
}

async fn prove_editor<F, Fut>(root: &Reference, candidates: Candidates, mut read: F) -> Result<()>
where
    F: FnMut(Reference) -> Fut,
    Fut: std::future::Future<Output = Result<Metadata>>,
{
    ensure_unique_editor(candidates.editors.len(), candidates.frames.len())?;
    let root_node = read(root.clone()).await?;
    ensure!(
        root_node.role == APPLICATION && live(&root_node.states),
        "Target accessibility root is invalid"
    );
    let editor = &candidates.editors[0];
    let frame = &candidates.frames[0];
    ensure!(
        active_frame(&read(frame.clone()).await?),
        "Target active window changed during validation"
    );
    let mut current = editor.clone();
    let mut ancestors = HashSet::new();
    for edge in 0..=MAX_PARENT_EDGES {
        ensure!(
            current != *root
                && current.0 == root.0
                && candidates.all.contains(&current)
                && ancestors.insert(current.clone()),
            "Text field ancestry is invalid, cyclic, or outside the target window"
        );
        let node = read(current.clone()).await?;
        ensure!(
            live(&node.states) && !matches!(node.role, PASSWORD | TERMINAL),
            "Text field ancestry is unavailable or unsupported"
        );
        if edge == 0 {
            ensure!(
                qualifies_editor(&node),
                "Focused text field is not an enabled editable target"
            );
        }
        if &current == frame {
            ensure!(
                active_frame(&node),
                "Target active window changed during validation"
            );
            let root_node = read(root.clone()).await?;
            ensure!(
                root_node.role == APPLICATION && live(&root_node.states),
                "Target accessibility root changed during validation"
            );
            ensure!(
                qualifies_editor(&read(editor.clone()).await?),
                "Focused text field changed during validation"
            );
            return Ok(());
        }
        ensure!(
            node.role != FRAME && node.role != APPLICATION,
            "Focused text field belongs to another window"
        );
        current = node.parent;
    }
    anyhow::bail!("Text field ancestry exceeds the validation bound")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reference(index: usize) -> Reference {
        (":1.42".into(), format!("/node/{index}").try_into().unwrap())
    }
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
                ensure_unique_editor(0, 1)
            }
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("text field"));
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
                ensure_unique_editor(2, 1)
            })
            .await
            .is_err()
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    fn node(role: u32, states: &[usize], parent: usize) -> Metadata {
        let mut words = vec![0, 0];
        for bit in states {
            words[bit / 32] |= 1 << (bit % 32);
        }
        Metadata {
            role,
            states: words,
            parent: reference(parent),
        }
    }
    fn fixture(role: u32) -> HashMap<Reference, Metadata> {
        HashMap::from([
            (reference(0), node(APPLICATION, &[], 0)),
            (reference(1), node(FRAME, &[1], 0)),
            (reference(2), node(role, &[7, 8, 12, 24, 25, 30], 3)),
            (reference(3), node(20, &[], 1)),
        ])
    }
    async fn prove(nodes: HashMap<Reference, Metadata>) -> Result<()> {
        let candidates = Candidates {
            all: nodes
                .keys()
                .filter(|r| **r != reference(0))
                .cloned()
                .collect(),
            editors: vec![reference(2)],
            frames: vec![reference(1)],
        };
        prove_editor(&reference(0), candidates, |r| {
            let result = nodes.get(&r).cloned().context("Object disappeared");
            async move { result }
        })
        .await
    }
    #[tokio::test]
    async fn single_line_entry_and_text_without_css_or_editable_interface_are_supported() {
        prove(fixture(ENTRY)).await.unwrap();
        prove(fixture(TEXT)).await.unwrap();
    }
    #[tokio::test]
    async fn protected_unfocused_unavailable_and_disabled_fields_are_rejected() {
        for role in [PASSWORD, TERMINAL, FRAME] {
            assert!(prove(fixture(role)).await.is_err());
        }
        for missing in [7, 8, 12, 24, 25, 30] {
            let mut nodes = fixture(ENTRY);
            nodes.get_mut(&reference(2)).unwrap().states[0] &= !(1 << missing);
            assert!(prove(nodes).await.is_err());
        }
        for forbidden in [6, 27, 43] {
            let mut nodes = fixture(ENTRY);
            nodes.get_mut(&reference(2)).unwrap().states[forbidden / 32] |= 1 << (forbidden % 32);
            assert!(prove(nodes).await.is_err());
        }
    }
    #[tokio::test]
    async fn field_must_reach_live_active_frame_without_crossing_root_foreign_cycle_or_protected_ancestor()
     {
        for parent in [0, 2, 99] {
            let mut nodes = fixture(ENTRY);
            nodes.get_mut(&reference(3)).unwrap().parent = reference(parent);
            assert!(prove(nodes).await.is_err());
        }
        let mut nodes = fixture(ENTRY);
        nodes.get_mut(&reference(3)).unwrap().parent.0 = ":1.99".into();
        assert!(prove(nodes).await.is_err());
        for role in [PASSWORD, TERMINAL, FRAME, APPLICATION] {
            let mut nodes = fixture(ENTRY);
            nodes.get_mut(&reference(3)).unwrap().role = role;
            assert!(prove(nodes).await.is_err());
        }
        for (object, states) in [(0, vec![6]), (1, vec![]), (1, vec![1, 27]), (3, vec![6])] {
            let mut nodes = fixture(ENTRY);
            let old = nodes[&reference(object)].clone();
            nodes.insert(reference(object), node(old.role, &states, 1));
            assert!(prove(nodes).await.is_err());
        }
    }
    #[tokio::test]
    async fn excessive_ancestry_and_changed_editor_at_final_check_fail() {
        let mut nodes = fixture(ENTRY);
        for index in 3..68 {
            nodes.insert(reference(index), node(20, &[], index + 1));
        }
        nodes.insert(reference(68), node(20, &[], 1));
        assert!(
            prove(nodes)
                .await
                .unwrap_err()
                .to_string()
                .contains("bound")
        );
        let nodes = fixture(ENTRY);
        let candidates = Candidates {
            all: nodes.keys().cloned().collect(),
            editors: vec![reference(2)],
            frames: vec![reference(1)],
        };
        let mut editor_reads = 0;
        let result = prove_editor(&reference(0), candidates, |r| {
            let mut node = nodes[&r].clone();
            if r == reference(2) {
                editor_reads += 1;
                if editor_reads > 1 {
                    node.states[0] &= !(1 << 12);
                }
            }
            async move { Ok(node) }
        })
        .await;
        assert!(result.unwrap_err().to_string().contains("changed"));
    }
    #[test]
    fn missing_or_ambiguous_fields_and_active_frames_fail() {
        for (editors, frames) in [(0, 1), (2, 1), (1, 0), (1, 2)] {
            assert!(ensure_unique_editor(editors, frames).is_err());
        }
    }
    #[tokio::test]
    async fn long_tree_uses_separate_bounded_filtered_queries() {
        let mut calls = 0;
        let candidates = bounded_candidates(":1.42", |rule, count| {
            let call = calls;
            calls += 1;
            async move {
                assert!(!rule.8);
                match call {
                    0 => {
                        assert_eq!(count, TREE_SENTINEL);
                        assert_eq!(rule.4, vec![0; 4]);
                        Ok((0..2000).map(reference).collect())
                    }
                    1 => {
                        assert_eq!(count, CANDIDATE_SENTINEL);
                        assert_eq!(rule.0, vec![(1 << 7) | (1 << 12), 0]);
                        assert_eq!(rule.4, vec![0, 1 << 29, 1 << 15, 0]);
                        assert_eq!(rule.5, 2);
                        Ok(vec![reference(1999)])
                    }
                    2 => {
                        assert_eq!(count, CANDIDATE_SENTINEL);
                        assert_eq!(rule.0, vec![1 << 1, 0]);
                        assert_eq!(rule.4, vec![1 << 23, 0, 0, 0]);
                        Ok(vec![reference(1)])
                    }
                    _ => panic!("Unexpected query"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(candidates.editors, vec![reference(1999)]);
        assert_eq!(candidates.frames, vec![reference(1)]);
        assert_eq!(candidates.all.len(), 2000);
    }
    #[tokio::test]
    async fn incomplete_foreign_duplicate_changed_and_unsupported_collections_fail() {
        let scenarios = [
            (
                (0..TREE_SENTINEL as usize).map(reference).collect(),
                vec![],
                vec![],
            ),
            (
                vec![(":1.99".into(), "/node/1".try_into().unwrap())],
                vec![],
                vec![],
            ),
            (
                vec![(":1.42".into(), NULL_PATH.try_into().unwrap())],
                vec![],
                vec![],
            ),
            (vec![reference(1), reference(1)], vec![], vec![]),
            (
                (0..100).map(reference).collect(),
                (0..CANDIDATE_SENTINEL as usize).map(reference).collect(),
                vec![],
            ),
            (vec![reference(1)], vec![reference(2)], vec![]),
            (vec![reference(1)], vec![reference(1), reference(1)], vec![]),
            (vec![reference(1)], vec![reference(1)], vec![reference(1)]),
            (
                (0..100).map(reference).collect(),
                vec![],
                (0..CANDIDATE_SENTINEL as usize).map(reference).collect(),
            ),
        ];
        for (all, editors, frames) in scenarios {
            let mut replies = std::collections::VecDeque::from([all, editors, frames]);
            assert!(
                bounded_candidates(":1.42", |_, _| {
                    let reply = replies.pop_front().unwrap();
                    async move { Ok(reply) }
                })
                .await
                .is_err()
            );
        }
        assert!(
            bounded_candidates(":1.42", |_, _| async {
                anyhow::bail!("Collection unsupported")
            })
            .await
            .is_err()
        );
    }
}
