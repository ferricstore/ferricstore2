use super::{fixtures as s, *};
use wf_ports::consensus::RaftMembershipLimits;

#[test]
fn step_node_limits_reject_zero_overflow_and_narrow_profiles_without_writes() -> TestResult {
    let healthy = s::limits()?;
    let selected = healthy.step_continue_limits().ok_or("Step profile")?;
    assert_eq!(selected.maximum_value_rows, 1024);
    assert_eq!(selected.maximum_value_bytes, 64 * f::MIB);
    assert_eq!(selected.maximum_value_work, 64 * f::MIB);
    assert_eq!(selected.maximum_value_temporary_bytes, 64 * f::MIB);
    for caps in [[0, 1, 1, 1], [1, 0, 1, 1], [1, 1, 0, 1], [1, 1, 1, 0]] {
        assert!(matches!(
            v::limits()?.with_step_continue_limits(caps[0], caps[1], caps[2], caps[3]),
            Err(Error::InvalidLimits)
        ));
    }
    let overflow = Limits::new(
        RaftMembershipLimits::default(),
        (10000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        v::limits()?.normal_limits(),
        (usize::MAX, 1),
        16 * f::MIB,
        1024 * f::MIB,
    )?;
    assert!(matches!(
        overflow.with_step_continue_limits(1, 1, 2, 1),
        Err(Error::Limit)
    ));
    let dir = f::directory("step-node-budget")?;
    s::pending(&dir, false, false)?;
    for caps in [
        [1, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB],
        [1024, 1, 64 * f::MIB, 64 * f::MIB],
        [1024, 64 * f::MIB, 1, 64 * f::MIB],
        [1024, 64 * f::MIB, 64 * f::MIB, 1],
    ] {
        let limits = v::limits()?.with_step_continue_limits(caps[0], caps[1], caps[2], caps[3])?;
        s::refuse(&dir, &limits, None)?;
    }
    for case in [0, 1, 2] {
        let mut normal = v::limits()?.normal_limits();
        if case == 0 {
            normal.workspace_bytes = 1;
        }
        let limits = Limits::new(
            RaftMembershipLimits::default(),
            (10000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
            normal,
            (if case == 1 { 1 } else { 1000 }, 64 * f::MIB),
            if case == 2 { 512 } else { 16 * f::MIB },
            1024 * f::MIB,
        )?
        .with_core_policy_patches()
        .with_original_policy_sets()
        .with_step_continue_limits(1024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
        s::refuse(&dir, &limits, None)?;
    }
    drop(s::recover(&dir, &healthy)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
