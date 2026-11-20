//! FIFO states (ADR-014 step 9b): one Flow at a time per partition, oldest
//! first; a running or not-yet-due head blocks its lane, a finished head
//! frees it (waking parked claims), entering a FIFO state needs an explicit
//! partition and priority 0, and lane order survives a restart.

use std::sync::mpsc;
use std::time::Duration;

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{ClaimedFlow, QueueService, ServiceError};
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

fn text(value: &str) -> Term {
    Term::text(value)
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &options(entries))?;
    call(|reply| node.execute(command, reply))
}

/// Makes `queued` of `flow_type` a FIFO state with a fixed 100 ms retry.
fn fifo_policy(node: &QueueNode, flow_type: &str) -> TestResult {
    let retry = options(&[(
        "backoff",
        options(&[
            ("kind", text("fixed")),
            ("base_ms", Term::Int(100)),
            ("jitter_pct", Term::Int(0)),
        ]),
    )]);
    run(
        node,
        "FLOW.POLICY.SET",
        &[
            ("type", text(flow_type)),
            (
                "states",
                options(&[(
                    "queued",
                    options(&[("mode", text("fifo")), ("retry", retry)]),
                )]),
            ),
        ],
    )??;
    Ok(())
}

fn create(
    node: &QueueNode,
    (flow_type, id, partition): (&str, &str, &str),
    now: Option<i64>,
    run_at: i64,
) -> TestResult<Result<Term, ServiceError>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("partition_key", text(partition)),
        ("run_at_ms", Term::Int(run_at)),
    ];
    entries.extend(now.map(|now| ("now_ms", Term::Int(now))));
    run(node, "FLOW.CREATE", &entries)
}

fn claim_options(
    flow_type: &str,
    partitions: &[&str],
    now: Option<i64>,
) -> Vec<(&'static str, Term)> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("w")),
        ("limit", Term::Int(10)),
        ("state", text("queued")),
        (
            "partition_keys",
            Term::List(partitions.iter().map(|name| text(name)).collect()),
        ),
    ];
    entries.extend(now.map(|now| ("now_ms", Term::Int(now))));
    entries
}

fn claim(
    node: &QueueNode,
    flow_type: &str,
    partitions: &[&str],
    now: Option<i64>,
) -> TestResult<Vec<ClaimedFlow>> {
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&claim_options(flow_type, partitions, now)),
    )?;
    let mut claimed = call(|reply| node.claim(parsed.request, reply))??;
    claimed.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(claimed)
}

fn ids(claimed: &[ClaimedFlow]) -> Vec<&[u8]> {
    claimed.iter().map(|job| job.id.as_slice()).collect()
}

fn finish(node: &QueueNode, name: &str, job: &ClaimedFlow, now: Option<i64>) -> TestResult {
    let mut entries = vec![
        ("id", Term::Bin(job.id.clone())),
        ("partition_key", Term::Bin(job.partition.clone())),
        ("lease_token", Term::Bin(job.lease_token.clone())),
        ("fencing_token", Term::int(job.fence)),
    ];
    entries.extend(now.map(|now| ("now_ms", Term::Int(now))));
    run(node, name, &entries)??;
    Ok(())
}

fn find<'a>(claimed: &'a [ClaimedFlow], id: &[u8]) -> TestResult<&'a ClaimedFlow> {
    Ok(claimed
        .iter()
        .find(|job| job.id == id)
        .ok_or("expected Flow not claimed")?)
}

/// Guards, then one head per partition through a retry and a completion.
fn lanes_take_heads_in_order(node: &QueueNode) -> TestResult {
    fifo_policy(node, "tf")?;
    assert_eq!(
        run(
            node,
            "FLOW.CREATE",
            &[("id", text("x")), ("type", text("tf"))]
        )?,
        Err(ServiceError::rejected(
            "ERR flow partition_key is required for fifo state".to_owned()
        ))
    );
    assert_eq!(
        run(
            node,
            "FLOW.CREATE",
            &[
                ("id", text("y")),
                ("type", text("tf")),
                ("partition_key", text("pa")),
                ("priority", Term::Int(1)),
            ]
        )?,
        Err(ServiceError::rejected(
            "ERR flow priority is not supported for fifo state".to_owned()
        ))
    );
    for (offset, id) in ["a0", "a1", "a2"].into_iter().enumerate() {
        let at = 1_000 + i64::try_from(offset)?;
        create(node, ("tf", id, "pa"), Some(at), at)??;
    }
    for (offset, id) in ["b0", "b1"].into_iter().enumerate() {
        let at = 1_010 + i64::try_from(offset)?;
        create(node, ("tf", id, "pb"), Some(at), at)??;
    }
    let first = claim(node, "tf", &["pa", "pb"], Some(1_100))?;
    assert_eq!(ids(&first), vec![b"a0".as_slice(), b"b0".as_slice()]);
    assert_eq!(
        ids(&claim(node, "tf", &["pa", "pb"], Some(1_101))?),
        Vec::<&[u8]>::new()
    );
    // A retried head keeps its place: its lane waits until it is due.
    finish(node, "FLOW.RETRY", find(&first, b"a0")?, Some(1_102))?;
    assert_eq!(
        ids(&claim(node, "tf", &["pa"], Some(1_103))?),
        Vec::<&[u8]>::new()
    );
    finish(node, "FLOW.COMPLETE", find(&first, b"b0")?, Some(1_104))?;
    assert_eq!(
        ids(&claim(node, "tf", &["pa", "pb"], Some(1_105))?),
        vec![b"b1".as_slice()]
    );
    let again = claim(node, "tf", &["pa"], Some(1_202))?;
    assert_eq!(ids(&again), vec![b"a0".as_slice()]);
    finish(node, "FLOW.COMPLETE", find(&again, b"a0")?, Some(1_203))
}

