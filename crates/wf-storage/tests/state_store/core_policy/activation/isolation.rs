//! Complete-field preservation across binary namespace boundaries.

use super::*;

#[test]
fn policy_publication_preserves_full_fields_and_binary_namespace_isolation() -> TestResult {
    let dir = f::directory("policy-complete-binary")?;
    let path = dir.join("state");
    let base = f::scope()?;
    let s = RequestScope::new(
        TenantId::new(vec![0, 255])?,
        base.group().clone(),
        base.recovery().clone(),
    );
    let foreign = RequestScope::new(
        TenantId::new(vec![0, 255, 0])?,
        base.group().clone(),
        base.recovery().clone(),
    );
    let kind = [0, 255];
    let mut store = f::create(
        &path,
        &s,
        &[b"\0", b"\0\xff"],
        &[
            (
                s.tenant().clone(),
                f::policy(&kind, 7, &[(b"q", StateMode::Parallel)], true)?,
            ),
            (
                foreign.tenant().clone(),
                f::policy(&kind, 1, &[(b"q", StateMode::Parallel)], false)?,
            ),
        ],
    )?;
    let target = f::owner(&s, b"\0", f::data(b"same", &kind, b"q", 0, Some(0))?, 2, 0)?;
    let other = f::owner(
        &foreign,
        b"\0\xff",
        f::data(b"same", &kind, b"q", 0, Some(0))?,
        1,
        1_u128 << 32,
    )?;
    f::seed(&mut store, &s, std::slice::from_ref(&target), 0, 0)?;
    f::seed(&mut store, &foreign, std::slice::from_ref(&other), 1, 0)?;
    drop(store);
    let before = f::cohort(&path)?;
    let mut expected = before.clone();
    let frame = f::frame(
        &s,
        b"full",
        100,
        &kind,
        Some(7),
        f::mode_patch(&[(b"q", Some(StateMode::Fifo))]),
    )?;
    let response = f::policy_wire(&kind, 8, &[(b"q", StateMode::Fifo)], true)?;
    assert_eq!(
        f::rows_mut(&mut expected)?.remove(&f::due(&s, &target)?),
        Some(vec![0])
    );
    f::rows_mut(&mut expected)?.insert(f::due(&s, &target)?, vec![1]);
    f::rows_mut(&mut expected)?.insert(f::member(&s, &target), Vec::new());
    f::rows_mut(&mut expected)?.insert(
        f::key(6, &[b"g", b"z", s.tenant().as_bytes(), &kind]),
        response.clone(),
    );
    let metadata = f::metadata(
        &s,
        b"full",
        2,
        100,
        f::frame_intent(frame.as_bytes(), 2)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        &mut expected,
        &s,
        frame.as_bytes(),
        f::log(2),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade5(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    f::apply(&path, &s, frame.as_bytes(), 2, &expected, &response)?;
    assert_preserved(&expected, &before, &s, &foreign, &target, &other)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_preserved(
    expected: &f::Cohort,
    before: &f::Cohort,
    s: &RequestScope,
    foreign: &RequestScope,
    target: &StoredWorkflowRecord,
    other: &StoredWorkflowRecord,
) -> TestResult {
    let kind = [0, 255];
    assert_eq!(
        f::rows(expected)?.get(&f::point(s, target)),
        Some(&f::record_wire(s, target)?)
    );
    assert_eq!(
        f::rows(expected)?.get(&f::point(foreign, other)),
        f::rows(before)?.get(&f::point(foreign, other))
    );
    assert_eq!(
        f::rows(expected)?.get(&f::due(foreign, other)?),
        Some(&vec![0])
    );
    assert_eq!(
        f::rows(expected)?.get(&f::key(
            6,
            &[b"g", b"z", foreign.tenant().as_bytes(), &kind]
        )),
        f::rows(before)?.get(&f::key(
            6,
            &[b"g", b"z", foreign.tenant().as_bytes(), &kind]
        ))
    );
    Ok(())
}
