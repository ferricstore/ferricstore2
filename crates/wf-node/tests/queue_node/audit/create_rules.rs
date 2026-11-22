//! `flow/mutation_attrs_test.exs`, as the wire shows it: a create's
//! defaults and `infinity` active time, schedule metadata kept from
//! clients, `running` refused on every create path, and times
//! and lease deadlines held to the exact integer range. (Its internal
//! schedule path is the node's schedule thread.)

use wf_application::commands::flow::Term;
use wf_application::commands::queue::Partition;
use wf_node::composition::QueueNode;

use super::{field, get, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

const MAX_EXACT: i64 = 9_007_199_254_740_991;
const RUNNING: &str = "ERR flow running state is only entered by FLOW.CLAIM_DUE";

fn create(node: &QueueNode, id: &str, extra: &[(&str, Term)]) -> TestResult<Result<Term, String>> {
    let mut entries = vec![("id", text(id)), ("type", text("email"))];
    entries.extend(extra.iter().cloned());
    Ok(run(node, "FLOW.CREATE", &entries)?.map_err(|error| error.to_string()))
}

#[test]
fn a_create_defaults_its_state_and_partition_and_takes_infinity() -> TestResult {
    // Elixir: "create_attrs preserves default state, auto partition, and
    // named value options" (its value options are in `audit::value_refs`)
    // and "create_attrs accepts infinity while validating compact query
    // metadata".
    let node = open(&directory("audit-create-defaults")?, 4)?;
    create(&node, "flow-1", &[])??;
    let record = get(&node, "flow-1", &[])?;
    assert_eq!(field(&record, "type"), text("email"));
    assert_eq!(field(&record, "state"), text("queued"));
    let auto = Partition::for_new_flow(b"flow-1", None).name();
    assert_eq!(field(&record, "partition_key"), Term::Bin(auto));
    // `infinity` is accepted and stored as no limit, as Elixir's
    // `resolve_max_active_ms` stores nil for it.
    create(&node, "unbounded", &[("max_active_ms", text("infinity"))])??;
    assert_eq!(
        field(&get(&node, "unbounded", &[])?, "max_active_ms"),
        Term::Nil
    );
    Ok(())
}

#[test]
fn schedule_metadata_is_not_a_client_option() -> TestResult {
    // Elixir: "schedule discovery metadata is accepted only through the
    // internal schedule path": on its wire `schedule_metadata` is not a
    // known option, so a client's is refused before the create reads it.
    let node = open(&directory("audit-create-schedule-meta")?, 4)?;
    let metadata = map(&[("kind", text("interval")), ("target_type", text("email"))]);
    let refused = create(&node, "flow-1", &[("schedule_metadata", metadata)])?;
    assert_eq!(
        refused,
        Err("ERR native unknown flow option schedule_metadata".to_owned())
    );
    assert_eq!(get(&node, "flow-1", &[])?, Term::Nil);
    Ok(())
}

#[test]
fn running_is_refused_on_every_create_path() -> TestResult {
    // Elixir: "create attrs rejects unleased running state before
    // replicated mutation preparation", "batch and spawned child creation
    // share the unleased running rejection" and "transition_attrs rejects
    // direct transition into running" (the last in
    // `audit::reclaim_batches`).
    let node = open(&directory("audit-create-running")?, 4)?;
    let running = [("state", text("running"))];
    assert_eq!(
        create(&node, "flow-running", &running)?,
        Err(RUNNING.to_owned())
    );
    let leased = [
        ("state", text("running")),
        ("worker", text("worker")),
        ("lease_ms", int(30_000)),
    ];
    assert_eq!(
        create(&node, "flow-running", &leased)?,
        Err(RUNNING.to_owned())
    );
    let children = Term::List(vec![
        map(&[
            ("id", text("queued-child")),
            ("type", text("email")),
            ("state", text("queued")),
        ]),
        map(&[
            ("id", text("running-child")),
            ("type", text("email")),
            ("state", text("running")),
        ]),
    ]);
    let batch = run(
        &node,
        "FLOW.CREATE_MANY",
        &[
            ("partition_key", text("partition")),
            ("items", children.clone()),
        ],
    )?;
    assert_eq!(refusal(batch)?, RUNNING);
    assert_eq!(
        get(
            &node,
            "queued-child",
            &[("partition_key", text("partition"))]
        )?,
        Term::Nil
    );
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("parent")),
            ("type", text("parent")),
            ("state", text("dispatch")),
            ("partition_key", text("partition")),
        ],
    )?;
    let spawned = run(
        &node,
        "FLOW.SPAWN_CHILDREN",
        &[
            ("id", text("parent")),
            ("partition_key", text("partition")),
            ("children", children),
            ("group_id", text("children")),
            ("wait_state", text("waiting")),
            ("from_state", text("dispatch")),
            (
                "exhaust_to",
                map(&[("success", text("completed")), ("failure", text("failed"))]),
            ),
            ("fencing_token", int(0)),
        ],
    )?;
    assert_eq!(refusal(spawned)?, RUNNING);
    Ok(())
}

