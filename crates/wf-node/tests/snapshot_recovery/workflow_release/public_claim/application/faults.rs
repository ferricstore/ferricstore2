use super::{fixtures::*, *};
#[test]
fn pending_original_survives_current_denial_unresolved_lookup_and_redirect_before_later_completion()
-> TestResult {
    let dir = f::directory("claim-app-pending-refusals")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    port.fault = ClaimApplicationFault::BeforeCommit;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    let R::Unknown { original, .. } = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("pending original".into());
    };
    let pointer = original.frame().as_ptr();
    port.fault = ClaimApplicationFault::DenyLookup;
    let R::Deferred {
        reason: N::Unauthorized,
        original,
    } = retry_public_claim_call(original, &mut port, Limits::default())?
    else {
        return Err("current denial keeps original".into());
    };
    assert_eq!(original.frame().as_ptr(), pointer);
    port.fault = ClaimApplicationFault::LookupUnknown;
    let R::Unknown {
        reason: U::Unavailable,
        original,
    } = retry_public_claim_call(original, &mut port, Limits::default())?
    else {
        return Err("unresolved inquiry keeps original".into());
    };
    assert_eq!(original.frame().as_ptr(), pointer);
    port.fault = ClaimApplicationFault::RefuseSubmit;
    let R::Deferred {
        reason: N::Redirect,
        original,
    } = retry_public_claim_call(original, &mut port, Limits::default())?
    else {
        return Err("redirect keeps original".into());
    };
    assert_eq!(original.frame().as_ptr(), pointer);
    assert_eq!(owner(&port)?.lease(0).ok_or("unspent original")?.in_use, 0);
    assert_eq!(port.log.state()?.committed(), Some(f::at(2)));
    port.fault = ClaimApplicationFault::None;
    assert!(matches!(
        retry_public_claim_call(original, &mut port, Limits::default())?,
        R::Retained(_)
    ));
    assert_eq!(
        port.trace
            .iter()
            .filter(|event| **event == ClaimApplicationTrace::Prepare)
            .count(),
        1
    );
    assert_eq!(
        port.trace
            .iter()
            .filter(|event| **event == ClaimApplicationTrace::Commit)
            .count(),
        1
    );
    assert!(
        port.frames
            .iter()
            .all(|frame| Arc::ptr_eq(&port.frames[0], frame))
    );
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn actual_pending_original_keeps_one_arc_across_unknown_then_retry_commits_only_it() -> TestResult {
    let dir = f::directory("claim-app-pending-original")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    port.fault = ClaimApplicationFault::BeforeCommit;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    let R::Unknown { original, .. } = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("pending original".into());
    };
    assert_eq!(owner(&port)?.lease(0).ok_or("unspent lease")?.in_use, 0);
    assert_eq!(port.log.state()?.committed(), Some(f::at(2)));
    let pointer = original.frame().as_ptr();
    let R::Retained(outcome) = retry_public_claim_call(original, &mut port, Limits::default())?
    else {
        return Err("committed pending original".into());
    };
    assert_eq!(outcome.original_position(), f::at(3));
    assert_eq!(port.frames[1].as_bytes().as_ptr(), pointer);
    assert!(Arc::ptr_eq(&port.frames[0], &port.frames[1]));
    assert_eq!(
        port.trace,
        [
            ClaimApplicationTrace::Lookup,
            ClaimApplicationTrace::Prepare,
            ClaimApplicationTrace::Submit,
            ClaimApplicationTrace::Lookup,
            ClaimApplicationTrace::Submit,
            ClaimApplicationTrace::Commit
        ]
    );
    assert_eq!(owner(&port)?.lease(0).ok_or("used lease")?.in_use, 2);
    assert_eq!(port.log.state()?.last_log(), Some(f::at(3)));
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn lost_reply_is_resolved_from_actual_original_before_another_spend_or_nonce() -> TestResult {
    let dir = f::directory("claim-app-lost-original")?;
    let mut port = ClaimApplicationFixture::new(&dir)?;
    port.fault = ClaimApplicationFault::LoseReply;
    let (scope, id, caller) = (f::scope()?, id()?, caller()?);
    let R::Unknown { original, .. } = submit_public_claim_call(
        Request {
            scope: &scope,
            request: &id,
            caller: &caller,
        },
        &mut port,
        Limits::default(),
    )?
    else {
        return Err("lost original reply".into());
    };
    let used = owner(&port)?.lease(0).ok_or("paid lease")?.in_use;
    assert_eq!(used, 2);
    port.trace.clear();
    assert!(matches!(
        retry_public_claim_call(original, &mut port, Limits::default())?,
        R::Retained(_)
    ));
    assert_eq!(port.trace, [ClaimApplicationTrace::Lookup]);
    assert_eq!(port.frames.len(), 1);
    assert_eq!(
        owner(&port)?.lease(0).ok_or("unchanged paid lease")?.in_use,
        used
    );
    drop(port);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
