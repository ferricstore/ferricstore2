use super::{TestResult, caller, phases};
use wf_domain::claim::caller::{ClaimDueInput, ClaimStatesInput};
use wf_domain::claim::coordination::{
    ClaimCoordinationError, ClaimCoordinationLimits, ClaimPhase, ClaimPhaseCompletion,
    ClaimScheduleStatus, advance_claim_schedule, plan_claim_schedule,
    plan_coalesced_claim_schedule,
};

#[test]
fn ordinary_source_phase_order_and_remaining_quotas() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(4);
    input.reclaim_ratio = Some(50);
    let call = caller(input)?;
    let mut schedule = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
    let actual = phases(&mut schedule, &[1, 2, 1])?;
    assert_eq!(actual.len(), 3);
    assert!(actual[0].0 == ClaimPhase::Running);
    assert!(actual[1].0 == ClaimPhase::Normal);
    assert!(actual[2].0 == ClaimPhase::Running);
    assert_eq!([actual[0].1, actual[1].1, actual[2].1], [2, 3, 1]);
    assert!(schedule.status() == ClaimScheduleStatus::Complete);
    assert!(schedule.phase()?.is_none());
    assert_eq!(schedule.allocations(0)?.collect::<Vec<_>>(), [(0, 0, 1)]);
    assert!(matches!(
        schedule.allocations(3),
        Err(ClaimCoordinationError::InvalidObservation)
    ));
    input.reclaim_expired = Some(false);
    let disabled = caller(input)?;
    let mut schedule = plan_claim_schedule(&disabled, ClaimCoordinationLimits::default())?;
    let actual = phases(&mut schedule, &[2])?;
    assert!(actual[0].0 == ClaimPhase::Normal);
    assert_eq!(actual[0].1, 4);
    input.reclaim_ratio = Some(0);
    input.reclaim_expired = Some(true);
    let zero = caller(input)?;
    assert!(
        plan_claim_schedule(&zero, ClaimCoordinationLimits::default())?
            .phase()?
            .ok_or("missing requested phase")?
            .kind()
            == ClaimPhase::Requested
    );
    let simple = caller(ClaimDueInput::new(b"job", b"w"))?;
    assert!(
        plan_claim_schedule(&simple, ClaimCoordinationLimits::new(1, 1, 1092)?)?
            .phase()?
            .is_some()
    );
    assert!(matches!(
        plan_claim_schedule(&simple, ClaimCoordinationLimits::new(1, 1, 1091)?),
        Err(ClaimCoordinationError::WorkBudget)
    ));
    Ok(())
}

#[test]
fn ordinary_limit_one_and_running_override_are_distinct() -> TestResult {
    let call = caller(ClaimDueInput::new(b"job", b"w"))?;
    let mut fresh = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
    let actual = phases(&mut fresh, &[1])?;
    assert!(actual[0].0 == ClaimPhase::Normal);
    assert!(fresh.phase()?.is_none());
    let mut empty = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
    let actual = phases(&mut empty, &[0, 1])?;
    assert!(actual[0].0 == ClaimPhase::Normal && actual[1].0 == ClaimPhase::Running);
    let names: [&[u8]; 2] = [b"running", b"running"];
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.states = ClaimStatesInput::Many(&names);
    input.reclaim_expired = Some(false);
    input.reclaim_ratio = Some(0);
    let call = caller(input)?;
    let mut direct = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
    assert!(phases(&mut direct, &[1])?[0].0 == ClaimPhase::Running);
    for states in [ClaimStatesInput::Many(&[]), ClaimStatesInput::One(b"")] {
        input.states = states;
        assert!(matches!(
            plan_claim_schedule(&caller(input)?, ClaimCoordinationLimits::default()),
            Err(ClaimCoordinationError::InvalidStates)
        ));
    }
    let mixed: [&[u8]; 2] = [b"ANY", b"queued"];
    input.states = ClaimStatesInput::Many(&mixed);
    assert!(matches!(
        plan_claim_schedule(&caller(input)?, ClaimCoordinationLimits::default()),
        Err(ClaimCoordinationError::InvalidStates)
    ));
    for (lease, limit, ratio, expected) in [
        (0, 1, 25, ClaimCoordinationError::InvalidLease),
        (1, 0, 25, ClaimCoordinationError::InvalidLimit),
        (1, 1, 101, ClaimCoordinationError::InvalidRatio),
    ] {
        input.states = ClaimStatesInput::Any;
        input.lease_ms = Some(lease);
        input.limit = Some(limit);
        input.reclaim_ratio = Some(ratio);
        assert!(
            matches!(plan_claim_schedule(&caller(input)?, ClaimCoordinationLimits::default()), Err(e) if e == expected)
        );
    }
    Ok(())
}

