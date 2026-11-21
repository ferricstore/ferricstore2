//! Client clocks (ADR-015): a client's `now_ms` reaches the service only
//! when the server trusts client clocks; otherwise the node decides due
//! times and leases with its own clock, so no client can claim a future
//! Flow early or take a live lease by sending a later time.

use super::typed::{Typed, bin, encode, map};
use super::{Client, Running, TestResult, start, start_trusting_client_clocks};

const CLAIM_DUE: u16 = 0x0203;
const SIGNAL: u16 = 0x020D;
const COMMAND_EXEC: u16 = 0x0100;

fn ok(value: &Typed) -> TestResult<Vec<u8>> {
    let mut body = 0_u16.to_be_bytes().to_vec();
    body.extend(encode(value)?);
    Ok(body)
}

/// What the service saw of `now_ms` 5 through a typed command, a typed
/// claim and a text command.
fn seen(running: &Running) -> TestResult<[Vec<u8>; 3]> {
    let mut client = Client::connect(running)?;
    let signal = map(&[
        ("id", bin("clock")),
        ("signal", bin("s")),
        ("now_ms", Typed::Int(5)),
    ]);
    let typed = client.call(SIGNAL, 1, 0, &encode(&signal)?)?.body;
    let claim = map(&[
        ("type", bin("t")),
        ("worker", bin("clock")),
        ("now_ms", Typed::Int(5)),
        ("return", bin("jobs_compact")),
    ]);
    let claimed = client.call(CLAIM_DUE, 1, 0, &encode(&claim)?)?.body;
    let text = map(&[
        ("command", bin("FLOW.SIGNAL")),
        (
            "args",
            Typed::List(vec![
                bin("clock"),
                bin("SIGNAL"),
                bin("s"),
                bin("NOW"),
                bin("5"),
            ]),
        ),
    ]);
    let texted = client.call(COMMAND_EXEC, 1, 0, &encode(&text)?)?.body;
    Ok([typed, claimed, texted])
}

fn claimed(now: &str) -> Typed {
    Typed::List(vec![Typed::List(vec![
        bin("f1"),
        bin("p"),
        bin(now),
        Typed::Int(1),
    ])])
}

#[test]
fn a_client_clock_is_ignored_unless_the_server_trusts_it() -> TestResult {
    let [typed, claim, text] = seen(&start()?)?;
    assert_eq!(typed, ok(&Typed::Nil)?);
    assert_eq!(claim, ok(&claimed("now=None"))?);
    assert_eq!(text, ok(&Typed::Nil)?);
    let [typed, claim, text] = seen(&start_trusting_client_clocks()?)?;
    assert_eq!(typed, ok(&Typed::Int(5))?);
    assert_eq!(claim, ok(&claimed("now=Some(5)"))?);
    assert_eq!(text, ok(&Typed::Int(5))?);
    Ok(())
}
