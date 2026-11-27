use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use openraft::impls::Membership;
use openraft::{Instant, RaftMetrics, StoredMembership};

use super::super::Config;
use super::heard;

type Moment = openraft::type_config::alias::InstantOf<Config>;

/// Member 1's metrics in a group of `voters`, led by `leader`, last
/// acknowledged by a quorum `acked` ago.
fn metrics(voters: &[u64], leader: u64, acked: Option<Duration>) -> RaftMetrics<Config> {
    let mut metrics = RaftMetrics::new_initial(1);
    metrics.current_leader = Some(leader);
    let ids: BTreeSet<u64> = voters.iter().copied().collect();
    metrics.membership_config = Arc::new(StoredMembership::new(
        None,
        Membership::new_with_defaults(vec![ids], []),
    ));
    metrics.last_quorum_acked = acked.map(|ago| (Moment::now() - ago).into());
    metrics
}

/// A leader proposes while a quorum acknowledged it within half its lease,
/// and waits otherwise; a leader alone needs no quorum, and a member that
/// does not lead has nothing to wait for.
#[test]
fn a_leader_proposes_while_a_quorum_heard_it_lately() {
    let group = [1, 2, 3];
    assert!(heard(&metrics(&group, 1, Some(Duration::ZERO))));
    assert!(heard(&metrics(&group, 1, Some(Duration::from_millis(400)))));
    assert!(!heard(&metrics(
        &group,
        1,
        Some(Duration::from_millis(600))
    )));
    assert!(!heard(&metrics(&group, 1, None)));
    assert!(!heard(&metrics(&[1, 2], 1, None)));
    assert!(heard(&metrics(&[1], 1, None)));
    assert!(heard(&metrics(&group, 2, None)));
}