#[test]
fn coalesced_caps_keep_original_call_allocations() -> TestResult {
    for ratio in [0, 1, 25, 50, 100] {
        let mut input = ClaimDueInput::new(b"job", b"w");
        input.reclaim_ratio = Some(ratio);
        let first = caller(input)?;
        let second = caller(input)?;
        let calls = [&first, &second];
        let mut schedule =
            plan_coalesced_claim_schedule(&calls, ClaimCoordinationLimits::default())?;
        let phase = schedule.phase()?.ok_or("missing coalesced phase")?;
        assert!(std::ptr::eq(phase.caller(), &raw const first));
        assert_eq!(phase.quota(), 2);
        assert!(
            phase.kind()
                == if ratio == 0 {
                    ClaimPhase::Requested
                } else {
                    ClaimPhase::Running
                }
        );
        advance_claim_schedule(
            &mut schedule,
            ClaimPhaseCompletion::Accepted {
                phase: 0,
                claimed: 2,
            },
        )?;
        assert_eq!(
            schedule.allocations(0)?.collect::<Vec<_>>(),
            [(0, 0, 1), (1, 1, 1)]
        );
        assert_eq!(
            schedule.allocations(2)?.collect::<Vec<_>>(),
            [(0, 0, 0), (1, 0, 0)]
        );
    }
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(2);
    input.reclaim_ratio = Some(25);
    let first = caller(input)?;
    input.limit = Some(3);
    let second = caller(input)?;
    let calls = [&first, &second];
    let mut schedule = plan_coalesced_claim_schedule(&calls, ClaimCoordinationLimits::default())?;
    assert_eq!(
        phases(&mut schedule, &[1, 2, 1])?
            .iter()
            .map(|p| p.1)
            .collect::<Vec<_>>(),
        [2, 4, 2]
    );
    assert_eq!(
        schedule.allocations(1)?.collect::<Vec<_>>(),
        [(0, 0, 1), (1, 1, 1)]
    );
    input.block_ms = Some(0);
    let blocked = caller(input)?;
    assert!(matches!(
        plan_coalesced_claim_schedule(&[&first, &blocked], ClaimCoordinationLimits::default()),
        Err(ClaimCoordinationError::PipelineBlockUnsupported)
    ));
    assert!(matches!(
        plan_coalesced_claim_schedule(&[&blocked], ClaimCoordinationLimits::default()),
        Err(ClaimCoordinationError::PipelineBlockUnsupported)
    ));
    input.block_ms = None;
    input.governance_limit_scope = Some(b"different");
    input.governance_shard_id = Some(0);
    let foreign = caller(input)?;
    assert!(matches!(
        plan_coalesced_claim_schedule(&[&first, &foreign], ClaimCoordinationLimits::default()),
        Err(ClaimCoordinationError::IncompatibleCalls)
    ));
    let max = ClaimCoordinationLimits::new(2, 3, 1_000_000)?;
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.reclaim_ratio = Some(0);
    input.limit = Some(3);
    let nonratio = caller(input)?;
    assert!(matches!(
        plan_coalesced_claim_schedule(&[&nonratio, &nonratio], max),
        Err(ClaimCoordinationError::InvalidLimit)
    ));
    Ok(())
}

#[test]
fn schedule_failure_preserves_earlier_counts_and_halts() -> TestResult {
    let mut input = ClaimDueInput::new(b"job", b"w");
    input.limit = Some(4);
    let call = caller(input)?;
    for unknown in [false, true] {
        let mut schedule = plan_claim_schedule(&call, ClaimCoordinationLimits::default())?;
        assert!(matches!(
            advance_claim_schedule(
                &mut schedule,
                ClaimPhaseCompletion::Accepted {
                    phase: 1,
                    claimed: 1
                }
            ),
            Err(ClaimCoordinationError::InvalidObservation)
        ));
        assert!(matches!(
            advance_claim_schedule(
                &mut schedule,
                ClaimPhaseCompletion::Accepted {
                    phase: 0,
                    claimed: 2
                }
            ),
            Err(ClaimCoordinationError::InvalidObservation)
        ));
        assert_eq!(schedule.phase()?.ok_or("missing phase")?.quota(), 1);
        advance_claim_schedule(
            &mut schedule,
            ClaimPhaseCompletion::Accepted {
                phase: 0,
                claimed: 1,
            },
        )?;
        let completion = if unknown {
            ClaimPhaseCompletion::UnknownOutcome { phase: 1 }
        } else {
            ClaimPhaseCompletion::KnownFailure { phase: 1 }
        };
        advance_claim_schedule(&mut schedule, completion)?;
        assert!(
            schedule.status()
                == if unknown {
                    ClaimScheduleStatus::UnknownOutcome
                } else {
                    ClaimScheduleStatus::KnownFailure
                }
        );
        assert!(schedule.phase()?.is_none());
        assert_eq!(schedule.allocations(0)?.collect::<Vec<_>>(), [(0, 0, 1)]);
        assert!(matches!(
            advance_claim_schedule(&mut schedule, completion),
            Err(ClaimCoordinationError::InvalidObservation)
        ));
    }
    assert!(ClaimCoordinationLimits::new(0, 1, 1).is_err());
    assert!(ClaimCoordinationLimits::new(100_001, 1, 1).is_err());
    assert!(plan_coalesced_claim_schedule(&[], ClaimCoordinationLimits::default()).is_err());
    Ok(())
}
