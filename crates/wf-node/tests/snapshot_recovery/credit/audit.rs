//! Historical full bodies cannot hide behind a valid applied last entry.

use super::*;

#[test]
fn credit_node_full_historical_audit_rejects_trailing_body_without_reexecution() -> TestResult {
    let dir = f::directory("credit-node-full-audit")?;
    let good = dir.join("good");
    std::fs::create_dir(&good)?;
    let (log, state, _) = c::input(&good, false)?;
    let scope = f::scope()?;
    let (log, mut state) =
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?)?;
    verify(&mut state)?;
    drop((log, state));
    let bad = dir.join("bad");
    std::fs::create_dir(&bad)?;
    let (log, state, _) = c::input(&bad, true)?;
    drop((log, state));
    let before = c::cohort(&bad)?;
    let rows = f::log_rows(&bad)?;
    let metadata = f::log_meta(&bad)?;
    let (log, state) = f::open_pair(&bad)?;
    assert_eq!(
        recover_committed(log, state, scope.group(), scope.recovery(), limits()?).err(),
        Some(E::Codec(CodecError::TrailingBytes))
    );
    assert_eq!(c::cohort(&bad)?, before);
    assert_eq!(f::log_rows(&bad)?, rows);
    assert_eq!(f::log_meta(&bad)?, metadata);
    drop(f::open_pair(&bad)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
