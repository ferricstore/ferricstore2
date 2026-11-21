//! The SDK's compact `FLOW.CREATE_MANY` bodies, as Elixir's codec reads
//! them: tag `0x90` (items `[id, payload]`, no partition), `0x96` (a batch
//! partition, possibly none) and `0x9E` (items `[id, partition, payload]`).
//! Independent batches take the queue fast path; the others are the generic
//! command.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::response::{self, ItemOutcome, STATUS_ERROR};

use super::handshake::negotiate;
use super::typed::{Typed, bin, reply};
use super::{Client, TestResult, start};

const CREATE_MANY: u16 = 0x020F;

/// Independence markers as the SDK writes them.
const DEFAULT: u8 = 0;
const INDEPENDENT: u8 = 2;

fn binary(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend(u32::try_from(bytes.len())?.to_be_bytes());
    out.extend(bytes);
    Ok(())
}

/// The batch partition field of a compact body.
#[derive(Clone, Copy)]
enum Batch<'a> {
    /// Tags `0x90` and `0x9E` have no field.
    Absent,
    /// Tag `0x96` without a partition.
    Empty,
    /// Tag `0x96` with one.
    Key(&'a str),
}

fn body(
    tag: u8,
    partition: Batch<'_>,
    independent: u8,
    ok_on_success: bool,
    items: &[&[&str]],
) -> TestResult<Vec<u8>> {
    let mut out = vec![tag];
    binary(&mut out, b"jobs")?;
    binary(&mut out, b"queued")?;
    match partition {
        Batch::Key(key) => binary(&mut out, key.as_bytes())?,
        Batch::Empty => out.extend(u32::MAX.to_be_bytes()),
        Batch::Absent => {}
    }
    out.extend(1_000_i64.to_be_bytes());
    out.extend(1_000_i64.to_be_bytes());
    out.push(independent);
    out.push(u8::from(ok_on_success));
    out.extend(u32::try_from(items.len())?.to_be_bytes());
    for item in items {
        for field in *item {
            binary(&mut out, field.as_bytes())?;
        }
    }
    Ok(out)
}

fn created(fields: &[Typed]) -> Typed {
    Typed::List(fields.to_vec())
}

#[test]
fn an_independent_mixed_batch_takes_the_queue_path() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    negotiate(&mut client)?;
    let items: &[&[&str]] = &[&["dup1", "p1", "x"], &["a", "p2", "y"]];
    let partial = client.call(
        CREATE_MANY,
        1,
        FLAG_CUSTOM_PAYLOAD,
        &body(0x9E, Batch::Absent, INDEPENDENT, true, items)?,
    )?;
    let expected = [
        ItemOutcome::Error("ERR flow already exists"),
        ItemOutcome::Ok,
    ];
    assert_eq!(partial.body, response::item_outcomes(&expected)?);
    let items: &[&[&str]] = &[&["a", "p1", "x"]];
    let ok = client.call(
        CREATE_MANY,
        1,
        FLAG_CUSTOM_PAYLOAD,
        &body(0x9E, Batch::Absent, INDEPENDENT, true, items)?,
    )?;
    assert_eq!(ok.body, response::ok_text(b"OK")?);
    Ok(())
}

#[test]
fn a_partition_batch_is_the_generic_command_with_its_items() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let items: &[&[&str]] = &[&["a", "x"], &["b", "y"]];
    let reply_frame = client.call(
        CREATE_MANY,
        1,
        FLAG_CUSTOM_PAYLOAD,
        &body(0x96, Batch::Key("p"), DEFAULT, false, items)?,
    )?;
    let expected = Typed::List(vec![
        created(&[bin("a"), bin("p"), bin("x"), Typed::Bool(true)]),
        created(&[bin("b"), bin("p"), bin("y"), Typed::Bool(true)]),
    ]);
    assert_eq!(reply(&reply_frame.body)?, (0, expected));
    Ok(())
}

#[test]
fn batches_without_a_partition_create_in_auto_buckets() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let items: &[&[&str]] = &[&["a", "x"]];
    for frame in [
        body(0x90, Batch::Absent, DEFAULT, false, items)?,
        body(0x96, Batch::Empty, DEFAULT, false, items)?,
    ] {
        let reply_frame = client.call(CREATE_MANY, 1, FLAG_CUSTOM_PAYLOAD, &frame)?;
        let expected = Typed::List(vec![created(&[
            bin("a"),
            Typed::Nil,
            bin("x"),
            Typed::Bool(false),
        ])]);
        assert_eq!(reply(&reply_frame.body)?, (0, expected));
    }
    Ok(())
}

#[test]
fn a_truncated_or_unknown_compact_batch_is_refused() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let items: &[&[&str]] = &[&["a", "p", "x"]];
    let mut truncated = body(0x9E, Batch::Absent, DEFAULT, false, items)?;
    truncated.pop();
    let mut unknown = body(0x9E, Batch::Absent, DEFAULT, false, items)?;
    if let Some(tag) = unknown.first_mut() {
        *tag = 0x91;
    }
    for frame in [truncated, unknown] {
        let refused = client.call(CREATE_MANY, 1, FLAG_CUSTOM_PAYLOAD, &frame)?;
        assert_eq!(
            refused.body,
            response::error(
                STATUS_ERROR,
                "ERR native compact FLOW.CREATE_MANY payload is invalid"
            )?
        );
    }
    Ok(())
}
