//! `FLOW.LIMIT.*` (ADR-014 step 15), ported from Elixir's governance tests:
//! leases grant up to what is free, spends move credits into use under
//! reservation ids, releases return them and start a new generation,
//! expired leases are reclaimed even while in use, configuration versions
//! guard limit changes, and shard ids are checked against the node.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::ServiceError;
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    match flow::parse(name, &options(entries)) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(error) => Ok(Err(error)),
    }
}

fn field(term: &Term, path: &[&str]) -> Term {
    path.iter()
        .try_fold(term, |term, key| term.get(key.as_bytes()))
        .cloned()
        .unwrap_or(Term::Nil)
}

fn limit(
    node: &QueueNode,
    name: &str,
    scope: &str,
    entries: &[(&str, i64)],
) -> TestResult<Result<Term, ServiceError>> {
    let mut all = vec![("scope", Term::text(scope))];
    all.extend(entries.iter().map(|(key, value)| (*key, Term::Int(*value))));
    run(node, name, &all)
}

#[test]
fn leases_spends_and_releases_move_credits() -> TestResult {
    let path = directory("flow-limit")?;
    let ids = {
        let node = open(&path, 4)?;
        let leased = limit(
            &node,
            "FLOW.LIMIT.LEASE",
            "l1",
            &[
                ("shard_id", 1),
                ("amount", 6),
                ("limit", 10),
                ("ttl_ms", 1_000),
                ("now_ms", 1_000),
            ],
        )??;
        assert_eq!(field(&leased, &["owner", "free"]), Term::Int(4));
        assert_eq!(field(&leased, &["lease", "available"]), Term::Int(6));
        let spent = limit(
            &node,
            "FLOW.LIMIT.SPEND",
            "l1",
            &[("shard_id", 1), ("amount", 4), ("now_ms", 1_001)],
        )??;
        assert_eq!(field(&spent, &["owner", "free"]), Term::Int(4));
        assert_eq!(field(&spent, &["lease", "available"]), Term::Int(2));
        assert_eq!(field(&spent, &["lease", "in_use"]), Term::Int(4));
        let partial = limit(
            &node,
            "FLOW.LIMIT.LEASE",
            "l1",
            &[
                ("shard_id", 2),
                ("amount", 10),
                ("ttl_ms", 1_000),
                ("now_ms", 1_002),
            ],
        )??;
        assert_eq!(field(&partial, &["owner", "free"]), Term::Int(0));
        assert_eq!(field(&partial, &["lease", "available"]), Term::Int(4));
        let Err(ServiceError::Governance(denial)) = limit(
            &node,
            "FLOW.LIMIT.LEASE",
            "l1",
            &[
                ("shard_id", 3),
                ("amount", 1),
                ("ttl_ms", 1_000),
                ("now_ms", 1_003),
            ],
        )?
        else {
            return Err("expected an exhausted limit".into());
        };
        assert_eq!(
            field(&denial.payload(), &["code"]),
            Term::text("GOVERNANCE_LIMIT_EXCEEDED")
        );
        // The refused shard marked a credit for reclaim on the lowest lease.
        let marked = limit(&node, "FLOW.LIMIT.GET", "l1", &[("now_ms", 1_004)])??;
        assert_eq!(
            field(&marked, &["leases", "1", "pending_reclaim"]),
            Term::Int(1)
        );
        field(&spent, &["reservation_ids"])
    };
    // A restart keeps the credits and the reservation ids in use.
    let node = open(&path, 4)?;
    let released = run(
        &node,
        "FLOW.LIMIT.RELEASE",
        &[
            ("scope", Term::text("l1")),
            ("shard_id", Term::Int(1)),
            ("reservation_ids", ids.clone()),
        ],
    )??;
    assert_eq!(
        field(&released, &["leases", "1", "available"]),
        Term::Int(6)
    );
    assert_eq!(field(&released, &["leases", "1", "in_use"]), Term::Int(0));
    // Nothing in use: the lease starts a new generation.
    assert_eq!(field(&released, &["leases", "1", "epoch"]), Term::Int(3));
    let again = run(
        &node,
        "FLOW.LIMIT.RELEASE",
        &[
            ("scope", Term::text("l1")),
            ("shard_id", Term::Int(1)),
            ("reservation_ids", ids),
        ],
    )??;
    assert_eq!(again, released);
    let reclaimed = limit(&node, "FLOW.LIMIT.GET", "l1", &[("now_ms", 2_003)])??;
    assert_eq!(field(&reclaimed, &["free"]), Term::Int(10));
    assert_eq!(field(&reclaimed, &["leases"]), Term::Map(Vec::new()));
    Ok(())
}

