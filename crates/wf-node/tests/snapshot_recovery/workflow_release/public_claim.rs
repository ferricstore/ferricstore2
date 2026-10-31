//! Real public funded parent replay with source IDs and native retained custody.
use super::{E, TestResult, f, fixtures as p, frames as q};
use wf_domain::{
    claim::{
        caller::{
            ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
            normalize_claim_due,
        },
        coordination::ClaimCoordinationLimits,
    },
    governance::credit::CreditLimits,
};
use wf_node::recovery::{CommittedRecoveryLimits, recover_committed};
use wf_ports::{
    log::{
        CreditSpendPreparation, PreparedBytes, PublicClaimPreparation, encode_public_claim_request,
        generate_public_credit_ids, inspect_public_claim_reply,
    },
    state::{GroupApplyStore, GroupApplyTransaction},
};

fn limits() -> TestResult<CommittedRecoveryLimits> {
    Ok(p::selected(true)?
        .with_identified_credit_catalog_limits(
            4,
            CreditLimits::new(4096, 1000, 8 * f::MIB, 128 * u64::try_from(f::MIB)?)?,
        )?
        .with_atomic_claim_recovery(
            ClaimCallerLimits::default(),
            ClaimCoordinationLimits::default(),
        )?
        .with_public_claim_recovery(128 * u64::try_from(f::MIB)?, 32 * f::MIB)?)
}
fn frames() -> TestResult<Vec<PreparedBytes>> {
    let mut frames = q::all()?;
    frames.truncate(3);
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"w");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(2);
    input.lease_ms = Some(30000);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    let caller = normalize_claim_due(input, ClaimCallerLimits::default())?;
    frames.push(encode_public_claim_request(
        &f::scope()?,
        &wf_types::identity::RequestId::new(b"public-node".to_vec())?,
        f::clock(104, 0, 104)?,
        &caller,
        PublicClaimPreparation {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 1000,
            head_budget: 1000,
            batch: [0x7a; 16],
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?);
    Ok(frames)
}

#[test]
fn startup_spends_actual_source_ids_and_reopens_the_single_original_public_receipt() -> TestResult {
    let dir = f::directory("public-node-funding")?;
    let (mut log, state) = p::pair(&dir, true)?;
    let frames = frames()?;
    p::append(&mut log, &frames, None)?;
    let scope = f::scope()?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?)?;
    assert_eq!(p::record(&mut state, b"i")?.record.state(), b"running");
    assert_eq!(p::record(&mut state, b"j")?.record.state(), b"running");
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let outcome = tx
        .outcome(
            scope.tenant(),
            &wf_types::identity::RequestId::new(b"public-node".to_vec())?,
        )?
        .ok_or("public parent outcome")?;
    assert_eq!(
        (
            outcome.intent().version(),
            outcome.response_version(),
            outcome.original_position()
        ),
        (21, 18, f::at(3))
    );
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        f::driver_limits().codec.admission(),
    )?;
    let claim = reply.claim().ok_or("actual claim")?;
    let original = generate_public_credit_ids(
        2,
        CreditSpendPreparation {
            lease_epoch: 1,
            batch: [0x7a; 16],
        },
        f::MIB,
    )?;
    assert_eq!(claim.count(), 2);
    assert_eq!(
        claim
            .used_reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        original.iter().map(Vec::as_slice).collect::<Vec<_>>()
    );
    assert_eq!(tx.control()?.position(), f::at(3).index());
    drop(tx);
    drop((log, state));
    let before = p::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        limits()?,
    )?);
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn old_atomic_profile_refuses_funded_parent_without_changing_the_actual_cohort() -> TestResult {
    let dir = f::directory("public-node-old-profile")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = frames()?;
    p::apply(&mut state, &frames, 3)?;
    p::append(&mut log, &frames, None)?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    let old = p::selected(true)?.with_atomic_claim_recovery(
        ClaimCallerLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), old).err(),
        Some(E::UnsupportedEntry)
    );
    assert_eq!(p::cohort(&dir)?, before);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn malformed_historical_public_body_is_audited_behind_later_control() -> TestResult {
    use wf_domain::workflow::control::EntryStamp;
    use wf_ports::log::{CodecError, RaftLogStore};
    let dir = f::directory("public-node-historical-body")?;
    let (mut log, mut state) = p::pair(&dir, true)?;
    let frames = frames()?;
    for (index, frame) in frames.iter().enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if index == 3 {
            body.push(0);
        }
        f::append(&mut log, u64::try_from(index)?, &body)?;
    }
    p::apply(&mut state, &frames, 3)?;
    let native = wf_storage::transaction::PublicClaimTransactionLimits {
        base: wf_storage::transaction::AtomicClaimTransactionLimits {
            values: p::values()?,
            work: wf_domain::claim::apply::ClaimWorkLimits::default(),
            caller: ClaimCallerLimits::default(),
            coordination: ClaimCoordinationLimits::default(),
            credit: CreditLimits::new(4096, 1000, 8 * f::MIB, 128 * u64::try_from(f::MIB)?)?,
        },
        maximum_credit_work: 128 * u64::try_from(f::MIB)?,
        maximum_credit_temporary_bytes: 32 * f::MIB,
    };
    wf_storage::transaction::apply_public_claim_transaction(
        &mut state,
        &f::scope()?,
        frames[3].as_bytes(),
        f::at(3),
        native,
    )?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, 4, blank.as_bytes())?;
    f::apply_control(&mut state, 4, blank.as_bytes())?;
    log.save_committed(f::at(4))?;
    drop((log, state));
    let before = p::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let (log, state) = f::open_pair(&dir)?;
    let scope = f::scope()?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(p::cohort(&dir)?, before);
    assert_eq!(f::log_rows(&dir)?, logs);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn public_recovery_requires_complete_dependencies_and_prospective_credit_ownership() -> TestResult {
    assert!(matches!(
        p::selected(true)?.with_public_claim_recovery(128 * u64::try_from(f::MIB)?, 32 * f::MIB),
        Err(E::InvalidLimits)
    ));
    let atomic = p::selected(true)?.with_atomic_claim_recovery(
        ClaimCallerLimits::default(),
        ClaimCoordinationLimits::default(),
    )?;
    assert!(matches!(
        atomic.with_public_claim_recovery(1, 1),
        Err(E::InvalidLimits)
    ));
    assert!(matches!(
        limits()?.with_public_claim_recovery(0, 32 * f::MIB),
        Err(E::InvalidLimits)
    ));
    Ok(())
}
