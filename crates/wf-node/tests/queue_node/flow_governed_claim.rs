//! Governed claims (ADR-014 step 15), ported from Elixir's governance
//! tests: a claim naming a limit spends a credit per Flow (through the
//! node's cache of credits spent ahead), is refused when none is left and
//! work is due, answers with nothing when no work is due, and a Flow's
//! credit is released when it leaves `running`; a policy's running limit
//! governs claims the same way, leasing on demand. Claimed Flows holding
//! one payload log it once and replay with their leases.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{ClaimedFlow, QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn log_bytes(path: &std::path::Path) -> TestResult<u64> {
    let mut total = 0;
    for entry in std::fs::read_dir(path.join("log"))? {
        total += entry?.metadata()?.len();
    }
    Ok(total)
}

fn map(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: Vec<(&str, Term)>,
) -> TestResult<Result<Term, ServiceError>> {
    match flow::parse(name, &map(entries)) {
        Ok(command) => call(|reply| node.execute(command, reply)),
        Err(error) => Ok(Err(error)),
    }
}

fn create(node: &QueueNode, id: &str, flow_type: &str, run_at: i64) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        vec![
            ("id", text(id)),
            ("type", text(flow_type)),
            ("partition_key", text("p")),
            ("run_at_ms", Term::Int(run_at)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    Ok(())
}

fn claim(
    node: &QueueNode,
    flow_type: &str,
    now: i64,
    scope: Option<&str>,
) -> TestResult<Result<Vec<ClaimedFlow>, ServiceError>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("state", text("queued")),
        ("partition_key", text("p")),
        ("worker", text("worker-1")),
        ("limit", Term::Int(1)),
        ("now_ms", Term::Int(now)),
    ];
    if let Some(scope) = scope {
        entries.extend([
            ("governance_limit_scope", text(scope)),
            ("governance_shard_id", Term::Int(0)),
        ]);
    }
    let parsed = flow::claim::parse("FLOW.CLAIM_DUE", &map(entries))?;
    call(|reply| QueueService::claim(node, parsed.request, reply))
}

fn complete(node: &QueueNode, job: &ClaimedFlow, now: i64) -> TestResult {
    run(
        node,
        "FLOW.COMPLETE",
        vec![
            ("id", Term::Bin(job.id.clone())),
            ("partition_key", text("p")),
            ("lease_token", Term::Bin(job.lease_token.clone())),
            ("fencing_token", Term::Int(i64::try_from(job.fence)?)),
            ("now_ms", Term::Int(now)),
        ],
    )??;
    Ok(())
}

