//! A typed `PIPELINE` takes only the atomicity modes Elixir takes and
//! refuses `same_shard` around a Flow command (audit gap G64); each item
//! keeps its refusal's status and value, and `COMMAND_EXEC` keeps a query
//! failure's diagnostic (G65).

use super::typed::{Typed, at, bin, encode, map, reply};
use super::{Client, TestResult, start};

const PIPELINE: u16 = 0x000E;
const COMMAND_EXEC: u16 = 0x0100;
const QUERY: u16 = 0x0231;

fn command(opcode: i64, request: i64, body: Typed) -> Typed {
    map(&[
        ("opcode", Typed::Int(opcode)),
        ("lane_id", Typed::Int(1)),
        ("request_id", Typed::Int(request)),
        ("body", body),
    ])
}

fn get() -> Typed {
    command(
        0x0202,
        1,
        map(&[("id", bin("f1")), ("partition_key", bin("p"))]),
    )
}

fn text(request: i64, name: &str, args: &[&str]) -> Typed {
    command(
        i64::from(COMMAND_EXEC),
        request,
        map(&[
            ("command", bin(name)),
            (
                "args",
                Typed::List(args.iter().map(|arg| bin(arg)).collect()),
            ),
        ]),
    )
}

fn pipeline(atomicity: Option<Typed>, commands: Vec<Typed>) -> TestResult<Vec<u8>> {
    let mut entries = vec![("commands", Typed::List(commands))];
    if let Some(atomicity) = atomicity {
        entries.push(("atomicity", atomicity));
    }
    encode(&map(&entries))
}

fn bad_request(message: &str) -> TestResult<Vec<u8>> {
    let mut body = 6_u16.to_be_bytes().to_vec();
    body.extend(encode(&map(&[
        ("code", bin("bad_request")),
        ("message", bin(message)),
        ("retry_after_ms", Typed::Int(0)),
        ("retryable", Typed::Bool(false)),
        ("safe_to_retry", Typed::Bool(false)),
    ]))?);
    Ok(body)
}

/// Each item's status and value.
fn items(body: &[u8]) -> TestResult<Vec<(Typed, Typed)>> {
    let (status, value) = reply(body)?;
    assert_eq!(status, 0, "{value:?}");
    let Typed::List(items) = value else {
        return Err(format!("a list expected, got {value:?}").into());
    };
    items
        .iter()
        .map(|item| {
            Ok((
                at(item, &["status"])?.clone(),
                at(item, &["value"])?.clone(),
            ))
        })
        .collect()
}

#[test]
fn a_pipeline_takes_only_elixir_s_atomicity_modes() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for (atomicity, refusal) in [
        (
            map(&[("mode", bin("none"))]),
            "ERR native field atomicity must be binary",
        ),
        (
            bin("global"),
            "ERR native unsupported pipeline atomicity global",
        ),
        (
            bin("same_shard"),
            "ERR native same_shard pipeline contains coordinated command",
        ),
    ] {
        let refused = client.call(
            PIPELINE,
            1,
            0,
            &pipeline(Some(atomicity.clone()), vec![get()])?,
        )?;
        assert_eq!(refused.body, bad_request(refusal)?, "{atomicity:?}");
    }
    let coordinated = bad_request("ERR native same_shard pipeline contains coordinated command")?;
    for flow in [
        text(2, "flow.get", &["f1"]),
        command(0x0203, 3, map(&[])),
        command(0x0215, 3, map(&[])),
        command(0x020F, 3, map(&[])),
        command(0x0210, 3, map(&[])),
    ] {
        let refused = client.call(
            PIPELINE,
            1,
            0,
            &pipeline(Some(bin("same_shard")), vec![text(1, "PING", &[]), flow])?,
        )?;
        assert_eq!(refused.body, coordinated);
    }
    // A body without commands is malformed, not a bad request.
    let malformed = client.call(
        PIPELINE,
        1,
        0,
        &encode(&map(&[("atomicity", bin("none"))]))?,
    )?;
    let (status, value) = reply(&malformed.body)?;
    assert_eq!(
        (status, at(&value, &["message"])?),
        (1, &bin("ERR native PIPELINE payload is invalid"))
    );
    // Nothing to coordinate: an empty or PING-only same_shard pipeline runs.
    let empty = client.call(PIPELINE, 1, 0, &pipeline(Some(bin("same_shard")), vec![])?)?;
    assert_eq!(items(&empty.body)?, []);
    let pings = client.call(
        PIPELINE,
        1,
        0,
        &pipeline(Some(bin("same_shard")), vec![text(1, "PING", &[])])?,
    )?;
    assert_eq!(items(&pings.body)?, [(bin("ok"), bin("PONG"))]);
    for atomicity in [None, Some(bin("none")), Some(bin("per_shard"))] {
        let ran = client.call(PIPELINE, 1, 0, &pipeline(atomicity.clone(), vec![get()])?)?;
        let (status, _) = items(&ran.body)?
            .into_iter()
            .next()
            .ok_or("one item expected")?;
        assert_eq!(status, bin("ok"), "{atomicity:?}");
    }
    Ok(())
}

