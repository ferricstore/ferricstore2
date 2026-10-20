//! Historical original-policy framing, owner bounds and durable prefix errors.

use super::{
    CommittedRecoveryLimits, Error, RaftMembershipLimits, TestResult, caps, cohort, f, normal,
    pending, policy, publish, recover_committed, reference,
};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, CompletePolicyReplaceApplyError as ReplaceError,
    CompletePolicyRequestError as RequestError,
    OriginalCompletePolicyReplaceApplyError as RawError, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::workflow::control::EntryStamp;
use wf_ports::log::RaftLogStore;
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::OriginalPolicyTransactionError;

#[test]
fn original_audit_optin_and_bounded_body_pool_preserve_origins() -> TestResult {
    assert_header_only_peak()?;
    assert_historical_corruption()?;
    assert_failed_prefix()?;
    Ok(())
}

fn assert_header_only_peak() -> TestResult {
    let dir = f::directory("original-audit-header")?;
    let scope = f::scope_for(b"t", b"original-group", b"original-recovery")?;
    let (mut log, mut state) = f::create_pair_for(&dir, &scope)?;
    let bytes = policy(&scope, b"set", 11, false)?;
    f::append(&mut log, 0, &bytes)?;
    publish(&mut state, &scope, 0, &bytes, 1)?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let before = cohort(&dir)?;
    let profile = normal();
    // Independent actual join base: two namespace owners + fixed2048 +512.
    // This deliberately uses longer names than the old core-policy fixture.
    let base = 512
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
        + 512
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
        + 2048
        + 512;
    let exact = base
        + 512
        + usize::try_from(profile.schema.max_work_bytes())?
        + 65_536
        + 2048
        + 3 * bytes.len();
    assert!(exact < profile.workspace_bytes);
    for maximum in [exact - 1, exact] {
        let limits = CommittedRecoveryLimits::new(
            RaftMembershipLimits::default(),
            (100, 8192, 1024, 1024),
            profile,
            (1, bytes.len()),
            512 + bytes.len(),
            maximum,
        )?
        .with_original_policy_sets();
        let (log, state) = f::open_pair_for(&dir, &scope)?;
        let result = recover_committed(log, state, scope.group(), scope.recovery(), limits);
        if maximum == exact {
            drop(result?);
        } else {
            assert_eq!(result.err(), Some(Error::Limit));
        }
        drop(f::open_pair_for(&dir, &scope)?);
        assert_eq!(cohort(&dir)?, before);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_historical_corruption() -> TestResult {
    for mode in 0..3 {
        let dir = f::directory("original-audit-malformed")?;
        let (mut log, mut state) = f::create_pair(&dir)?;
        let scope = f::scope()?;
        let valid = policy(&scope, b"set", 11, false)?;
        let mut bad = valid.clone();
        if mode == 0 {
            bad.push(0);
        } else if mode == 1 {
            let offset = 7
                + [b"t".as_slice(), b"g", b"z", b"set"]
                    .iter()
                    .map(|s| 4 + s.len())
                    .sum::<usize>()
                + 2;
            bad[offset] ^= 1;
        } else {
            *bad.last_mut().ok_or("sidecar end")? ^= 1;
        }
        f::append(&mut log, 0, &bad)?;
        publish(&mut state, &scope, 0, &valid, 1)?;
        let blank = f::blank(EntryStamp::Unstamped)?;
        f::append(&mut log, 1, blank.as_bytes())?;
        f::apply_control(&mut state, 1, blank.as_bytes())?;
        log.save_committed(f::at(1))?;
        drop((log, state));
        let before = cohort(&dir)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let (log, state) = f::open_pair(&dir)?;
        let error = recover_committed(log, state, scope.group(), scope.recovery(), caps(normal())?)
            .err()
            .ok_or("old malformed body not audited")?;
        let Error::OriginalPolicy(OriginalPolicyTransactionError::Original(RawError::Replace(
            ReplaceError::Request(reason),
        ))) = error
        else {
            return Err(format!("historical origin changed: {error:?}").into());
        };
        if mode == 0 {
            assert_eq!(reason, RequestError::Encoding(EncodingError::TrailingBytes));
        } else if mode == 1 {
            assert_eq!(reason, RequestError::IntentMismatch);
        } else {
            assert!(matches!(reason, RequestError::Encoding(_)));
        }
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, before);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn assert_failed_prefix() -> TestResult {
    let source = f::directory("original-prefix-source")?;
    let frames = reference(&source)?;
    for mode in 0..3 {
        let dir = f::directory("original-prefix-limit")?;
        pending(&dir, &frames)?;
        // Commit a real create as the expected durable prefix before SET fails.
        let (log, mut state) = f::open_pair(&dir)?;
        drop(f::apply_normal(&mut state, 1, &frames[1], 5)?);
        drop((log, state));
        let before = cohort(&dir)?;
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let mut driver = normal();
        if mode == 0 {
            driver.schema = PolicySchemaLimits::new(driver.schema.policy_limits(), 1)?;
        } else if mode == 1 {
            driver.maximum_read_steps = 1;
        } else {
            driver.maximum_adapter_bytes = 1;
        }
        let scope = f::scope()?;
        let (log, state) = f::open_pair(&dir)?;
        let error = recover_committed(log, state, scope.group(), scope.recovery(), caps(driver)?)
            .err()
            .ok_or("narrow original profile accepted")?;
        if mode == 0 {
            assert!(matches!(
                error,
                Error::OriginalPolicy(OriginalPolicyTransactionError::Original(RawError::Replace(
                    ReplaceError::Request(RequestError::Encoding(EncodingError::Schema(
                        PolicySchemaError::WorkBytes { maximum: 1, .. }
                    )))
                )))
            ));
        } else {
            assert_eq!(
                error,
                Error::OriginalPolicy(OriginalPolicyTransactionError::Transaction(
                    StateTransactionError::Limit
                ))
            );
        }
        drop(f::open_pair(&dir)?);
        assert_eq!(cohort(&dir)?, before);
        assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        let (log, state) = f::open_pair(&dir)?;
        drop(recover_committed(
            log,
            state,
            scope.group(),
            scope.recovery(),
            caps(normal())?,
        )?);
        assert_eq!(cohort(&dir)?, cohort(&source)?);
        std::fs::remove_dir_all(dir)?;
    }
    std::fs::remove_dir_all(source)?;
    Ok(())
}
