//! A run's record holds its payload once and reads back as its
//! after-images, leases included, in the log and in a proposal; a record
//! whose image lacks the mark does not read.

use wf_application::commands::queue::Partition;

use super::super::super::sink;
use super::super::{Logged, RUN, bin, optional_bin};
use super::records;

const PAYLOAD: usize = 1_000;

/// An after-image's bytes up to and including its payload field, then a
/// stand-in for the fields after it, laid out as `encode_record` writes.
fn image(id: &[u8], leased: bool, payload: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for field in [b"tenant".as_slice(), id] {
        bin(&mut out, field)?;
    }
    out.push(0);
    for field in [b"jobs".as_slice(), b"ready"] {
        bin(&mut out, field)?;
    }
    for number in 1_u64..=5 {
        out.extend_from_slice(&number.to_be_bytes());
    }
    out.push(0);
    optional_bin(&mut out, Some(b"step"))?;
    if leased {
        out.push(1);
        bin(&mut out, b"worker-1")?;
        bin(&mut out, b"token-1")?;
        out.extend_from_slice(&9_u64.to_be_bytes());
    } else {
        out.push(0);
    }
    optional_bin(&mut out, payload)?;
    out.extend_from_slice(b"rest of the record");
    Some(out)
}

fn put(id: &[u8], leased: bool, payload: &[u8]) -> Option<Logged> {
    Some(Logged::Put {
        partition: Partition::parse(b"tenant"),
        image: image(id, leased, Some(payload))?,
    })
}

fn run() -> Option<Vec<Logged>> {
    let payload = vec![b'x'; PAYLOAD];
    Some(vec![
        put(b"a", true, &payload)?,
        put(b"b", false, &payload)?,
        put(b"c", true, &payload)?,
    ])
}

#[test]
fn a_run_is_one_record_holding_its_payload_once() -> Result<(), &'static str> {
    let changes = run().ok_or("no run")?;
    let logged = records(&changes, usize::MAX).ok_or("not encoded")?;
    let [record] = logged.as_slice() else {
        return Err("not one record");
    };
    assert!(record.len() < 2 * PAYLOAD, "{} bytes", record.len());
    assert_eq!(Logged::decode(record), Some(Logged::Group(changes)));
    Ok(())
}

#[test]
fn a_proposal_holds_a_run_s_payload_once() -> Result<(), &'static str> {
    let changes = run().ok_or("no run")?;
    let body = sink::proposal(&changes).ok_or("not proposed")?;
    assert!(body.len() < 2 * PAYLOAD, "{} bytes", body.len());
    let decoded: Vec<Option<Logged>> = sink::records(&body)
        .ok_or("no records")?
        .iter()
        .map(|record| Logged::decode(record))
        .collect();
    assert_eq!(decoded, vec![Some(Logged::Group(changes))]);
    Ok(())
}

#[test]
fn a_run_whose_image_lacks_the_mark_does_not_read() -> Result<(), &'static str> {
    let payload = vec![b'x'; PAYLOAD];
    let mut record = vec![RUN];
    bin(&mut record, &payload).ok_or("payload")?;
    record.extend_from_slice(&1_u32.to_be_bytes());
    bin(&mut record, b"tenant").ok_or("partition")?;
    let whole = image(b"a", true, Some(&payload)).ok_or("image")?;
    bin(&mut record, &whole).ok_or("image")?;
    assert_eq!(Logged::decode(&record), None);
    Ok(())
}
