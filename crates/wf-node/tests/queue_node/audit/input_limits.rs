//! `flow/flow_input_limits_test.exs`: a new Flow's id and partition take up
//! to 256 bytes, its type 128, its states 64, its correlation id 256, a
//! worker name 256 and a child group id 128; a longer one is refused and
//! nothing is written. (Its Flows stored before the limits came through
//! Elixir's uncapped internal create, which the node has no form of; its
//! `reschedule` has no wire command.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, get, int, map, refusal, run, text};
use crate::{TestResult, directory, open};

fn bytes(count: usize) -> String {
    "s".repeat(count)
}

fn id_of(count: usize) -> String {
    "x".repeat(count)
}

const LONG_ID: &str = "ERR flow id is longer than 256 bytes";
const LONG_PARTITION: &str = "ERR flow partition_key is longer than 256 bytes";
const LONG_STATE: &str = "ERR flow state is longer than 64 bytes";

fn create(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("id-limit")),
        ("partition_key", text("input-limits")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

fn start(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text("id-limit")),
        ("initial_state", text("queued")),
        ("partition_key", text("input-limits")),
        ("worker", text("w")),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.START_AND_CLAIM", &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn ids_take_256_bytes_and_a_longer_one_writes_nothing() -> TestResult {
    // Elixir: "public create takes a 256-byte id and rejects a longer one
    // without writing it", "a batch with one long id writes none of its
    // items" and "start and claim rejects a long id".
    let node = open(&directory("audit-limits-ids")?, 4)?;
    let in_partition = [("partition_key", text("input-limits"))];
    create(&node, &id_of(256), &[])??;
    assert_eq!(
        field(&get(&node, &id_of(256), &in_partition)?, "id"),
        text(&id_of(256))
    );
    assert_eq!(create(&node, &id_of(257), &[])?, Err(LONG_ID.to_owned()));
    assert_eq!(get(&node, &id_of(257), &in_partition)?, Term::Nil);
    let batch = run(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text("id-limit-batch")),
            ("type", text("id-limit")),
            ("items", Term::List(vec![text("short"), text(&id_of(257))])),
        ],
    )?;
    assert_eq!(refusal(batch)?, LONG_ID);
    assert_eq!(
        get(&node, "short", &[("partition_key", text("id-limit-batch"))])?,
        Term::Nil
    );
    assert_eq!(start(&node, &id_of(257), &[])?, Err(LONG_ID.to_owned()));
    Ok(())
}

#[test]
fn partitions_take_256_bytes_on_every_new_flow_path() -> TestResult {
    // Elixir: "a new Flow takes a 256-byte partition key and rejects a
    // longer one".
    let node = open(&directory("audit-limits-partitions")?, 4)?;
    let longest = id_of(256);
    create(&node, "partitioned", &[("partition_key", text(&longest))])??;
    let found = get(&node, "partitioned", &[("partition_key", text(&longest))])?;
    assert_eq!(field(&found, "id"), text("partitioned"));
    let too_long = [("partition_key", text(&id_of(257)))];
    assert_eq!(
        create(&node, "too-long", &too_long)?,
        Err(LONG_PARTITION.to_owned())
    );
    let batch = run(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text(&id_of(257))),
            ("type", text("id-limit")),
            ("items", Term::List(vec![text("batched")])),
        ],
    )?;
    assert_eq!(refusal(batch)?, LONG_PARTITION);
    assert_eq!(
        start(&node, "started", &too_long)?,
        Err(LONG_PARTITION.to_owned())
    );
    Ok(())
}

#[test]
fn types_states_correlations_and_workers_stay_bounded() -> TestResult {
    // Elixir: "new Flows take a type up to 128 bytes, a state up to 64 and
    // a correlation id up to 256" and "a claim or start takes a worker name
    // up to 256 bytes".
    let node = open(&directory("audit-limits-names")?, 4)?;
    create(
        &node,
        "widest",
        &[
            ("type", text(&bytes(128))),
            ("state", text(&bytes(64))),
            ("correlation_id", text(&bytes(256))),
        ],
    )??;
    // Lineage ids name Flows (or outside records) and take up to 4096 bytes,
    // as Elixir's reference check allows (audit gap G32).
    create(
        &node,
        "long-lineage",
        &[
            ("parent_flow_id", text(&bytes(300))),
            ("root_flow_id", text(&bytes(4_096))),
        ],
    )??;
    let lineage = create(
        &node,
        "too-long-lineage",
        &[("parent_flow_id", text(&bytes(4_097)))],
    )?;
    assert_eq!(
        lineage,
        Err("ERR flow parent_flow_id too large (max 4096 bytes)".to_owned())
    );
    for (extra, refused) in [
        (
            ("type", bytes(129)),
            "ERR flow type is longer than 128 bytes",
        ),
        (("state", bytes(65)), LONG_STATE),
        (
            ("correlation_id", bytes(257)),
            "ERR flow correlation_id is longer than 256 bytes",
        ),
    ] {
        let created = create(&node, "too-wide", &[(extra.0, text(&extra.1))])?;
        assert_eq!(created, Err(refused.to_owned()), "{}", extra.0);
    }
    let wide_type = start(&node, "wide-type", &[("type", text(&bytes(129)))])?;
    assert_eq!(
        wide_type,
        Err("ERR flow type is longer than 128 bytes".to_owned())
    );
    let wide_state = start(&node, "wide-state", &[("initial_state", text(&bytes(65)))])?;
    assert_eq!(wide_state, Err(LONG_STATE.to_owned()));

    let long_worker = "ERR flow worker is longer than 256 bytes".to_owned();
    let claim = |worker: &str| {
        claimed(
            &node,
            &[
                ("type", text("input-limits-claim")),
                ("partition_key", text("input-limits-claim")),
                ("worker", text(worker)),
            ],
        )
        .map_err(|error| error.to_string())
    };
    assert_eq!(claim(&bytes(257)), Err(long_worker.clone()));
    assert_eq!(
        start(&node, "worker", &[("worker", text(&bytes(257)))])?,
        Err(long_worker)
    );
    assert_eq!(claim(&bytes(256)), Ok(Vec::new()));
    Ok(())
}

