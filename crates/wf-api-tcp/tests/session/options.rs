//! Claim options reach the queue service (ADR-013); values outside Elixir's
//! bounds are refused with Elixir's text.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::request::ClaimShape;
use wf_api_tcp::response::{self, ClaimedJob, STATUS_ERROR};

use super::typed::Typed;
use super::{Client, TestResult, start};

const CLAIM_DUE: u16 = 0x0203;

fn bin(out: &mut Vec<u8>, bytes: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// A compact `FLOW.CLAIM_DUE` for one `jobs` Flow by worker `probe`.
fn claim_due(reclaim: u8, ratio: i64, block_ms: i64) -> TestResult<Vec<u8>> {
    let mut out = vec![0x91];
    bin(&mut out, b"jobs")?;
    out.extend_from_slice(&u32::MAX.to_be_bytes());
    bin(&mut out, b"probe")?;
    for value in [30_000_i64, 1, block_ms] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    out.push(reclaim);
    out.extend_from_slice(&ratio.to_be_bytes());
    out.extend_from_slice(&i64::MIN.to_be_bytes());
    out.extend_from_slice(&[1, 0]);
    Ok(out)
}

fn echoed(run_state: &[u8]) -> TestResult<Vec<u8>> {
    let job = ClaimedJob {
        id: b"f1",
        partition: b"__flow_auto__:7",
        lease_token: b"probe:1:1",
        fencing_token: 1,
        run_state,
    };
    Ok(response::typed_claim_jobs(&[job], ClaimShape::Jobs)?)
}

fn refusal(client: &mut Client, body: &[u8]) -> TestResult<String> {
    let reply = client.call(CLAIM_DUE, 1, FLAG_CUSTOM_PAYLOAD, body)?;
    let (status, value) = super::typed::reply(&reply.body)?;
    assert_eq!(status, STATUS_ERROR);
    match super::typed::at(&value, &["message"])? {
        Typed::Bin(text) => Ok(String::from_utf8_lossy(text).into_owned()),
        other => Err(format!("message {other:?}").into()),
    }
}

#[test]
fn reclaim_options_reach_the_service() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for (reclaim, ratio, run_state) in [
        (1, 40, "reclaim=Some(40)"),
        (1, 0, "reclaim=Some(0)"),
        (0, 25, "reclaim=None"),
    ] {
        let reply = client.call(
            CLAIM_DUE,
            1,
            FLAG_CUSTOM_PAYLOAD,
            &claim_due(reclaim, ratio, -1)?,
        )?;
        assert_eq!(reply.body, echoed(run_state.as_bytes())?, "{run_state}");
    }
    Ok(())
}

#[test]
fn claim_options_outside_elixir_bounds_are_refused() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    for ratio in [101, -1] {
        assert_eq!(
            refusal(&mut client, &claim_due(1, ratio, -1)?)?,
            "ERR flow reclaim_ratio must be an integer between 0 and 100"
        );
    }
    Ok(())
}
