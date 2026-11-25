//! `FERRICSTORE.CAPABILITIES` (audit gap G73): the management map the SDKs'
//! `capabilities()` reads, naming what this workflow node serves.

use super::typed::{Typed, bin, encode, map, reply};
use super::{Client, TestResult, start};

const COMMAND_EXEC: u16 = 0x0100;

#[test]
fn capabilities_name_what_this_node_serves() -> TestResult {
    // Elixir: `management_contract_test.exs` "default management
    // capabilities expose OSS-safe feature flags" and
    // "FERRICSTORE.CAPABILITIES returns string-keyed SDK contract".
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for name in ["FERRICSTORE.CAPABILITIES", "ferricstore.capabilities"] {
        let request = encode(&map(&[
            ("command", bin(name)),
            ("args", Typed::List(Vec::new())),
        ]))?;
        let answer = client.call(COMMAND_EXEC, 1, 0, &request)?;
        assert_eq!(
            reply(&answer.body)?,
            (
                0,
                map(&[
                    ("acl_management", Typed::Bool(false)),
                    ("flow_observability", Typed::Bool(true)),
                    ("health", Typed::Bool(false)),
                    ("namespace_management", Typed::Bool(false)),
                    ("quota_management", Typed::Bool(false)),
                    ("sdk", Typed::Bool(true)),
                    ("telemetry", Typed::Bool(false)),
                ])
            ),
            "{name}"
        );
    }
    let extra = encode(&map(&[
        ("command", bin("FERRICSTORE.CAPABILITIES")),
        ("args", Typed::List(vec![bin("x")])),
    ]))?;
    let (status, refusal) = reply(&client.call(COMMAND_EXEC, 1, 0, &extra)?.body)?;
    assert_eq!(status, 1);
    assert_eq!(
        super::typed::at(&refusal, &["message"])?,
        &bin("ERR wrong number of arguments for 'ferricstore.capabilities' command")
    );
    Ok(())
}