#[test]
fn moves_into_a_state_take_64_bytes() -> TestResult {
    // Elixir: "a transition, step or reschedule moves only into a state up
    // to 64 bytes".
    let node = open(&directory("audit-limits-moves")?, 4)?;
    create(&node, "moving", &[("run_at_ms", int(1_000))])??;
    let in_partition = ("partition_key", text("input-limits"));
    let transition = |to: &str| {
        run(
            &node,
            "FLOW.TRANSITION",
            &[
                ("id", text("moving")),
                in_partition.clone(),
                ("from_state", text("queued")),
                ("to_state", text(to)),
                ("fencing_token", int(0)),
            ],
        )
    };
    assert_eq!(refusal(transition(&bytes(65))?)?, LONG_STATE);
    transition(&bytes(64))??;
    create(
        &node,
        "stepping",
        &[("type", text("stepper")), ("run_at_ms", int(1_000))],
    )??;
    let records = claimed(
        &node,
        &[
            ("type", text("stepper")),
            in_partition.clone(),
            ("worker", text("w")),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    let step = run(
        &node,
        "FLOW.STEP_CONTINUE",
        &[
            ("id", text("stepping")),
            in_partition,
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
            ("from_state", text("queued")),
            ("to_state", text(&bytes(65))),
        ],
    )?;
    assert_eq!(refusal(step)?, LONG_STATE);
    Ok(())
}

#[test]
fn a_child_group_takes_a_128_byte_id_and_64_byte_states() -> TestResult {
    // Elixir: "a child group takes a group id up to 128 bytes and states up
    // to 64", "spawned children are new Flows and take the same limit" and
    // "children inherit a parent's longer partition key but cannot open a
    // new one" (its inherited longer key predates the limit). Its spawns
    // name no wait_state, which the node checks first; each here names one.
    let node = open(&directory("audit-limits-children")?, 4)?;
    let spawn =
        |parent: &str, child: Term, extra: &[(&str, Term)]| -> TestResult<Result<Term, String>> {
            create(&node, parent, &[("state", text("dispatch"))])??;
            let mut entries = vec![
                ("id", text(parent)),
                ("partition_key", text("input-limits")),
                ("children", Term::List(vec![child])),
                ("group_id", text("g")),
                ("wait_state", text("waiting")),
                ("from_state", text("dispatch")),
                ("fencing_token", int(0)),
                (
                    "exhaust_to",
                    map(&[("success", text("completed")), ("failure", text("failed"))]),
                ),
            ];
            entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
            entries.extend(extra.iter().cloned());
            Ok(run(&node, "FLOW.SPAWN_CHILDREN", &entries)?.map_err(|error| error.to_string()))
        };
    let kid = |id: &str| map(&[("id", text(id)), ("type", text("t"))]);
    let widest = [
        ("group_id", text(&bytes(128))),
        ("wait_state", text(&bytes(64))),
    ];
    spawn("parent-ok", kid("kid-ok"), &widest)??;
    let refused = [
        (
            vec![("group_id", text(&bytes(129)))],
            "ERR flow group_id is longer than 128 bytes",
        ),
        (vec![("wait_state", text(&bytes(65)))], LONG_STATE),
        (
            vec![(
                "exhaust_to",
                map(&[("success", text(&bytes(65))), ("failure", text("failed"))]),
            )],
            LONG_STATE,
        ),
        (
            vec![(
                "exhaust_to",
                map(&[
                    ("success", text("completed")),
                    ("failure", text(&bytes(65))),
                ]),
            )],
            LONG_STATE,
        ),
    ];
    for (index, (extra, text_of)) in refused.iter().enumerate() {
        let parent = format!("parent-{index}");
        let answer = spawn(&parent, kid(&format!("kid-{index}")), extra)?;
        assert_eq!(answer, Err((*text_of).to_owned()), "{parent}");
    }
    let long_kid = spawn("parent-long-kid", kid(&id_of(257)), &[])?;
    assert_eq!(long_kid, Err(LONG_ID.to_owned()));
    let wide_partition = map(&[
        ("id", text("kid-wide")),
        ("type", text("t")),
        ("partition_key", text(&id_of(300))),
    ]);
    let answer = spawn("parent-wide", wide_partition, &[])?;
    assert_eq!(answer, Err(LONG_PARTITION.to_owned()));
    Ok(())
}
