//! Compact batch and value-read bodies, from the byte layouts of Elixir's
//! `native/codec_test.exs` (rest-of-suite audit).

use wf_application::commands::flow::Term;

use super::many_options;

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NONE: [u8; 4] = [0xFF; 4];

/// Elixir's `compact_bin`: a `u32` length, then the bytes.
fn bin(text: &str) -> Vec<u8> {
    let mut out = u32::try_from(text.len())
        .unwrap_or(u32::MAX)
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(text.as_bytes());
    out
}

fn body(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn text(value: &str) -> Term {
    Term::Bin(value.as_bytes().to_vec())
}

fn map(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn leased_item() -> Term {
    map(&[
        ("id", text("flow-1")),
        ("partition_key", text("p1")),
        ("lease_token", text("lease-1")),
        ("fencing_token", Term::Int(7)),
    ])
}

/// A batch with no partition, `now_ms` 123, the independence byte, one
/// item `flow-1` in `p1` leased by `lease-1` at fence 7.
fn leased(tag: u8, run_at: Option<i64>, independence: u8) -> Vec<u8> {
    let run_at = run_at.map(i64::to_be_bytes);
    body(&[
        &[tag],
        &NONE,
        &123_i64.to_be_bytes(),
        run_at.as_ref().map_or(&[][..], |bytes| bytes.as_slice()),
        &[independence],
        &1_u32.to_be_bytes(),
        &bin("flow-1"),
        &bin("p1"),
        &bin("lease-1"),
        &7_i64.to_be_bytes(),
    ])
}

#[test]
fn ok_on_success_batches_read_like_their_typed_form() -> TestResult {
    let ok = ("return", text("ok_on_success"));
    let independent = ("independent", Term::Bool(true));
    let now = ("now_ms", Term::Int(123));
    let items = ("items", Term::List(vec![leased_item()]));
    for (name, tag) in [("FLOW.COMPLETE_MANY", 0x93), ("FLOW.FAIL_MANY", 0x93)] {
        assert_eq!(
            many_options(name, &leased(tag, None, 2))?,
            map(&[now.clone(), independent.clone(), ok.clone(), items.clone()]),
            "{name}"
        );
    }
    assert_eq!(
        many_options("FLOW.COMPLETE_MANY", &leased(0x92, None, 2))?,
        map(&[now.clone(), independent.clone(), items.clone()])
    );
    assert_eq!(
        many_options("FLOW.RETRY_MANY", &leased(0x98, Some(456), 2))?,
        map(&[
            now.clone(),
            ("run_at_ms", Term::Int(456)),
            independent.clone(),
            ok.clone(),
            items,
        ])
    );
    let cancel = body(&[
        &[0x9A],
        &NONE,
        &123_i64.to_be_bytes(),
        &[2],
        &1_u32.to_be_bytes(),
        &bin("flow-1"),
        &bin("p1"),
        &7_i64.to_be_bytes(),
    ]);
    let unleased = map(&[
        ("id", text("flow-1")),
        ("partition_key", text("p1")),
        ("fencing_token", Term::Int(7)),
    ]);
    assert_eq!(
        many_options("FLOW.CANCEL_MANY", &cancel)?,
        map(&[
            now.clone(),
            independent.clone(),
            ok.clone(),
            ("items", Term::List(vec![unleased.clone()])),
        ])
    );
    // A transition names its states first; its items put the fence before
    // an optional lease token.
    let transition = body(&[
        &[0x9C],
        &bin("queued"),
        &bin("next"),
        &NONE,
        &123_i64.to_be_bytes(),
        &456_i64.to_be_bytes(),
        &[2],
        &1_u32.to_be_bytes(),
        &bin("flow-1"),
        &bin("p1"),
        &7_i64.to_be_bytes(),
        &NONE,
    ]);
    assert_eq!(
        many_options("FLOW.TRANSITION_MANY", &transition)?,
        map(&[
            ("from_state", text("queued")),
            ("to_state", text("next")),
            now,
            ("run_at_ms", Term::Int(456)),
            independent,
            ok,
            ("items", Term::List(vec![unleased])),
        ])
    );
    Ok(())
}

#[test]
fn the_independence_byte_and_local_only_markers_are_read() -> TestResult {
    // Elixir: "decodes compact FLOW.COMPLETE_MANY local-only request
    // marker": `3` and `4` are `2` and `1` asking for local routing, which
    // the node, routing every batch by partition, does not need.
    let independence = |byte: u8| -> Result<Term, Box<dyn std::error::Error>> {
        let Term::Map(entries) = many_options("FLOW.COMPLETE_MANY", &leased(0x92, None, byte))?
        else {
            return Err("a map expected".into());
        };
        Ok(entries
            .into_iter()
            .find(|(key, _)| key.as_slice() == b"independent")
            .map_or(Term::Nil, |(_, value)| value))
    };
    assert_eq!(independence(0)?, Term::Nil);
    assert_eq!(independence(1)?, Term::Bool(false));
    assert_eq!(independence(2)?, Term::Bool(true));
    assert_eq!(independence(3)?, Term::Bool(true));
    assert_eq!(independence(4)?, Term::Bool(false));
    assert!(many_options("FLOW.COMPLETE_MANY", &leased(0x92, None, 5)).is_err());
    Ok(())
}

#[test]
fn a_value_read_takes_its_references_and_byte_limit() -> TestResult {
    // Elixir: "decodes compact FLOW.VALUE.MGET request body".
    let read = |limit: i64| {
        body(&[
            &[0x9D],
            &limit.to_be_bytes(),
            &2_u32.to_be_bytes(),
            &bin("ref-1"),
            &bin("ref-2"),
        ])
    };
    let refs = ("refs", Term::List(vec![text("ref-1"), text("ref-2")]));
    assert_eq!(
        many_options("FLOW.VALUE.MGET", &read(64))?,
        map(&[refs.clone(), ("max_bytes", Term::Int(64))])
    );
    assert_eq!(
        many_options("FLOW.VALUE.MGET", &read(i64::MIN))?,
        map(&[refs])
    );
    Ok(())
}

#[test]
fn unknown_tags_and_leftover_bytes_are_refused() {
    let refused = |name: &str, body: &[u8]| {
        many_options(name, body).map_err(|error| error.message().to_owned())
    };
    let invalid = |name: &str| Err(format!("ERR native compact {name} payload is invalid"));
    // A tag belongs to its commands only.
    assert_eq!(
        refused("FLOW.CANCEL_MANY", &leased(0x92, None, 2)),
        invalid("FLOW.CANCEL_MANY")
    );
    assert_eq!(
        refused("FLOW.COMPLETE_MANY", &[0x9D]),
        invalid("FLOW.COMPLETE_MANY")
    );
    let mut long = leased(0x92, None, 2);
    long.push(0);
    assert_eq!(
        refused("FLOW.COMPLETE_MANY", &long),
        invalid("FLOW.COMPLETE_MANY")
    );
    let mut read = body(&[&[0x9D], &64_i64.to_be_bytes(), &0_u32.to_be_bytes()]);
    assert!(many_options("FLOW.VALUE.MGET", &read).is_ok());
    read.push(0);
    assert_eq!(
        refused("FLOW.VALUE.MGET", &read),
        invalid("FLOW.VALUE.MGET")
    );
    assert_eq!(
        refused(
            "FLOW.VALUE.MGET",
            &[0x9E, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        ),
        invalid("FLOW.VALUE.MGET")
    );
}
