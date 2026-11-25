//! HELLO and STARTUP negotiate compact replies alike and refuse a malformed
//! request as Elixir does (audit gap G63): a bad request, leaving the
//! earlier choice in place.

use super::typed::{Typed, at, bin, encode, map, reply, texts};
use super::{Client, TestResult, start};

const HELLO: u16 = 0x0001;
const OPTIONS: u16 = 0x000B;
const STARTUP: u16 = 0x000C;

fn codecs(names: &[&str]) -> Typed {
    Typed::List(names.iter().map(|name| bin(name)).collect())
}

/// The compact codecs a handshake reply says were chosen, and whether
/// compact Flow replies are on.
fn chosen(client: &mut Client, opcode: u16, body: &Typed) -> TestResult<(Vec<String>, Typed)> {
    let (status, value) = reply(&client.call(opcode, 0, 0, &encode(body)?)?.body)?;
    assert_eq!(status, 0, "{value:?}");
    let codecs = at(&value, &["capabilities", "response_codecs"])?;
    Ok((
        texts(at(codecs, &["selected_compact"])?)?,
        at(codecs, &["compact_flow_responses"])?.clone(),
    ))
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

#[test]
fn hello_negotiates_compact_replies_as_startup_does() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let offer = map(&[
        ("client_name", bin("ferricstore-go")),
        ("compact_flow_responses", Typed::Bool(true)),
        (
            "compact_response_codecs",
            codecs(&["flow_claim_jobs_v1", "flow_query_result_v1", "ok_list_v1"]),
        ),
    ]);
    let both = vec!["flow_claim_jobs_v1".to_owned(), "ok_list_v1".to_owned()];
    assert_eq!(
        chosen(&mut client, HELLO, &offer)?,
        (both.clone(), Typed::Bool(true))
    );
    assert_eq!(
        chosen(&mut client, OPTIONS, &map(&[]))?,
        (both, Typed::Bool(true))
    );
    // A HELLO that asks for nothing turns compact replies off again.
    assert_eq!(
        chosen(&mut client, HELLO, &map(&[]))?,
        (Vec::new(), Typed::Bool(false))
    );
    Ok(())
}

#[test]
fn compact_replies_take_elixir_s_spellings_of_true() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for yes in [bin("true"), bin("1"), Typed::Int(1), Typed::Bool(true)] {
        let offer = map(&[
            ("compact_flow_responses", yes.clone()),
            ("compact_response_codecs", codecs(&["ok_list_v1"])),
        ]);
        assert_eq!(
            chosen(&mut client, STARTUP, &offer)?,
            (vec!["ok_list_v1".to_owned()], Typed::Bool(true)),
            "{yes:?}"
        );
    }
    for no in [bin("yes"), Typed::Int(2), Typed::Bool(false)] {
        let offer = map(&[
            ("compact_flow_responses", no.clone()),
            ("compact_response_codecs", codecs(&["ok_list_v1"])),
        ]);
        assert_eq!(
            chosen(&mut client, STARTUP, &offer)?,
            (Vec::new(), Typed::Bool(false)),
            "{no:?}"
        );
    }
    Ok(())
}

