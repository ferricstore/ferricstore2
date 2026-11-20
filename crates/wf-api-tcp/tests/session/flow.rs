//! Flow commands beyond the queue fast path (ADR-014): the typed-map body is
//! handed to the service as options and its term reply is encoded as a
//! typed value.

use super::typed::{Typed, bin, encode, map};
use super::{Client, TestResult, start};

const GET: u16 = 0x0202;

fn ok(value: &Typed) -> TestResult<Vec<u8>> {
    let mut body = 0_u16.to_be_bytes().to_vec();
    body.extend(encode(value)?);
    Ok(body)
}

#[test]
fn flow_get_replies_with_the_service_term() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[("id", bin("f1")), ("partition_key", bin("p1"))]))?;
    let reply = client.call(GET, 1, 0, &request)?;
    assert_eq!(
        reply.body,
        ok(&map(&[("id", bin("f1")), ("partition_key", bin("p1"))]))?
    );
    let missing = client.call(GET, 1, 0, &encode(&map(&[("id", bin("none1"))]))?)?;
    assert_eq!(missing.body, ok(&Typed::Nil)?);
    Ok(())
}

#[test]
fn flow_get_options_are_read_by_the_application() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[("id", bin("f1")), ("bogus", Typed::Int(1))]))?;
    let reply = client.call(GET, 1, 0, &request)?;
    assert_eq!(
        reply.body,
        wf_api_tcp::response::error(
            wf_api_tcp::response::STATUS_ERROR,
            "ERR FLOW.GET option bogus is not supported by this server"
        )?
    );
    Ok(())
}
