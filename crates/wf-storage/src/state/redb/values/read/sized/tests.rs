//! Live-reader corruption controls after authentic consumed publication.

use std::path::PathBuf;

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::WorkflowValueSizedReadValue;
use wf_ports::clock::WorkflowValueReadClock;
use wf_ports::state::StateTransactionError;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use crate::state::key;
use crate::state::redb::RedbStateStore;

mod fixtures;
use fixtures::fixture_data::{
    cleanup, clock, corrupt_alias, fixture, read_limits, table_rows, write_row,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct SizedReadFixture {
    path: PathBuf,
    store: RedbStateStore,
    scope: RequestScope,
    reference: Vec<u8>,
}

struct SizedReadTestClock {
    hlc: HlcTimestamp,
    wall: TimestampMillis,
    calls: usize,
    failure: Option<StateTransactionError>,
}

impl WorkflowValueReadClock for SizedReadTestClock {
    fn read_snapshot(&mut self) -> Result<(HlcTimestamp, TimestampMillis), StateTransactionError> {
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(StateTransactionError::Limit)?;
        self.failure.map_or(Ok((self.hlc, self.wall)), Err)
    }
}

#[test]
fn sized_value_reads_do_not_hide_live_digest_or_length_corruption() -> TestResult {
    for fault in 0..2 {
        let fixture = fixture("bytes", fault, false)?;
        let mut clock = clock(249)?;
        let healthy = fixture.store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &fixture.scope,
            &[&fixture.reference],
            1,
            read_limits(),
        );
        assert!(healthy.is_ok(), "{:?}", healthy.as_ref().err());
        let healthy = healthy?;
        assert_eq!(healthy.slots.as_slice(), [Ok(Some(0))]);
        assert!(matches!(
            healthy.values.as_slice(),
            [WorkflowValueSizedReadValue::Omitted { stored_bytes: 9 }]
        ));
        let before = table_rows(&fixture.store, false)?;
        let metadata = table_rows(&fixture.store, true)?;
        let immutable = key::encode_immutable_key(&fixture.scope, &fixture.reference, 0, 65_535)?;
        let mut changed = before
            .get(&immutable)
            .ok_or("missing real immutable")?
            .clone();
        assert_eq!(changed.as_slice(), b"123456789");
        if fault == 0 {
            changed[0] = b'X';
        } else {
            changed.push(b'!');
        }
        write_row(&fixture.store, &immutable, Some(&changed))?;
        let mut expected = before;
        expected.insert(immutable, changed);
        assert_eq!(table_rows(&fixture.store, false)?, expected);
        assert_eq!(table_rows(&fixture.store, true)?, metadata);
        let corrupted = fixture.store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &fixture.scope,
            &[&fixture.reference],
            1,
            read_limits(),
        );
        assert!(matches!(corrupted, Err(StateTransactionError::Corrupt)));
        assert_eq!(table_rows(&fixture.store, false)?, expected);
        assert_eq!(table_rows(&fixture.store, true)?, metadata);
        assert_eq!(clock.calls, 2);
        assert!(matches!(
            healthy.values.as_slice(),
            [WorkflowValueSizedReadValue::Omitted { stored_bytes: 9 }]
        ));
        cleanup(fixture)?;
    }
    Ok(())
}

#[test]
fn sized_value_reads_do_not_hide_missing_immutable_or_alias_corruption() -> TestResult {
    for fault in 0..6 {
        let fixture = fixture("alias", fault, fault == 4)?;
        let mut clock = clock(249)?;
        let healthy = fixture.store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &fixture.scope,
            &[&fixture.reference],
            1,
            read_limits(),
        );
        assert!(healthy.is_ok(), "{:?}", healthy.as_ref().err());
        let healthy = healthy?;
        assert_eq!(healthy.slots.as_slice(), [Ok(Some(0))]);
        assert!(matches!(
            healthy.values.as_slice(),
            [WorkflowValueSizedReadValue::Omitted { stored_bytes: 9 }]
        ));
        let before = table_rows(&fixture.store, false)?;
        let metadata = table_rows(&fixture.store, true)?;
        let immutable = key::encode_immutable_key(&fixture.scope, &fixture.reference, 0, 65_535)?;
        let alias = key::encode_alias_key(&fixture.scope, &fixture.reference, 65_535)?;
        let mut expected = before.clone();
        match fault {
            0 => {
                expected.remove(&immutable).ok_or("missing immutable")?;
                write_row(&fixture.store, &immutable, None)?;
            }
            1..=3 => {
                let changed = corrupt_alias(before.get(&alias).ok_or("missing alias")?, fault)?;
                expected.insert(alias.clone(), changed.clone());
                write_row(&fixture.store, &alias, Some(&changed))?;
            }
            4 => {
                let mut changed = before
                    .get(&immutable)
                    .ok_or("missing expired bytes")?
                    .clone();
                assert_eq!(changed.as_slice(), b"123456789");
                changed[0] = b'X';
                expected.insert(immutable.clone(), changed.clone());
                write_row(&fixture.store, &immutable, Some(&changed))?;
                clock.hlc = HlcTimestamp::new(250, 0)?;
                clock.wall = TimestampMillis::new(250);
            }
            5 => {
                expected.remove(&alias).ok_or("missing alias")?;
                write_row(&fixture.store, &alias, None)?;
            }
            _ => return Err("unexpected corruption control".into()),
        }
        assert_eq!(table_rows(&fixture.store, false)?, expected);
        assert_eq!(table_rows(&fixture.store, true)?, metadata);
        let corrupted = fixture.store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &fixture.scope,
            &[&fixture.reference],
            1,
            read_limits(),
        );
        if fault == 5 {
            assert!(corrupted.is_ok(), "{:?}", corrupted.as_ref().err());
            let missing = corrupted?;
            assert!(missing.values.is_empty());
            assert_eq!(missing.slots.as_slice(), [Ok(None)]);
        } else {
            assert!(matches!(corrupted, Err(StateTransactionError::Corrupt)));
        }
        assert_eq!(table_rows(&fixture.store, false)?, expected);
        assert_eq!(table_rows(&fixture.store, true)?, metadata);
        assert_eq!(clock.calls, 2);
        cleanup(fixture)?;
    }
    Ok(())
}
