use super::*;

#[test]
fn policy_snapshot2_receive_install_and_downgrade_refusal_are_atomic() -> TestResult {
    let dir = f::directory("policy-receive5")?;
    let source = dir.join("source");
    let s = f::scope()?;
    let mut expected = published(&source)?;
    let wire = f::snapshot_wire(&s, &expected, 2)?;
    let facts = (
        Sha256::digest(&wire).into(),
        3 + f::rows(&expected)?.len(),
        wire.len(),
    );
    let mut state = f::open(&source, &s)?;
    let mut captured_wire = Vec::new();
    assert_eq!(
        state
            .capture_snapshot(&mut captured_wire, SnapshotLimits::default())?
            .id(),
        &facts.0
    );
    assert_eq!(captured_wire, wire);
    drop(state);
    frozen(&mut expected, 0)?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(facts), None], Some(0), None, None)?,
    );
    assert_eq!(f::cohort(&source)?, expected);
    let target = dir.join("target");
    let mut active = f::create(&target, &s, &[b"p"], &[])?;
    let incoming = stage(&dir.join("stage"), &wire)?;
    assert_eq!(
        active
            .install_snapshot(incoming, SnapshotLimits::default())?
            .id(),
        &facts.0
    );
    assert!(active.supports_snapshot_transitions()?);
    drop(active);
    assert_eq!(f::cohort(&target)?, expected);
    drop(f::open(&target, &s)?);
    interrupted_and_min_target(&dir, &wire, &expected)?;
    downgrade(&dir, &target, &expected)?;
    stage_and_commit_faults(&dir, &wire, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let s = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        s.group().clone(),
        s.recovery().clone(),
        &mut Cursor::new(wire),
        f::store_limits()?,
        SnapshotLimits::default(),
    )?)
}

fn interrupted_and_min_target(dir: &Path, wire: &[u8], new: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    for cut in [6, wire.len() - 1] {
        let path = dir.join(format!("truncated-{cut}"));
        assert!(matches!(
            RedbSnapshotStage::receive(
                &path,
                s.group().clone(),
                s.recovery().clone(),
                &mut Cursor::new(&wire[..cut]),
                f::store_limits()?,
                SnapshotLimits::default()
            ),
            Err(SnapshotError::Malformed)
        ));
        assert_empty_stage(&path)?;
    }
    let mut failed_input = std::fs::File::open(dir)?;
    assert!(matches!(
        RedbSnapshotStage::receive(
            &dir.join("io-error"),
            s.group().clone(),
            s.recovery().clone(),
            &mut failed_input,
            f::store_limits()?,
            SnapshotLimits::default()
        ),
        Err(SnapshotError::InputUnavailable)
    ));
    target_c_pair(dir, wire, new)?;
    Ok(())
}

fn target_c_pair(dir: &Path, wire: &[u8], new: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    let schema = PolicySchemaLimits::new(PolicyLimits::default(), 50_000)?;
    // SAME schema work is sufficient: eight full P52 decodes cost 28,856.
    // Source LIVE+frozen alone need four U52 owners = 53,120 cumulative C.
    // Only actual target C changes; the wide positive rules out a schema mask.
    for maximum_c in [50_000, 2 * 1_048_576] {
        let target_limits = StateStoreLimits::new(
            f::limits().codec.admission(),
            schema,
            (1000, 128 * 1_048_576),
            (1_000_000, 1_073_741_824, 10_000_000),
            maximum_c,
            256 * 1_048_576,
        )?;
        let target = dir.join(format!("target-c-{maximum_c}"));
        let placements = [(PartitionId::new(b"p".to_vec())?, s.group().clone())];
        drop(RedbStateStore::create(
            &target,
            s.group().clone(),
            s.recovery().clone(),
            wf_storage::state::StateStoreGenesis {
                initial_time: f::clock(0)?,
                placements: &placements,
                policies: &[],
            },
            target_limits,
        )?);
        let before = f::cohort(&target)?;
        let mut active = RedbStateStore::open(
            &target,
            s.group().clone(),
            s.recovery().clone(),
            target_limits,
        )?;
        let candidate = stage(&dir.join(format!("stage-c-{maximum_c}")), wire)?;
        let result = active.install_snapshot(candidate, SnapshotLimits::default());
        if maximum_c == 50_000 {
            assert!(matches!(
                result,
                Err(SnapshotError::Stage(StateTransactionError::Limit))
            ));
            drop(active.begin_apply(
                s.group(),
                s.recovery(),
                f::limits().codec.admission(),
                100,
                1_000_000,
                maximum_c,
            )?);
        } else {
            assert_eq!(result?.id().as_slice(), Sha256::digest(wire).as_slice());
            assert!(active.supports_snapshot_transitions()?);
        }
        drop(active);
        assert_eq!(
            f::cohort(&target)?,
            if maximum_c == 50_000 {
                before
            } else {
                new.clone()
            }
        );
        drop(RedbStateStore::open(
            &target,
            s.group().clone(),
            s.recovery().clone(),
            target_limits,
        )?);
    }
    Ok(())
}