fn lease(node: &QueueNode, scope: &str, credits: i64) -> TestResult {
    run(
        node,
        "FLOW.LIMIT.LEASE",
        vec![
            ("scope", text(scope)),
            ("shard_id", Term::Int(0)),
            ("amount", Term::Int(credits)),
            ("limit", Term::Int(credits)),
            ("ttl_ms", Term::Int(30_000)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    Ok(())
}

/// The lease's available and in-use credits, once releases have landed.
fn credits(node: &QueueNode, scope: &str, shard: &str) -> TestResult<(Term, Term)> {
    std::thread::sleep(std::time::Duration::from_millis(50));
    let owner = run(
        node,
        "FLOW.LIMIT.GET",
        vec![("scope", text(scope)), ("now_ms", Term::Int(1_500))],
    )??;
    let lease = owner
        .get(b"leases")
        .and_then(|leases| leases.get(shard.as_bytes()))
        .cloned()
        .unwrap_or(Term::Nil);
    let field = |key: &[u8]| lease.get(key).cloned().unwrap_or(Term::Nil);
    Ok((field(b"available"), field(b"in_use")))
}

fn first(claimed: Result<Vec<ClaimedFlow>, ServiceError>) -> TestResult<ClaimedFlow> {
    Ok(claimed?.into_iter().next().ok_or("nothing claimed")?)
}

#[test]
fn credits_govern_claims_and_terminal_moves_free_them() -> TestResult {
    let node = open(&directory("flow-governed-claim")?, 4)?;
    lease(&node, "g1", 1)?;
    create(&node, "a", "ta", 1_000)?;
    create(&node, "b", "ta", 1_000)?;
    let held = first(claim(&node, "ta", 1_001, Some("g1"))?)?;
    let Err(ServiceError::Governance(denial)) = claim(&node, "ta", 1_002, Some("g1"))? else {
        return Err("expected an exhausted limit".into());
    };
    assert_eq!(
        denial.payload().get(b"code"),
        Some(&text("GOVERNANCE_LIMIT_EXCEEDED"))
    );
    complete(&node, &held, 1_003)?;
    assert_eq!(claim(&node, "ta", 1_004, Some("g1"))??.len(), 1);
    Ok(())
}

#[test]
fn claims_take_credits_spent_ahead_and_releases_return_each() -> TestResult {
    let node = open(&directory("flow-governed-cache")?, 4)?;
    lease(&node, "g2", 4)?;
    for id in ["c1", "c2", "c3"] {
        create(&node, id, "tc", 1_000)?;
    }
    let one = first(claim(&node, "tc", 1_001, Some("g2"))?)?;
    assert_eq!(credits(&node, "g2", "0")?, (Term::Int(0), Term::Int(4)));
    let two = first(claim(&node, "tc", 1_003, Some("g2"))?)?;
    assert_eq!(credits(&node, "g2", "0")?, (Term::Int(0), Term::Int(4)));
    assert_ne!(one.id, two.id);
    // A completion releases the Flow's own credit, options or not.
    complete(&node, &one, 1_005)?;
    assert_eq!(credits(&node, "g2", "0")?, (Term::Int(1), Term::Int(3)));
    // A claim with no due work releases the credit it took.
    let three = first(claim(&node, "tc", 1_006, Some("g2"))?)?;
    assert_eq!(claim(&node, "tc", 1_007, Some("g2"))??.len(), 0);
    assert_eq!(credits(&node, "g2", "0")?, (Term::Int(2), Term::Int(2)));
    complete(&node, &three, 1_008)?;
    Ok(())
}

#[test]
fn an_empty_claim_spends_nothing() -> TestResult {
    let node = open(&directory("flow-governed-empty")?, 4)?;
    lease(&node, "g3", 1)?;
    run(
        &node,
        "FLOW.LIMIT.SPEND",
        vec![
            ("scope", text("g3")),
            ("shard_id", Term::Int(0)),
            ("amount", Term::Int(1)),
            ("now_ms", Term::Int(1_001)),
        ],
    )??;
    create(&node, "later", "te", 60_000)?;
    assert_eq!(claim(&node, "te", 1_002, Some("g3"))??.len(), 0);
    assert_eq!(credits(&node, "g3", "0")?, (Term::Int(0), Term::Int(1)));
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &map(vec![
            ("type", text("te")),
            ("worker", text("w")),
            ("governance_limit_scope", text("g3")),
        ]),
    )?;
    assert_eq!(
        call(|reply| QueueService::claim(&node, parsed.request, reply))?.err(),
        Some(ServiceError::rejected(
            "ERR flow governance_limit_scope and governance_shard_id must be provided together"
                .to_owned()
        ))
    );
    Ok(())
}

#[test]
fn a_policy_running_limit_governs_claims_across_restarts() -> TestResult {
    let path = directory("flow-governed-policy")?;
    let policy = map(vec![(
        "queued",
        map(vec![(
            "governance",
            map(vec![(
                "limits",
                map(vec![("running", map(vec![("limit", Term::Int(1))]))]),
            )]),
        )]),
    )]);
    let held = {
        let node = open(&path, 4)?;
        run(
            &node,
            "FLOW.POLICY.SET",
            vec![("type", text("tp")), ("states", policy)],
        )??;
        create(&node, "p1", "tp", 1_000)?;
        create(&node, "p2", "tp", 1_000)?;
        let held = first(claim(&node, "tp", 1_001, None)?)?;
        assert!(matches!(
            claim(&node, "tp", 1_002, None)?,
            Err(ServiceError::Governance(_))
        ));
        held
    };
    // The rules and the Flow's credit survive a restart.
    let node = open(&path, 4)?;
    assert!(matches!(
        claim(&node, "tp", 1_003, None)?,
        Err(ServiceError::Governance(_))
    ));
    complete(&node, &held, 1_004)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(claim(&node, "tp", 1_005, None)??.len(), 1);
    Ok(())
}

#[test]
fn a_refused_batch_keeps_its_flows_credit() -> TestResult {
    // A batch put back after a Flow left `running` (audit gap G28) must not
    // release that Flow's credit: the Flow runs on, and the limit holds.
    let node = open(&directory("flow-governed-refused-batch")?, 4)?;
    lease(&node, "g5", 1)?;
    create(&node, "held", "tr", 1_000)?;
    create(&node, "waiting", "tr", 1_000)?;
    let held = first(claim(&node, "tr", 1_001, Some("g5"))?)?;
    let item = |id: Term, token: Term, fence: i64| {
        map(vec![
            ("id", id),
            ("lease_token", token),
            ("fencing_token", Term::Int(fence)),
        ])
    };
    let refused = run(
        &node,
        "FLOW.COMPLETE_MANY",
        vec![
            ("partition_key", text("p")),
            (
                "items",
                Term::List(vec![
                    item(
                        Term::Bin(held.id.clone()),
                        Term::Bin(held.lease_token.clone()),
                        i64::try_from(held.fence)?,
                    ),
                    item(text("waiting"), text("no-lease"), 0),
                ]),
            ),
            ("now_ms", Term::Int(1_002)),
        ],
    )?;
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(credits(&node, "g5", "0")?, (Term::Int(0), Term::Int(1)));
    assert!(matches!(
        claim(&node, "tr", 1_003, Some("g5"))?,
        Err(ServiceError::Governance(_))
    ));
    complete(&node, &held, 1_004)?;
    assert_eq!(claim(&node, "tr", 1_005, Some("g5"))??.len(), 1);
    Ok(())
}

#[test]
fn a_governed_claim_logs_a_shared_payload_once_and_replays_the_leases() -> TestResult {
    // A governed claim logs each Flow's after-image, lease included; Flows
    // holding one payload share a record (perf note P1).
    let path = directory("governed-claim-shared-payload")?;
    let node = open(&path, 1)?;
    lease(&node, "g-shared", 3)?;
    let payload = text(&"p".repeat(4_096));
    for id in ["s1", "s2", "s3"] {
        run(
            &node,
            "FLOW.CREATE",
            vec![
                ("id", text(id)),
                ("type", text("shared")),
                ("partition_key", text("p")),
                ("payload", payload.clone()),
                ("run_at_ms", Term::Int(1_000)),
                ("now_ms", Term::Int(1_000)),
            ],
        )??;
    }
    let before = log_bytes(&path)?;
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &map(vec![
            ("type", text("shared")),
            ("state", text("queued")),
            ("partition_key", text("p")),
            ("worker", text("worker-1")),
            ("limit", Term::Int(3)),
            ("now_ms", Term::Int(1_100)),
            ("governance_limit_scope", text("g-shared")),
            ("governance_shard_id", Term::Int(0)),
        ]),
    )?;
    let claimed = call(|reply| QueueService::claim(&node, parsed.request, reply))??;
    assert_eq!(claimed.len(), 3);
    let logged = log_bytes(&path)?.saturating_sub(before);
    assert!(logged < 2 * 4_096, "the claim logged {logged} bytes");
    drop(node);
    let node = open(&path, 1)?;
    for job in &claimed {
        let get = vec![
            ("id", Term::Bin(job.id.clone())),
            ("partition_key", text("p")),
            ("full", Term::Bool(true)),
        ];
        let flow = run(&node, "FLOW.GET", get)??;
        assert_eq!(flow.get(b"state"), Some(&text("running")));
        assert_eq!(flow.get(b"payload"), Some(&payload));
        complete(&node, job, 1_200)?;
    }
    drop(node);
    std::fs::remove_dir_all(path)?;
    Ok(())
}