#[test]
fn times_and_lease_deadlines_stay_within_the_exact_integer_range() -> TestResult {
    // Elixir: "create_attrs rejects timestamps that cannot be represented
    // exactly by Flow indexes" and "lease mutations reject deadlines
    // outside the exact Flow timestamp range".
    let node = open(&directory("audit-create-exact")?, 4)?;
    let over = MAX_EXACT.saturating_add(1);
    let now = create(&node, "flow-1", &[("now_ms", int(over))])?;
    assert_eq!(
        now,
        Err(format!("ERR flow now_ms exceeds maximum {MAX_EXACT}"))
    );
    let due = create(
        &node,
        "flow-1",
        &[("now_ms", int(1)), ("run_at_ms", int(over))],
    )?;
    assert_eq!(
        due,
        Err(format!("ERR flow run_at_ms exceeds maximum {MAX_EXACT}"))
    );
    let start = |now: i64, lease: i64| {
        run(
            &node,
            "FLOW.START_AND_CLAIM",
            &[
                ("id", text("flow-1")),
                ("type", text("email")),
                ("initial_state", text("queued")),
                ("worker", text("worker")),
                ("now_ms", int(now)),
                ("lease_ms", int(lease)),
            ],
        )
    };
    assert_eq!(
        refusal(start(0, over)?)?,
        format!("ERR flow lease_ms exceeds maximum {MAX_EXACT}")
    );
    assert_eq!(
        refusal(start(MAX_EXACT, 1)?)?,
        format!("ERR flow lease_ms deadline exceeds maximum {MAX_EXACT}")
    );
    Ok(())
}

#[test]
fn a_create_ignores_known_options_it_does_not_read_but_not_return() -> TestResult {
    // Elixir's native option rule (audit gap G35): a known Flow option a
    // command does not read is ignored, `return` is refused where the
    // command takes none, and an unknown option is refused.
    let node = open(&directory("audit-create-option-rule")?, 4)?;
    create(
        &node,
        "ignored",
        &[("worker", text("worker")), ("lease_ms", int(30_000))],
    )??;
    assert_eq!(field(&get(&node, "ignored", &[])?, "state"), text("queued"));
    let returned = create(&node, "returned", &[("return", text("items"))])?;
    assert_eq!(
        returned,
        Err("ERR flow return option is not supported".to_owned())
    );
    let bogus = create(&node, "bogus", &[("bogus", int(1))])?;
    assert_eq!(
        bogus,
        Err("ERR native unknown flow option bogus".to_owned())
    );
    Ok(())
}

#[test]
fn integer_options_take_the_exact_ceiling_and_refuse_beyond_it() -> TestResult {
    // Elixir: "integer options reject values above Flow's exact integer
    // ceiling", as a transition's fence and a history read's `to_ms`.
    let node = open(&directory("audit-create-integer-ceiling")?, 4)?;
    create(&node, "flow-1", &[("state", text("queued"))])??;
    let moved = run(
        &node,
        "FLOW.TRANSITION",
        &[
            ("id", text("flow-1")),
            ("from_state", text("queued")),
            ("to_state", text("ready")),
            ("fencing_token", int(MAX_EXACT.saturating_add(1))),
        ],
    )?;
    assert_eq!(
        refusal(moved)?,
        format!("ERR flow fencing_token exceeds maximum {MAX_EXACT}")
    );
    let read = |to: i64| {
        run(
            &node,
            "FLOW.HISTORY",
            &[("id", text("flow-1")), ("to_ms", int(to))],
        )
    };
    read(MAX_EXACT)??;
    assert_eq!(
        refusal(read(MAX_EXACT.saturating_add(1))?)?,
        format!("ERR flow to_ms exceeds maximum {MAX_EXACT}")
    );
    Ok(())
}