/// A policy set after its Flows exist orders them by when they entered the
/// state, not by due time.
fn late_policy_orders_by_entry(node: &QueueNode) -> TestResult {
    for (offset, id) in ["c0", "c1", "c2"].into_iter().enumerate() {
        let at = i64::try_from(offset)?;
        create(node, ("tl", id, "pc"), Some(1_400 + at), 1_500 - at)??;
    }
    fifo_policy(node, "tl")?;
    assert_eq!(
        ids(&claim(node, "tl", &["pc"], Some(1_600))?),
        vec![b"c0".as_slice()]
    );
    Ok(())
}

/// A Flow that enters a FIFO state joins its lane behind the Flows already
/// there, however long it existed before.
fn entering_joins_the_tail(node: &QueueNode) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("e0")),
            ("type", text("tf")),
            ("state", text("start")),
            ("partition_key", text("pe")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(1_000)),
        ],
    )??;
    create(node, ("tf", "e1", "pe"), Some(1_001), 1_001)??;
    let mut entries = claim_options("tf", &["pe"], Some(1_002));
    entries.retain(|(key, _)| *key != "state");
    entries.push(("state", text("start")));
    let parsed = flow::claim::parse("FLOW.CLAIM_DUE", &options(&entries))?;
    let started = call(|reply| node.claim(parsed.request, reply))??;
    let job = find(&started, b"e0")?;
    run(
        node,
        "FLOW.TRANSITION",
        &[
            ("id", text("e0")),
            ("partition_key", text("pe")),
            ("from_state", text("running")),
            ("to_state", text("queued")),
            ("lease_token", Term::Bin(job.lease_token.clone())),
            ("fencing_token", Term::int(job.fence)),
            ("now_ms", Term::Int(1_003)),
            ("run_at_ms", Term::Int(1_003)),
        ],
    )??;
    assert_eq!(
        ids(&claim(node, "tf", &["pe"], Some(1_004))?),
        vec![b"e1".as_slice()]
    );
    Ok(())
}

#[test]
fn fifo_lanes_take_one_head_per_partition_and_keep_order_across_a_restart() -> TestResult {
    let path = directory("flow-fifo")?;
    {
        let node = open(&path, 4)?;
        lanes_take_heads_in_order(&node)?;
        entering_joins_the_tail(&node)?;
        late_policy_orders_by_entry(&node)?;
    }
    let node = open(&path, 3)?;
    assert_eq!(
        ids(&claim(&node, "tf", &["pa"], Some(1_300))?),
        vec![b"a1".as_slice()]
    );
    assert_eq!(
        ids(&claim(&node, "tl", &["pc"], Some(1_700))?),
        Vec::<&[u8]>::new()
    );
    Ok(())
}

#[test]
fn a_finished_head_wakes_a_parked_claim_for_its_lane() -> TestResult {
    let path = directory("flow-fifo-wake")?;
    let node = open(&path, 4)?;
    fifo_policy(&node, "tw")?;
    create(&node, ("tw", "w0", "pw"), None, 0)??;
    create(&node, ("tw", "w1", "pw"), None, 0)??;
    let first = claim(&node, "tw", &["pw"], None)?;
    assert_eq!(ids(&first), vec![b"w0".as_slice()]);
    let mut entries = claim_options("tw", &["pw"], None);
    entries.push(("block_ms", Term::Int(10_000)));
    let parsed = flow::claim::parse("FLOW.CLAIM_DUE", &options(&entries))?;
    let (sender, waiting) = mpsc::channel();
    node.claim(
        parsed.request,
        Box::new(move |result| {
            let _delivered = sender.send(result);
        }),
    );
    std::thread::sleep(Duration::from_millis(200));
    assert!(waiting.try_recv().is_err(), "claimed past a running head");
    finish(&node, "FLOW.COMPLETE", find(&first, b"w0")?, None)?;
    let woken = waiting.recv_timeout(Duration::from_secs(5))??;
    assert_eq!(ids(&woken), vec![b"w1".as_slice()]);
    Ok(())
}
