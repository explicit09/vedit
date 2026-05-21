use serde_json::json;
use tempfile::tempdir;
use vedit_core::bisect::{BisectSession, BisectVerdict};
use vedit_core::commit::Author;
use vedit_core::repo::Repo;

fn author() -> Author {
    Author {
        name: "tester".to_string(),
        email: "test@example.com".to_string(),
    }
}

fn commit_named(repo: &Repo, name: &str) -> String {
    let timeline = json!({ "OTIO_SCHEMA": "Timeline.1", "name": name });
    let timeline_hash = repo.write_timeline(&timeline).unwrap();
    repo.commit(&timeline_hash, author(), name).unwrap()
}

#[test]
fn bisect_start_selects_middle_candidate() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let good = commit_named(&repo, "c1");
    let _c2 = commit_named(&repo, "c2");
    let expected_middle = commit_named(&repo, "c3");
    let _c4 = commit_named(&repo, "c4");
    let bad = commit_named(&repo, "c5");

    let session = BisectSession::start(&repo, &good, &bad).unwrap();

    assert_eq!(session.good, good);
    assert_eq!(session.bad, bad);
    assert_eq!(session.current.as_deref(), Some(expected_middle.as_str()));
    assert_eq!(session.remaining, 2);
}

#[test]
fn bisect_converges_on_first_bad_commit() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let good = commit_named(&repo, "c1");
    let _c2 = commit_named(&repo, "c2");
    let _c3 = commit_named(&repo, "c3");
    let first_bad = commit_named(&repo, "c4");
    let bad = commit_named(&repo, "c5");

    let session = BisectSession::start(&repo, &good, &bad).unwrap();
    let session = session
        .record(&repo, BisectVerdict::Good)
        .expect("c3 is good");
    assert_eq!(session.current.as_deref(), Some(first_bad.as_str()));

    let session = session
        .record(&repo, BisectVerdict::Bad)
        .expect("c4 is bad");
    assert_eq!(session.current, None);
    assert_eq!(session.first_bad.as_deref(), Some(first_bad.as_str()));
}

#[test]
fn bisect_rejects_good_ref_that_is_not_an_ancestor_of_bad_ref() {
    let dir = tempdir().unwrap();
    let repo = Repo::init(dir.path()).unwrap();
    let base = commit_named(&repo, "base");
    repo.create_branch("alt", "HEAD").unwrap();
    let bad = commit_named(&repo, "main-bad");
    repo.switch_branch("alt").unwrap();
    let unrelated_tip = commit_named(&repo, "alt-good");

    let err = BisectSession::start(&repo, &unrelated_tip, &bad).unwrap_err();
    assert!(
        err.to_string().contains("is not an ancestor"),
        "unexpected error: {err:#}"
    );
    assert!(BisectSession::start(&repo, &base, &bad).is_ok());
}