#[test]
fn expired_leases_in_use_are_reclaimed() -> TestResult {
    let node = open(&directory("flow-limit-expired")?, 4)?;
    limit(
        &node,
        "FLOW.LIMIT.LEASE",
        "e1",
        &[
            ("shard_id", 1),
            ("amount", 1),
            ("limit", 1),
            ("ttl_ms", 100),
            ("now_ms", 1_000),
        ],
    )??;
    limit(
        &node,
        "FLOW.LIMIT.SPEND",
        "e1",
        &[("shard_id", 1), ("amount", 1), ("now_ms", 1_001)],
    )??;
    // A lease lapses at its expiry time.
    let owner = limit(&node, "FLOW.LIMIT.GET", "e1", &[("now_ms", 1_100)])??;
    assert_eq!(field(&owner, &["free"]), Term::Int(1));
    assert_eq!(field(&owner, &["leases"]), Term::Map(Vec::new()));
    // A spend on a lapsed lease is refused as exhausted.
    assert!(matches!(
        limit(
            &node,
            "FLOW.LIMIT.SPEND",
            "e1",
            &[("shard_id", 1), ("amount", 1), ("now_ms", 1_102)]
        )?,
        Err(ServiceError::Governance(_))
    ));
    Ok(())
}

#[test]
fn configuration_versions_guard_limit_changes() -> TestResult {
    let node = open(&directory("flow-limit-config")?, 4)?;
    let lease = |entries: &[(&str, i64)]| {
        let mut all = vec![
            ("shard_id", 0),
            ("amount", 1),
            ("ttl_ms", 1_000),
            ("now_ms", 10),
        ];
        all.extend(entries.iter().copied());
        limit(&node, "FLOW.LIMIT.LEASE", "c1", &all)
    };
    assert_eq!(
        lease(&[])?,
        Err(ServiceError::rejected(
            "ERR flow limit limit must be a non-negative integer".to_owned()
        ))
    );
    lease(&[("limit", 5)])??;
    assert_eq!(
        lease(&[("limit", 3)])?,
        Err(ServiceError::rejected(
            "ERR flow limit config_version is required to change limit".to_owned()
        ))
    );
    let raised = lease(&[("limit", 3), ("config_version", 2)])??;
    assert_eq!(field(&raised, &["owner", "limit"]), Term::Int(3));
    assert_eq!(
        lease(&[("limit", 4), ("config_version", 2)])?,
        Err(ServiceError::rejected(
            "ERR flow limit config_version conflict".to_owned()
        ))
    );
    let older = lease(&[("limit", 9), ("config_version", 1)])??;
    assert_eq!(field(&older, &["owner", "limit"]), Term::Int(3));
    // A numeric policy version is its own configuration version, and its
    // fingerprint is Elixir's.
    let versioned = limit(
        &node,
        "FLOW.LIMIT.LEASE",
        "c2",
        &[
            ("shard_id", 0),
            ("amount", 1),
            ("limit", 3),
            ("policy_version", 7),
            ("ttl_ms", 1_000),
            ("now_ms", 1),
        ],
    )??;
    assert_eq!(
        field(&versioned, &["owner", "config_version"]),
        Term::Int(7)
    );
    assert_eq!(
        field(&versioned, &["owner", "policy_version_hash"]),
        Term::text("DkKNcZoMDC2l16Duf-rTdbJKU3ZmpNKz1IsFovkce6U")
    );
    assert_eq!(
        limit(
            &node,
            "FLOW.LIMIT.LEASE",
            "c3",
            &[("shard_id", 4), ("amount", 1), ("limit", 1), ("ttl_ms", 1)]
        )?,
        Err(ServiceError::rejected(
            "ERR flow limit shard_id must be between 0 and 3".to_owned()
        ))
    );
    assert_eq!(
        limit(
            &node,
            "FLOW.LIMIT.SPEND",
            "missing",
            &[("shard_id", 0), ("amount", 1)]
        )?,
        Err(ServiceError::rejected(
            "ERR flow limit not found".to_owned()
        ))
    );
    Ok(())
}

#[test]
fn spends_and_lower_limits_respect_what_is_leased() -> TestResult {
    let node = open(&directory("flow-limit-drain")?, 4)?;
    limit(
        &node,
        "FLOW.LIMIT.LEASE",
        "d1",
        &[
            ("shard_id", 0),
            ("amount", 5),
            ("limit", 5),
            ("ttl_ms", 1_000),
            ("now_ms", 1),
        ],
    )??;
    // A spend beyond the lease's available credits is refused.
    assert!(matches!(
        limit(
            &node,
            "FLOW.LIMIT.SPEND",
            "d1",
            &[("shard_id", 0), ("amount", 6), ("now_ms", 2)]
        )?,
        Err(ServiceError::Governance(_))
    ));
    // A lower limit drains the lease's available credits to fit.
    assert!(matches!(
        limit(
            &node,
            "FLOW.LIMIT.LEASE",
            "d1",
            &[
                ("shard_id", 0),
                ("amount", 1),
                ("limit", 2),
                ("config_version", 1),
                ("ttl_ms", 1_000),
                ("now_ms", 3)
            ]
        )?,
        Err(ServiceError::Governance(_))
    ));
    let owner = limit(&node, "FLOW.LIMIT.GET", "d1", &[("now_ms", 4)])??;
    assert_eq!(field(&owner, &["limit"]), Term::Int(2));
    assert_eq!(field(&owner, &["leases", "0", "available"]), Term::Int(2));
    assert_eq!(field(&owner, &["free"]), Term::Int(0));
    Ok(())
}