#[test]
fn a_malformed_codec_request_is_a_bad_request_that_keeps_the_earlier_choice() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let first = map(&[
        ("compact_flow_responses", Typed::Bool(true)),
        ("compact_response_codecs", codecs(&["flow_claim_jobs_v1"])),
    ]);
    chosen(&mut client, STARTUP, &first)?;
    let long = "c".repeat(129);
    let many: Vec<String> = (0..33).map(|index| format!("c{index}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let names = "ERR native compact_response_codecs must contain binaries of 1..128 bytes";
    for (offered, refusal) in [
        (
            bin("ok_list_v1"),
            "ERR native compact_response_codecs must be a list",
        ),
        (
            codecs(&["ok_list_v1", "ok_list_v1"]),
            "ERR native compact_response_codecs must not contain duplicates",
        ),
        (
            codecs(&["ok_list_v1", "zeta_v1", "future_v1"]),
            "ERR native unsupported response codec: future_v1, zeta_v1",
        ),
        (Typed::List(vec![Typed::Int(1)]), names),
        (codecs(&[""]), names),
        (codecs(&[&long]), names),
        (Typed::List(vec![Typed::Bin(vec![0xFF])]), names),
        (
            codecs(&many),
            "ERR native compact_response_codecs accepts at most 32 entries",
        ),
    ] {
        for opcode in [HELLO, STARTUP] {
            let request = map(&[
                ("compact_flow_responses", Typed::Bool(true)),
                ("compact_response_codecs", offered.clone()),
            ]);
            let refused = client.call(opcode, 0, 0, &encode(&request)?)?;
            assert_eq!(refused.body, bad_request(refusal)?, "{offered:?}");
        }
    }
    // Thirty-two names are within the bound: these are refused only as
    // names the protocol does not define.
    let unknown: Vec<String> = (0..32).map(|index| format!("c{index:02}")).collect();
    let listed = unknown.join(", ");
    let unknown: Vec<&str> = unknown.iter().map(String::as_str).collect();
    let refused = client.call(
        STARTUP,
        0,
        0,
        &encode(&map(&[("compact_response_codecs", codecs(&unknown))]))?,
    )?;
    assert_eq!(
        refused.body,
        bad_request(&format!("ERR native unsupported response codec: {listed}"))?
    );
    // Every name the protocol defines is taken; the server picks those it
    // writes.
    let known = map(&[(
        "compact_response_codecs",
        codecs(&[
            "flow_claim_jobs_v1",
            "flow_query_result_v1",
            "flow_record_v1",
            "kv_get_v1",
            "kv_mget_v1",
            "ok_list_v1",
            "pipeline_v1",
            "pubsub_batch_v1",
        ]),
    )]);
    let reply = client.call(STARTUP, 0, 0, &encode(&known)?)?;
    assert_eq!(reply.body.get(..2), Some(&[0_u8, 0][..]));
    Ok(())
}

#[test]
fn a_refused_handshake_keeps_the_earlier_choice() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let first = map(&[
        ("compact_flow_responses", Typed::Bool(true)),
        ("compact_response_codecs", codecs(&["flow_claim_jobs_v1"])),
    ]);
    chosen(&mut client, STARTUP, &first)?;
    let refused = map(&[
        ("compact_flow_responses", Typed::Bool(true)),
        (
            "compact_response_codecs",
            codecs(&["ok_list_v1", "ok_list_v1"]),
        ),
    ]);
    client.call(HELLO, 0, 0, &encode(&refused)?)?;
    assert_eq!(
        chosen(&mut client, OPTIONS, &map(&[]))?,
        (vec!["flow_claim_jobs_v1".to_owned()], Typed::Bool(true))
    );
    Ok(())
}

#[test]
fn a_handshake_that_is_not_a_map_names_its_command() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for (opcode, command) in [(HELLO, "HELLO"), (STARTUP, "STARTUP")] {
        let refused = client.call(opcode, 0, 0, &encode(&bin("hello"))?)?;
        assert_eq!(
            refused.body,
            bad_request(&format!("ERR native {command} payload must be a map"))?
        );
        let refused = client.call(opcode, 0, 0, &[0xEE])?;
        let (status, value) = reply(&refused.body)?;
        assert_eq!(status, 6);
        let Typed::Bin(message) = at(&value, &["message"])? else {
            return Err("a message expected".into());
        };
        assert!(
            message.starts_with(format!("ERR native {command} payload is invalid: ").as_bytes()),
            "{message:?}"
        );
    }
    Ok(())
}

#[test]
fn compression_and_client_names_are_checked_as_elixir_checks_them() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let long = "n".repeat(1025);
    for (field, value, refusal) in [
        (
            "compression",
            bin("zlib"),
            "ERR native request compression is disabled",
        ),
        (
            "compression",
            bin("lz4"),
            "ERR native unsupported compression lz4",
        ),
        (
            "compression",
            Typed::Int(3),
            "ERR native unsupported compression 3",
        ),
        (
            "compression",
            Typed::Bool(true),
            "ERR native unsupported compression true",
        ),
        (
            "compression",
            Typed::List(vec![bin("none")]),
            "ERR native unsupported compression",
        ),
        (
            "client_name",
            Typed::Int(5),
            "ERR native client name must be a binary",
        ),
        (
            "client_name",
            bin(&long),
            "ERR native client name exceeds 1024 bytes",
        ),
        (
            "driver_name",
            Typed::Bin(vec![0xFF]),
            "ERR native driver name must be valid UTF-8",
        ),
    ] {
        for opcode in [HELLO, STARTUP] {
            let refused = client.call(opcode, 0, 0, &encode(&map(&[(field, value.clone())]))?)?;
            assert_eq!(refused.body, bad_request(refusal)?, "{field} {value:?}");
        }
    }
    for (field, value) in [
        ("compression", bin("none")),
        ("client_name", bin(&"n".repeat(1024))),
        ("driver_name", bin("ferricstore-python/1")),
    ] {
        let (status, _) = reply(
            &client
                .call(HELLO, 0, 0, &encode(&map(&[(field, value)]))?)?
                .body,
        )?;
        assert_eq!(status, 0, "{field}");
    }
    Ok(())
}
