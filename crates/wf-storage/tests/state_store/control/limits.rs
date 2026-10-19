use super::{TestResult, f, fixtures};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, GroupControlChanges, GroupControlTransaction,
    StateTransactionError,
};
use wf_storage::state::{
    RedbStateStore, StateStoreGenesis, StateStoreLimits,
    value::{self, StateValueError},
};

#[test]
fn applied_membership_values_have_independent_frames_and_budgets() -> TestResult {
    let s = f::scope()?;
    let limits = RaftMembershipLimits::default();
    let body = fixtures::hex(fixtures::MEMBER)?;
    let none = fixtures::row(None, None)?;
    let some = fixtures::row(Some(f::log(0)), Some(&body))?;
    assert_eq!(none, fixtures::hex(fixtures::NONE)?);
    assert_eq!(some, fixtures::hex(fixtures::SOME)?);
    assert_eq!(
        value::encode_applied_membership(s.group(), s.recovery(), None, None, limits, 45)?,
        none
    );
    assert_eq!(
        value::encode_applied_membership(
            s.group(),
            s.recovery(),
            Some(f::log(0)),
            Some(&body),
            limits,
            211
        )?,
        some
    );
    assert_eq!(
        value::measure_applied_membership(&none, s.group(), s.recovery(), limits)?,
        1024
    );
    assert_eq!(
        value::measure_applied_membership(&some, s.group(), s.recovery(), limits)?,
        4661
    );
    assert!(matches!(
        value::encode_applied_membership(
            s.group(),
            s.recovery(),
            Some(f::log(0)),
            Some(&body),
            limits,
            210
        ),
        Err(StateValueError::Limit { .. })
    ));
    assert!(
        value::encode_applied_membership(
            s.group(),
            s.recovery(),
            Some(f::log(0)),
            None,
            limits,
            1000
        )
        .is_err()
    );
    assert!(
        value::encode_applied_membership(s.group(), s.recovery(), None, Some(&body), limits, 1000)
            .is_err()
    );
    let tiny_wire = RaftMembershipLimits::new(3, 6, 4, 2, 1, 4661)?;
    assert_eq!(
        value::encode_applied_membership(s.group(), s.recovery(), None, None, tiny_wire, 45)?,
        none
    );
    for (bytes, u, present) in [(&none, 1024, false), (&some, 4661, true)] {
        let m = value::decode_applied_membership(bytes, s.group(), s.recovery(), limits, u)?;
        assert_eq!(m.entry().is_some(), present);
        assert!(matches!(
            value::decode_applied_membership(bytes, s.group(), s.recovery(), limits, u - 1),
            Err(StateValueError::Limit { .. })
        ));
        if present {
            assert_eq!(m.log_id(), Some(f::log(0)));
            assert_eq!(m.stamp(), Some(EntryStamp::Unstamped));
            assert_eq!(
                m.entry().ok_or("entry")?.prepared().as_bytes().as_slice(),
                fixtures::hex("ec550282ecf2cb11f71e4f9a9aa2dac6318ab8772c0da4c432d9c30175a3edbe")?
            );
            assert_eq!(m.membership().configs(), fixtures::membership()?.configs());
            assert_eq!(m.membership().nodes(), fixtures::membership()?.nodes());
        }
    }
    Ok(())
}

fn create_profile(
    path: &std::path::Path,
    profile: RaftMembershipLimits,
) -> TestResult<RedbStateStore> {
    let s = f::scope()?;
    let placement = f::placements()?;
    let policies = [(s.tenant().clone(), f::rich_policy()?)];
    Ok(RedbStateStore::create(
        path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placement,
            policies: &policies,
        },
        StateStoreLimits::default().with_membership_limits(profile),
    )?)
}

