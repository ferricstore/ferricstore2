use super::{fixtures as s, *};

#[test]
fn step_node_audits_complete_historical_body_before_any_new_publication() -> TestResult {
    let limits = s::limits()?;
    let healthy = f::directory("step-node-audit-control")?;
    historical(&healthy, "healthy")?;
    let before = s::cohort(&healthy)?;
    drop(s::recover(&healthy, &limits)?);
    assert_eq!(s::cohort(&healthy)?, before);
    std::fs::remove_dir_all(healthy)?;
    for case in ["digest", "last-value", "eof", "namespace"] {
        let dir = f::directory("step-node-audit-corrupt")?;
        historical(&dir, case)?;
        s::refuse(&dir, &limits, None)?;
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

fn historical(dir: &Path, case: &str) -> TestResult {
    let (mut log, mut state, _) = s::seed(dir, false, true)?;
    let frame = s::frame(b"step", 200, Some(b"new"))?;
    let mut bytes = frame.as_bytes().to_vec();
    // Literal t/g/z plus request step: intent30..32 and digest32..64.
    assert_eq!(&bytes[30..32], 10_u16.to_be_bytes());
    match case {
        "healthy" => {}
        "digest" => bytes[32] ^= 1,
        "last-value" => *bytes.last_mut().ok_or("payload")? ^= 1,
        "eof" => bytes.push(0),
        "namespace" => bytes[16] = b'x',
        _ => return Err("audit case".into()),
    }
    f::append(&mut log, 2, &bytes)?;
    drop(s::apply(&mut state, &frame, 2)?);
    let blank = f::blank(wf_domain::workflow::control::EntryStamp::Stamped(f::clock(
        210, 0, 210,
    )?))?;
    f::append(&mut log, 3, blank.as_bytes())?;
    f::apply_control(&mut state, 3, blank.as_bytes())?;
    log.save_committed(f::at(3))?;
    drop((log, state));
    Ok(())
}
