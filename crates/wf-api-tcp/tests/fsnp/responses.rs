//! Reply bodies must equal what the Elixir server sent, byte for byte.

use wf_api_tcp::request::ClaimShape;
use wf_api_tcp::response::{self, ClaimedJob, ItemOutcome, STATUS_ERROR};

use super::TestResult;

const CLAIM_JOBS: &[u8] = include_bytes!("fixtures/elixir_claim_jobs_response.bin");

/// Reads the compact claim-jobs reply Elixir sent (status, tag 0x80, count,
/// then id, partition, lease, fencing and an empty attributes map per job).
struct Rows<'a>(&'a [u8]);

impl<'a> Rows<'a> {
    fn take(&mut self, len: usize) -> TestResult<&'a [u8]> {
        let (head, tail) = self.0.split_at_checked(len).ok_or("truncated fixture")?;
        self.0 = tail;
        Ok(head)
    }
    fn u32(&mut self) -> TestResult<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn bin(&mut self) -> TestResult<&'a [u8]> {
        let len = usize::try_from(self.u32()?)?;
        self.take(len)
    }
}

#[test]
fn compact_claim_jobs_match_elixir() -> TestResult {
    let mut rows = Rows(CLAIM_JOBS);
    assert_eq!(rows.take(3)?, [0, 0, 0x80]);
    let count = rows.u32()?;
    let mut jobs = Vec::new();
    for _ in 0..count {
        let id = rows.bin()?;
        let partition = rows.bin()?;
        let lease_token = rows.bin()?;
        let fencing_token = i64::from_be_bytes(rows.take(8)?.try_into()?);
        assert_eq!(rows.take(5)?, [6, 0, 0, 0, 0], "empty attributes map");
        jobs.push(ClaimedJob {
            id,
            partition,
            lease_token,
            fencing_token,
            run_state: b"running",
        });
    }
    assert_eq!(rows.0, []);
    assert_eq!(jobs.len(), usize::try_from(count)?);
    assert_ne!(count, 0);
    assert_eq!(
        response::compact_claim_jobs(&jobs, ClaimShape::JobsAttributes)?,
        CLAIM_JOBS
    );
    Ok(())
}

#[test]
fn claim_shapes_add_run_state_and_attributes() -> TestResult {
    let job = ClaimedJob {
        id: b"f",
        partition: b"__flow_auto__:1",
        lease_token: b"w:1:1",
        fencing_token: 1,
        run_state: b"running",
    };
    let mut base = vec![0, 0, 0x80, 0, 0, 0, 1];
    for part in [b"f".as_slice(), b"__flow_auto__:1", b"w:1:1"] {
        base.extend_from_slice(&u32::try_from(part.len())?.to_be_bytes());
        base.extend_from_slice(part);
    }
    base.extend_from_slice(&1_i64.to_be_bytes());
    let state = [&[0, 0, 0, 7][..], b"running"].concat();
    let attributes = [6, 0, 0, 0, 0];
    for (shape, suffix) in [
        (ClaimShape::Jobs, Vec::new()),
        (ClaimShape::JobsState, state.clone()),
        (ClaimShape::JobsAttributes, attributes.to_vec()),
        (
            ClaimShape::JobsStateAttributes,
            [state.as_slice(), &attributes].concat(),
        ),
    ] {
        let expected = [base.as_slice(), &suffix].concat();
        assert_eq!(
            response::compact_claim_jobs(&[job], shape)?,
            expected,
            "{shape:?}"
        );
    }
    Ok(())
}

#[test]
fn typed_claim_jobs_are_rows_of_typed_values() -> TestResult {
    let job = ClaimedJob {
        id: b"f",
        partition: b"p",
        lease_token: b"l",
        fencing_token: 2,
        run_state: b"running",
    };
    let body = response::typed_claim_jobs(&[job], ClaimShape::Jobs)?;
    let mut expected = vec![0, 0, 5, 0, 0, 0, 1, 5, 0, 0, 0, 4];
    for part in [b"f", b"p", b"l"] {
        expected.extend_from_slice(&[4, 0, 0, 0, 1]);
        expected.extend_from_slice(part);
    }
    expected.push(3);
    expected.extend_from_slice(&2_i64.to_be_bytes());
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn single_item_replies_match_elixir() -> TestResult {
    assert_eq!(
        response::ok_text(b"OK")?,
        include_bytes!("fixtures/elixir_complete_ok_response.bin")
    );
    for (message, fixture) in [
        (
            "ERR stale flow lease",
            include_bytes!("fixtures/elixir_complete_stale_response.bin").as_slice(),
        ),
        (
            "ERR flow not found",
            include_bytes!("fixtures/elixir_complete_not_found_response.bin").as_slice(),
        ),
        (
            "ERR flow already exists",
            include_bytes!("fixtures/elixir_create_exists_response.bin").as_slice(),
        ),
    ] {
        assert_eq!(
            response::error(STATUS_ERROR, message)?,
            fixture,
            "{message}"
        );
    }
    Ok(())
}

#[test]
fn batch_replies_match_elixir() -> TestResult {
    assert_eq!(
        response::compact_ok_list(3)?,
        include_bytes!("fixtures/elixir_create_many_ok_list_response.bin")
    );
    assert_eq!(
        response::item_outcomes(&[
            ItemOutcome::Error("ERR flow already exists"),
            ItemOutcome::Ok
        ])?,
        include_bytes!("fixtures/elixir_create_many_partial_response.bin")
    );
    Ok(())
}
