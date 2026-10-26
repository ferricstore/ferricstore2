//! Received image custody, actual target carry and profile error origins.
use super::*;

pub(super) fn downgrade(dir: &Path, target: &Path, expected: &f::Cohort) -> TestResult {
    let path = dir.join("legacy");
    let s = f::scope()?;
    let mut state = f::create(&path, &[], 64)?;
    for at in 0_u64..2 {
        let id = [b'a' + u8::try_from(at)?];
        let row = f::owner(
            &s,
            b"p",
            f::data(&id, b"other", b"q", at, Some(at))?,
            0,
            u128::from(at) << 32,
        )?;
        f::seed(&mut state, &s, &[row], at, at)?;
    }
    let mut wire = Vec::new();
    assert_eq!(
        state
            .capture_snapshot(&mut wire, SnapshotLimits::default())?
            .control()
            .position()
            .get(),
        1
    );
    drop(state);
    let mut active = f::open(target, 64)?;
    assert!(matches!(
        active.install_snapshot(
            stage(&dir.join("legacy-stage"), &wire)?,
            SnapshotLimits::default()
        ),
        Err(SnapshotError::State(
            StateTransactionError::UnsupportedLayout
        ))
    ));
    assert!(active.supports_snapshot_transitions()?);
    drop(active);
    assert_eq!(f::cohort(target)?, *expected);
    drop(f::open(target, 64)?);
    Ok(())
}

pub(super) fn stage_and_commit_faults(dir: &Path, wire: &[u8], new: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    for source_fault in [true, false] {
        let suffix = if source_fault { "source" } else { "commit" };
        let target = dir.join(format!("fault-{suffix}"));
        drop(f::create(&target, &[], 64)?);
        let old = f::cohort(&target)?;
        let source = dir.join(format!("stage-{suffix}"));
        drop(redb::Builder::new().create(&source)?);
        let fault = Arc::new(AtomicU8::new(0));
        let candidate = RedbSnapshotStage::receive_database(
            fault_database(&source, Arc::clone(&fault), 0)?,
            s.group().clone(),
            s.recovery().clone(),
            &mut Cursor::new(wire),
            f::store_limits(64)?,
            SnapshotLimits::default(),
        )?;
        let target_fault = Arc::new(AtomicU8::new(0));
        let mut active = RedbStateStore::open_database(
            fault_database(&target, Arc::clone(&target_fault), 0)?,
            s.group().clone(),
            s.recovery().clone(),
            f::store_limits(64)?,
        )?;
        if source_fault {
            fault.store(3, Ordering::SeqCst);
        } else {
            target_fault.store(4, Ordering::SeqCst);
        }
        let error = active
            .install_snapshot(candidate, SnapshotLimits::default())
            .err()
            .ok_or("fault acknowledged")?;
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
        assert_eq!(f::cohort(&source)?, *new, "failed consumed stage changed");
        drop(f::open(&target, 64)?);
    }
    Ok(())
}

