//! End-to-end test of the v0.3 branching surface.
//!
//! This exercises the full divergent-branches workflow: init, commit on
//! main, branch alt, switch, commit on alt, switch back, verify the two
//! branches point at different commits and have different histories.

use serde_json::json;
use tempfile::tempdir;
use vedit_core::commit::Author;
use vedit_core::repo::Repo;

fn author() -> Author {
    Author {
        name: "tester".to_string(),
        email: "test@example.com".to_string(),
    }
}

fn timeline(name: &str) -> serde_json::Value {
    json!({
        "OTIO_SCHEMA": "Timeline.1",
        "name": name,
        "tracks": {
            "OTIO_SCHEMA": "Stack.1",
            "name": "tracks",
            "children": []
        }
    })
}

#[test]
fn branches_diverge_independently() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();

    // First commit on main.
    let t0 = repo.write_timeline(&timeline("v0")).unwrap();
    let main_c0 = repo.commit(&t0, author(), "v0 on main").unwrap();

    // Branch alt at HEAD.
    repo.create_branch("alt", "HEAD").unwrap();
    assert_eq!(
        repo.branch_target("alt").unwrap().as_deref(),
        Some(main_c0.as_str())
    );

    // Commit on main first; alt should not move.
    let t1 = repo.write_timeline(&timeline("v1")).unwrap();
    let main_c1 = repo.commit(&t1, author(), "v1 on main").unwrap();
    assert_eq!(
        repo.branch_target("main").unwrap().as_deref(),
        Some(main_c1.as_str())
    );
    assert_eq!(
        repo.branch_target("alt").unwrap().as_deref(),
        Some(main_c0.as_str()),
        "alt should still point at the original commit"
    );

    // Switch to alt and commit; main should not move.
    repo.switch_branch("alt").unwrap();
    let t2 = repo.write_timeline(&timeline("alt-v1")).unwrap();
    let alt_c1 = repo.commit(&t2, author(), "alt-v1").unwrap();
    assert_eq!(
        repo.branch_target("alt").unwrap().as_deref(),
        Some(alt_c1.as_str())
    );
    assert_eq!(
        repo.branch_target("main").unwrap().as_deref(),
        Some(main_c1.as_str()),
        "main should stay where it was"
    );

    // Logs should reflect the divergence.
    let main_log = repo.log(Some("main")).unwrap();
    let alt_log = repo.log(Some("alt")).unwrap();

    assert_eq!(main_log.len(), 2);
    assert_eq!(main_log[0].0, main_c1);
    assert_eq!(main_log[1].0, main_c0);

    assert_eq!(alt_log.len(), 2);
    assert_eq!(alt_log[0].0, alt_c1);
    assert_eq!(alt_log[1].0, main_c0);

    // The shared ancestor is main_c0.
    assert_eq!(main_log[1].0, alt_log[1].0);

    // list_branches reports both, sorted.
    let list = repo.list_branches().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].0, "alt");
    assert_eq!(list[1].0, "main");
}

#[test]
fn switch_branch_and_check_current() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let t = repo.write_timeline(&timeline("v")).unwrap();
    repo.commit(&t, author(), "v").unwrap();

    repo.create_branch("alt", "HEAD").unwrap();
    assert_eq!(repo.current_branch().unwrap().as_deref(), Some("main"));
    repo.switch_branch("alt").unwrap();
    assert_eq!(repo.current_branch().unwrap().as_deref(), Some("alt"));
}

#[test]
fn delete_branch_then_recreate() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let t = repo.write_timeline(&timeline("v")).unwrap();
    repo.commit(&t, author(), "v").unwrap();

    repo.create_branch("scratch", "HEAD").unwrap();
    assert!(repo.branch_target("scratch").unwrap().is_some());
    repo.delete_branch("scratch").unwrap();
    assert!(repo.branch_target("scratch").unwrap().is_none());
    // Recreating the same name should now succeed.
    repo.create_branch("scratch", "HEAD").unwrap();
    assert!(repo.branch_target("scratch").unwrap().is_some());
}