#[test]
fn captured_membership_profiles_and_cumulative_guard_budgets() -> TestResult {
    let dir = f::directory("member-caps")?;
    let path = dir.join("state.redb");
    let tiny = RaftMembershipLimits::new(64, 1_000_000, 65536, 1_048_576, 16 * f::MIB, 1023)?;
    assert!(
        matches!(create_profile(&path,tiny),Err(e)if e.downcast_ref::<StateTransactionError>()==Some(&StateTransactionError::Limit))
    );
    assert!(
        !path.exists(),
        "initial unreadable profile must refuse before file creation"
    );
    assert_blank_profile(&dir.join("blank-wire.redb"))?;
    let small = RaftMembershipLimits::new(64, 1_000_000, 3, 1_048_576, 16 * f::MIB, 64 * f::MIB)?;
    let mut store = create_profile(&path, small)?;
    drop(store);
    let rows = f::raw_rows(&path)?;
    let meta = fixtures::raw_meta(&path)?;
    store = RedbStateStore::open(
        &path,
        f::scope()?.group().clone(),
        f::scope()?.recovery().clone(),
        StateStoreLimits::default().with_membership_limits(small),
    )?;
    let s = f::scope()?;
    let body = fixtures::frame(true, EntryStamp::Unstamped)?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        tx.control()?,
        f::log(0),
        body.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    assert_eq!(
        tx.commit_control(&changes),
        Err(StateTransactionError::Limit)
    );
    assert_eq!(fixtures::current(&mut store)?.last_entry(), None);
    drop(store);
    fixtures::assert_pair(&path, &rows, &meta)?;
    store = f::open(&path)?;
    fixtures::apply(&mut store, &body, f::log(0))?;
    assert_query_profiles(&mut store, small)?;
    drop(store);
    let low_open = RedbStateStore::open(
        &path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreLimits::default().with_membership_limits(small),
    );
    assert!(matches!(low_open, Err(StateTransactionError::Limit)));
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn assert_query_profiles(store: &mut RedbStateStore, small: RaftMembershipLimits) -> TestResult {
    let s = f::scope()?;
    for (r, success) in [(5173, true), (5172, false)] {
        let tx = store.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            r,
            65536,
        )?;
        tx.control()?;
        let m = tx.applied_membership(RaftMembershipLimits::default());
        if success {
            assert_eq!(m?.log_id(), Some(f::log(0)));
        } else {
            assert!(matches!(m, Err(StateTransactionError::Limit)));
        }
    }
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        f::MIB,
        f::MIB,
    )?;
    assert!(matches!(
        tx.applied_membership(small),
        Err(StateTransactionError::Limit)
    ));
    let larger =
        RaftMembershipLimits::new(65, 1_000_000, 65536, 1_048_576, 16 * f::MIB, 64 * f::MIB)?;
    assert!(matches!(
        tx.applied_membership(larger),
        Err(StateTransactionError::InvalidChanges)
    ));
    assert_eq!(
        tx.applied_membership(RaftMembershipLimits::default())?
            .log_id(),
        Some(f::log(0))
    );
    drop(tx);
    Ok(())
}

fn assert_blank_profile(path: &std::path::Path) -> TestResult {
    let profile = RaftMembershipLimits::new(64, 1_000_000, 65536, 1_048_576, 1, 64 * f::MIB)?;
    let mut store = create_profile(path, profile)?;
    drop(store);
    let rows = f::raw_rows(path)?;
    let meta = fixtures::raw_meta(path)?;
    let s = f::scope()?;
    store = RedbStateStore::open(
        path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreLimits::default().with_membership_limits(profile),
    )?;
    let body = fixtures::frame(false, EntryStamp::Unstamped)?;
    let tx = store.begin_apply(
        s.group(),
        s.recovery(),
        AdmissionLimits::default(),
        100,
        1024,
        65536,
    )?;
    let expected = tx.control()?;
    let changes = GroupControlChanges::new(
        s.group(),
        s.recovery(),
        expected,
        f::log(0),
        body.as_bytes(),
        RaftMembershipLimits::default(),
    )?;
    assert_eq!(
        tx.commit_control(&changes),
        Err(StateTransactionError::Limit)
    );
    assert_eq!(fixtures::current(&mut store)?, expected);
    drop(store);
    fixtures::assert_pair(path, &rows, &meta)
}