#[test]
fn original_snapshot_profile_corruption_and_target_carry_are_prospective() -> TestResult {
    let dir = f::directory("original-snapshot-profile")?;
    let source = dir.join("source");
    let (expected, wire) = capture(&source, published(&source, true)?)?;
    for cut in [6, wire.len() - 1] {
        let path = dir.join(format!("truncated-{cut}"));
        assert!(
            matches!(stage(&path,&wire[..cut]).err().and_then(|error|error.downcast::<SnapshotError>().ok()),Some(error)if *error==SnapshotError::Malformed)
        );
        assert_empty_stage(&path)?;
    }
    malformed_streams(&dir, &wire)?;
    profile_origins(&dir)?;
    target_c_pair(&dir, &wire, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn malformed_streams(dir: &Path, original: &[u8]) -> TestResult {
    let s = f::scope()?;
    for case in 0..5 {
        let mut wire = original.to_vec();
        let expected = if case == 0 {
            SnapshotError::UnsupportedVersion
        } else {
            SnapshotError::Malformed
        };
        match case {
            0 => wire[5] = u8::MAX,
            1 => wire[5] = 2,
            2 => wire[19] = 2,
            3 => wire.push(0),
            4 => wire[5] = 4,
            _ => return Err("unknown original snapshot mutation".into()),
        }
        if case != 3 {
            f::rehash(&mut wire)?;
        }
        let path = dir.join(format!("bad-stream-{case}"));
        assert!(
            matches!(RedbSnapshotStage::receive(&path,s.group().clone(),s.recovery().clone(),&mut Cursor::new(&wire),f::store_limits(64)?,SnapshotLimits::default()),Err(error)if error==expected)
        );
        assert_empty_stage(&path)?;
    }
    Ok(())
}

fn profile_origins(dir: &Path) -> TestResult {
    let source = dir.join("profile-base");
    let expected = published(&source, false)?;
    for case in 0..4 {
        let path = dir.join(format!("profile-{case}"));
        drop(f::create(&path, &[], 64)?);
        let mut bad = expected.clone();
        if case == 0 {
            let manifest = bad[0]
                .as_mut()
                .ok_or("no metadata")?
                .get_mut(b"manifest".as_slice())
                .ok_or("no manifest")?;
            manifest[110..114].copy_from_slice(&2_u32.to_be_bytes());
            f::rehash(manifest)?;
        } else {
            let key = f::key(6, &[b"g", b"z", b"t", b"r"]);
            let raw = f::rows_mut(&mut bad)?
                .get_mut(&key)
                .ok_or("no original policy")?;
            if case != 3 {
                assert_eq!(&raw[28..34], b"states");
                raw[28] = b'x';
            }
            if case >= 2 {
                raw.push(0);
            }
            f::upgrade6(
                &mut bad,
                &f::scope()?,
                f::catalog(&f::scope()?, [None, None], None, None, None)?,
            )?;
        }
        f::replace(&path, &bad)?;
        let s = f::scope()?;
        assert!(
            matches!(RedbStateStore::open(&path,s.group().clone(),s.recovery().clone(),f::store_limits(64)?),
            Err(error)if error==if case<2{StateTransactionError::UnsupportedPolicyProfile}else{StateTransactionError::Corrupt})
        );
        assert_eq!(
            f::cohort(&path)?,
            bad,
            "strict open repaired unknown/malformed profile"
        );
    }
    Ok(())
}

fn target_c_pair(dir: &Path, wire: &[u8], new: &f::Cohort) -> TestResult {
    let s = f::scope()?;
    let schema = PolicySchemaLimits::new(PolicyLimits::default(), 3 * 1_048_576)?;
    let x = 65_536 + 256 * 8258;
    assert_eq!(x, 2_179_584);
    assert!(x < 3 * 1_048_576 && 4 * x > 3 * 1_048_576);
    // Both arms have SAME schema. A single original validation fits; LIVE and
    // frozen policy+receipt validations cumulatively exceed narrow actual C.
    // The full wide positive prevents work/profile refusal masking the C cut.
    for maximum_c in [3 * 1_048_576, 32 * 1_048_576] {
        let limits = StateStoreLimits::new(
            f::limits().codec.admission(),
            schema,
            (1000, 128 * 1_048_576),
            (1_000_000, 1_073_741_824, 10_000_000),
            maximum_c,
            256 * 1_048_576,
        )?
        .with_history_max_events_limit(64)?;
        let path = dir.join(format!("target-c-{maximum_c}"));
        let placements = [(PartitionId::new(b"p".to_vec())?, s.group().clone())];
        drop(RedbStateStore::create(
            &path,
            s.group().clone(),
            s.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0)?,
                placements: &placements,
                policies: &[],
            },
            limits,
        )?);
        let before = f::cohort(&path)?;
        let mut active =
            RedbStateStore::open(&path, s.group().clone(), s.recovery().clone(), limits)?;
        let result = active.install_snapshot(
            stage(&dir.join(format!("stage-c-{maximum_c}")), wire)?,
            SnapshotLimits::default(),
        );
        if maximum_c == 3 * 1_048_576 {
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
            f::cohort(&path)?,
            if maximum_c == 3 * 1_048_576 {
                before
            } else {
                new.clone()
            }
        );
        drop(RedbStateStore::open(
            &path,
            s.group().clone(),
            s.recovery().clone(),
            limits,
        )?);
    }
    Ok(())
}