fn downgrade(dir: &Path, target: &Path, expected: &f::Cohort) -> TestResult {
    let legacy = dir.join("legacy");
    let s = f::scope()?;
    let mut old = f::create(&legacy, &s, &[b"p"], &[])?;
    for at in 0_u64..2 {
        let id = [b'a' + u8::try_from(at)?];
        let row = f::owner(
            &s,
            b"p",
            f::data(&id, b"other", b"q", at, Some(at))?,
            0,
            u128::from(at) << 32,
        )?;
        f::seed(&mut old, &s, &[row], at, at)?;
    }
    let mut wire = Vec::new();
    assert_eq!(
        old.capture_snapshot(&mut wire, SnapshotLimits::default())?
            .control()
            .position()
            .get(),
        1
    );
    drop(old);
    let candidate = stage(&dir.join("legacy-stage"), &wire)?;
    let mut active = f::open(target, &s)?;
    assert!(matches!(
        active.install_snapshot(candidate, SnapshotLimits::default()),
        Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout
        ))
    ));
    assert!(active.supports_snapshot_transitions()?);
    drop(active);
    assert_eq!(
        f::cohort(target)?,
        *expected,
        "downgrade retired live or immutable data"
    );
    drop(f::open(target, &s)?);
    Ok(())
}

fn stage_and_commit_faults(dir: &Path, wire: &[u8], new: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    for source_fault in [true, false] {
        let target = dir.join(if source_fault {
            "source-fault-target"
        } else {
            "commit-fault-target"
        });
        drop(f::create(&target, &s, &[b"p"], &[])?);
        let old = f::cohort(&target)?;
        let source = dir.join(if source_fault {
            "bad-source"
        } else {
            "commit-source"
        });
        drop(redb::Builder::new().create(&source)?);
        let fault = Arc::new(AtomicU8::new(0));
        let database = super::super::super::faults::fault_database(&source, Arc::clone(&fault), 0)?;
        let candidate = RedbSnapshotStage::receive_database(
            database,
            s.group().clone(),
            s.recovery().clone(),
            &mut Cursor::new(wire),
            f::store_limits()?,
            SnapshotLimits::default(),
        )?;
        let source_before = new.clone();
        let target_fault = Arc::new(AtomicU8::new(0));
        let database =
            super::super::super::faults::fault_database(&target, Arc::clone(&target_fault), 0)?;
        let mut active = RedbStateStore::open_database(
            database,
            s.group().clone(),
            s.recovery().clone(),
            f::store_limits()?,
        )?;
        if source_fault {
            fault.store(3, Ordering::SeqCst);
        } else {
            target_fault.store(4, Ordering::SeqCst);
        }
        let error = active
            .install_snapshot(candidate, SnapshotLimits::default())
            .err()
            .ok_or("snapshot fault acknowledged")?;
        if source_fault {
            assert_eq!(
                error,
                SnapshotError::Stage(StateTransactionError::Unavailable)
            );
            assert_eq!(fault.load(Ordering::SeqCst), 0);
            assert!(active.supports_snapshot_transitions().is_ok());
        } else {
            assert_eq!(
                error,
                SnapshotError::State(StateTransactionError::Uncertain)
            );
            assert_eq!(
                active.supports_snapshot_transitions(),
                Err(SnapshotError::State(StateTransactionError::Unavailable))
            );
        }
        drop(active);
        let actual = f::cohort(&target)?;
        if source_fault {
            assert_eq!(actual, old);
        } else {
            assert!(actual == old || actual == *new);
        }
        assert_eq!(
            f::cohort(&source)?,
            source_before,
            "consumed stage mutated by failed install"
        );
        drop(f::open(&target, &s)?);
    }
    Ok(())
}
