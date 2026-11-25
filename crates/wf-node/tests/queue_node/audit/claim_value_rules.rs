//! Rest-of-suite audit rows on claim states and value puts
//! (`flow/pipeline_claim_due_command_test.exs`, `flow/value_store_test.exs`).

use wf_application::commands::flow::Term;

use super::{claim, field, has, ok, refusal, run, text};
use crate::{TestResult, directory, open};

#[test]
fn a_claim_takes_any_state_alone_or_named_states_but_not_both() -> TestResult {
    // Elixir: "rejects mixed ANY and explicit states" (audit gap G68).
    let node = open(&directory("audit-rest-claim-any")?, 4)?;
    let mixed = "ERR flow STATE ANY cannot be mixed with explicit states";
    for states in [["ANY", "queued"], ["queued", "any"]] {
        let listed = Term::List(states.iter().map(|state| text(state)).collect());
        let refused = claim(
            &node,
            "FLOW.CLAIM_DUE",
            &[
                ("type", text("t")),
                ("states", listed),
                ("worker", text("w")),
                ("partition_key", text("p")),
            ],
        )?;
        assert_eq!(refusal(refused)?, mixed, "{states:?}");
    }
    for alone in [text("ANY"), Term::List(vec![text("any")])] {
        let taken = claim(
            &node,
            "FLOW.CLAIM_DUE",
            &[
                ("type", text("t")),
                ("states", alone),
                ("worker", text("w")),
                ("partition_key", text("p")),
            ],
        )??;
        assert_eq!(taken, Term::List(Vec::new()));
    }
    Ok(())
}

#[test]
fn a_value_s_owner_and_name_must_not_be_empty() -> TestResult {
    // Elixir: "named value identity components must be non-empty" (audit
    // gap G69); a shared put names the owner it was given.
    let node = open(&directory("audit-rest-value-identity")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("owner")),
            ("type", text("t")),
            ("partition_key", text("p")),
        ],
    )?;
    let put = |extra: &[(&str, Term)]| {
        let mut entries = vec![("value", text("v")), ("partition_key", text("p"))];
        entries.extend(extra.iter().cloned());
        run(&node, "FLOW.VALUE.PUT", &entries)
    };
    for (owner, name, refused) in [
        ("", "n", "ERR flow owner_flow_id must be a non-empty string"),
        ("owner", "", "ERR flow name must be a non-empty string"),
    ] {
        let reply = put(&[("owner_flow_id", text(owner)), ("name", text(name))])?;
        assert_eq!(refusal(reply)?, refused);
    }
    assert_eq!(
        refusal(put(&[("owner_flow_id", text(""))])?)?,
        "ERR flow owner_flow_id must be a non-empty string"
    );
    let shared = put(&[("owner_flow_id", text("owner"))])??;
    assert_eq!(field(&shared, "owner_flow_id"), text("owner"));
    let anonymous = put(&[("name", text("n"))])??;
    assert!(!has(&anonymous, "owner_flow_id"));
    Ok(())
}
