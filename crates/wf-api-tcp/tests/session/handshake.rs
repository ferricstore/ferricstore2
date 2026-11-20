//! HELLO and STARTUP declare what the Python SDK checks before it connects.

use super::typed::{Typed, at, bin, encode, map, reply, texts};
use super::{Client, TestResult, start};

const HELLO: u16 = 0x0001;
const STARTUP: u16 = 0x000C;
const PING: u16 = 0x0003;

fn hello_body() -> TestResult<Vec<u8>> {
    encode(&map(&[
        ("compression", bin("none")),
        ("client_name", bin("ferricstore-python")),
    ]))
}

fn startup_body(codecs: &[&str]) -> TestResult<Vec<u8>> {
    encode(&map(&[
        ("compression", bin("none")),
        ("compact_flow_responses", Typed::Bool(true)),
        (
            "compact_response_codecs",
            Typed::List(codecs.iter().map(|codec| bin(codec)).collect()),
        ),
    ]))
}

#[test]
fn hello_declares_the_fields_the_sdk_requires() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let (status, value) = reply(&client.call(HELLO, 0, 0, &hello_body()?)?.body)?;
    assert_eq!(status, 0);
    assert_eq!(at(&value, &["protocol"])?, &bin("ferricstore-native"));
    assert_eq!(at(&value, &["version"])?, &Typed::Int(1));
    assert_eq!(at(&value, &["auth_required"])?, &Typed::Bool(false));
    assert!(
        matches!(at(&value, &["capabilities", "limits", "max_response_bytes"])?, Typed::Int(bytes) if *bytes > 0)
    );
    let opcodes = at(
        &value,
        &[
            "capabilities",
            "response_codecs",
            "compact_response_opcodes",
        ],
    )?;
    assert_eq!(
        at(opcodes, &["flow_claim_jobs_v1"])?,
        &Typed::List(vec![Typed::Int(0x0203)])
    );
    assert_eq!(
        at(opcodes, &["ok_list_v1"])?,
        &Typed::List(vec![Typed::Int(0x020F), Typed::Int(0x0210)])
    );
    let policy_fields = texts(at(
        &value,
        &["capabilities", "schemas", "FLOW.POLICY.SET", "fields"],
    )?)?;
    assert!(
        policy_fields.contains(&"expected_generation".into())
            && policy_fields.contains(&"replace".into())
    );
    let query = at(&value, &["capabilities", "schemas", "FLOW.QUERY"])?;
    assert_eq!(texts(at(query, &["required"])?)?, ["version", "query"]);
    let manifest = at(&value, &["capabilities", "flow_query"])?;
    assert_eq!(
        at(manifest, &["request_contract"])?,
        &bin("ferric.flow.query.request/v1")
    );
    assert!(texts(at(manifest, &["language_versions"])?)?.contains(&"FQL1".into()));
    assert_eq!(texts(at(manifest, &["shapes"])?)?.len(), 12);
    assert_eq!(at(&value, &["route", "shard_count"])?, &Typed::Int(4));
    assert_eq!(at(&value, &["route", "slots"])?, &Typed::Int(1024));
    Ok(())
}

#[test]
fn startup_selects_only_the_compact_codecs_this_server_writes() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let codecs = [
        "flow_claim_jobs_v1",
        "flow_query_result_v1",
        "ok_list_v1",
        "pipeline_v1",
    ];
    let (status, value) = reply(&client.call(STARTUP, 0, 0, &startup_body(&codecs)?)?.body)?;
    assert_eq!(status, 0);
    let codecs = at(&value, &["capabilities", "response_codecs"])?;
    assert_eq!(at(codecs, &["compact_flow_responses"])?, &Typed::Bool(true));
    assert_eq!(
        texts(at(codecs, &["selected_compact"])?)?,
        ["flow_claim_jobs_v1", "ok_list_v1"]
    );
    let (status, pong) = reply(&client.call(PING, 0, 0, &encode(&map(&[]))?)?.body)?;
    assert_eq!((status, pong), (0, bin("PONG")));
    Ok(())
}

/// Runs HELLO and STARTUP with the SDK's codec request.
pub(crate) fn negotiate(client: &mut Client) -> TestResult {
    client.call(HELLO, 0, 0, &hello_body()?)?;
    client.call(
        STARTUP,
        0,
        0,
        &startup_body(&["flow_claim_jobs_v1", "ok_list_v1"])?,
    )?;
    Ok(())
}