/// The typed `FLOW.QUERY` reply to a query that does not parse.
fn invalid_syntax(client: &mut Client) -> TestResult<Vec<u8>> {
    let typed = encode(&map(&[("version", bin("FQL1")), ("query", bin("BAD"))]))?;
    let direct = client.call(QUERY, 1, 0, &typed)?;
    let (status, diagnostic) = reply(&direct.body)?;
    assert_eq!(status, 6);
    assert_eq!(at(&diagnostic, &["code"])?, &bin("invalid_syntax"));
    Ok(direct.body)
}

#[test]
fn command_exec_keeps_a_query_failure_s_status_and_diagnostic() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let direct = invalid_syntax(&mut client)?;
    let request = encode(&map(&[
        ("command", bin("FLOW.QUERY")),
        ("args", Typed::List(vec![bin("FQL1"), bin("BAD")])),
    ]))?;
    let text = client.call(COMMAND_EXEC, 1, 0, &request)?;
    assert_eq!(text.body, direct);
    Ok(())
}

#[test]
fn pipeline_items_keep_their_refusal_s_status_and_value() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let create = |request: i64, id: &str| {
        command(
            0x0201,
            request,
            map(&[("id", bin(id)), ("type", bin("t")), ("state", bin("a"))]),
        )
    };
    let query = command(
        i64::from(QUERY),
        1,
        map(&[("version", bin("FQL1")), ("query", bin("BAD"))]),
    );
    let malformed = command(
        i64::from(QUERY),
        2,
        map(&[
            ("version", bin("FQL1")),
            ("query", bin("q")),
            ("bogus", Typed::Int(1)),
        ]),
    );
    let denied = map(&[
        ("id", bin("denied")),
        ("effect_key", bin("k")),
        ("effect_type", bin("t")),
    ]);
    let (status, denial) = reply(&client.call(0x0240, 1, 0, &encode(&denied)?)?.body)?;
    assert_eq!(status, 1);
    let late = command(
        0x0202,
        8,
        map(&[("id", bin("f1")), ("deadline_ms", bin("soon"))]),
    );
    let answer = client.call(
        PIPELINE,
        1,
        0,
        &pipeline(
            None,
            vec![
                query,
                text(3, "FLOW.QUERY", &["FQL1", "BAD"]),
                malformed,
                create(4, "busy-1"),
                create(5, "reroute-1"),
                create(6, "dup-1"),
                create(7, "fine-1"),
                late,
                command(0x0240, 9, denied),
            ],
        )?,
    )?;
    let (_, diagnostic) = reply(&invalid_syntax(&mut client)?)?;
    assert_eq!(
        items(&answer.body)?,
        [
            (bin("bad_request"), diagnostic.clone()),
            (bin("bad_request"), diagnostic),
            (
                bin("bad_request"),
                bin("ERR native FLOW.QUERY contains unsupported fields")
            ),
            (
                bin("busy"),
                bin("BUSY flow shards are overloaded, retry later")
            ),
            (
                bin("reroute"),
                bin("REROUTE this member does not lead the slot; refresh SHARDS")
            ),
            (bin("error"), bin("ERR flow already exists")),
            (bin("ok"), bin("OK")),
            (
                bin("bad_request"),
                bin("ERR native request deadline must be a non-negative integer")
            ),
            (bin("error"), denial),
        ]
    );
    Ok(())
}
