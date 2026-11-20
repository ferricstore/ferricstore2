//! A restart rebuilds every shard from the log, even with another shard count.

use super::{TestResult, claim, complete, completion, create, directory, open};

#[test]
fn a_restart_replays_creates_claims_and_completions() -> TestResult {
    let directory = directory("restart")?;
    let ids: Vec<String> = (0..100).map(|index| format!("r{index}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let held = {
        let node = open(&directory, 4)?;
        assert!(create(&node, &id_refs)?.iter().all(Result::is_ok));
        let claimed = claim(&node, 30)?;
        assert_eq!(claimed.len(), 30);
        let (done, held) = claimed.split_at(10);
        let outcomes = complete(&node, done.iter().map(completion).collect())?;
        assert!(outcomes.iter().all(Result::is_ok));
        held.to_vec()
    };
    for shards in [4, 7] {
        let node = open(&directory, shards)?;
        let created_again = create(&node, &["r0"])?;
        assert_eq!(created_again.len(), 1);
        assert!(
            created_again.iter().all(Result::is_err),
            "r0 was created twice"
        );
        drop(node);
    }
    let node = open(&directory, 7)?;
    let remaining = claim(&node, 500)?;
    assert_eq!(
        remaining.len(),
        70,
        "only unclaimed flows are due after restart"
    );
    let outcomes = complete(&node, held.iter().map(completion).collect())?;
    assert!(
        outcomes.iter().all(Result::is_ok),
        "a lease from before the restart: {outcomes:?}"
    );
    drop(node);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
