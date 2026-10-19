//! Independent genesis framing, donor admission and strict constructor controls.

use redb::ReadableDatabase;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use wf_domain::policy::complete::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, complete_policy_value_owned_bytes,
    prepare_complete_policy_value_v2, restore_complete_policy, verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::ControlState;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_types::identity::{GroupId, PartitionId, TenantId};

use super::{TestResult, fixtures as f};
mod inventory;
pub(super) fn candidate(
    name: &[u8],
    version: Option<Vec<u8>>,
) -> TestResult<CompletePolicyCandidate> {
    let schema = PolicySchemaLimits::default();
    Ok(restore_complete_policy(
        7,
        CompletePolicyData {
            workflow_type: name.to_vec(),
            version: version.map(PolicyVersion::Bytes),
            max_active_ms: None,
            retry: None,
            retention: None,
            states: BTreeMap::new(),
            indexed_attributes: None,
            indexed_state_meta: None,
            governance: None,
        },
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?)
}
pub(super) fn minimal(name: &[u8]) -> TestResult<CompletePolicyValue> {
    let schema = PolicySchemaLimits::default();
    Ok(prepare_complete_policy_value_v2(
        candidate(name, None)?,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?)
}
/// Pack the reviewed version independently of the product manifest parser.
pub(super) fn manifest(
    group: &[u8],
    recovery: &[u8],
    now: u64,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
) -> TestResult<Vec<u8>> {
    let mut hash = Sha256::new();
    hash.update(b"wf-state-genesis-v1");
    let mut counts = [0_u64; 2];
    for family in [6_u8, 7] {
        for (key, value) in rows.iter().filter(|(key, _)| key.get(6) == Some(&family)) {
            counts[usize::from(family - 6)] += 1;
            hash.update(u64::try_from(key.len())?.to_be_bytes());
            hash.update(key);
            hash.update(u64::try_from(value.len())?.to_be_bytes());
            hash.update(value);
        }
    }
    let mut wire = b"FWSS\0\x03".to_vec();
    wire.extend_from_slice(&u16::try_from(group.len())?.to_be_bytes());
    wire.extend_from_slice(group);
    wire.extend_from_slice(&u16::try_from(recovery.len())?.to_be_bytes());
    wire.extend_from_slice(recovery);
    wire.extend_from_slice(&now.to_be_bytes());
    wire.extend_from_slice(&0_u16.to_be_bytes());
    wire.extend_from_slice(&now.to_be_bytes());
    wire.extend_from_slice(&counts[1].to_be_bytes());
    wire.extend_from_slice(&counts[0].to_be_bytes());
    wire.extend_from_slice(&hash.finalize());
    let checksum = Sha256::digest(&wire);
    wire.extend_from_slice(&checksum);
    assert_eq!(wire.len(), 108 + group.len() + recovery.len());
    Ok(wire)
}

#[test]
fn scoped_genesis_and_reopen() -> TestResult {
    owner_metrics()?;
    exact_genesis()?;
    constructor_refusals()?;
    whole_input_admission()?;
    Ok(())
}

fn owner_metrics() -> TestResult {
    let schema = PolicySchemaLimits::default();
    let value = minimal(b"r")?;
    assert_eq!(value.encoded().len(), 40);
    assert_eq!(
        value.encoded(),
        hex("46575056000200000000000000070000001646575053000200000001720000000000000000000000")?
    );
    let mut charge = PolicySchemaCharge::new(schema);
    // Two represented nodes, one type byte and the separate 40-byte envelope.
    assert_eq!(
        complete_policy_value_owned_bytes(&value, schema, &mut charge)?,
        3_113
    );
    assert_eq!(charge.p(), 0);
    let first = charge.g();
    assert!(first > 0);
    assert_eq!(
        complete_policy_value_owned_bytes(&value, schema, &mut charge)?,
        3_113
    );
    assert_eq!(charge.g(), 2 * first);
    verify_complete_policy_value_v2(&value, schema, &mut charge)?;
    assert!(charge.g() > 2 * first);
    let maximum = prepare_complete_policy_value_v2(
        candidate(b"r", Some(vec![0; 262_117]))?,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?;
    assert_eq!(maximum.encoded().len(), 262_162);
    assert_eq!(
        complete_policy_value_owned_bytes(&maximum, schema, &mut PolicySchemaCharge::new(schema))?,
        527_352
    );
    let mismatched = candidate(b"r", Some(vec![0; 262_117]))?.bind_encoded_snapshot(
        0..0,
        Vec::new(),
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?;
    assert_eq!(mismatched.encoded(), &[]);
    assert_eq!(
        complete_policy_value_owned_bytes(
            &mismatched,
            schema,
            &mut PolicySchemaCharge::new(schema)
        )?,
        265_190
    );
    assert!(
        verify_complete_policy_value_v2(&mismatched, schema, &mut PolicySchemaCharge::new(schema))
            .is_err()
    );
    let tight = PolicySchemaLimits::new(schema.policy_limits(), first - 1)?;
    assert!(matches!(
        complete_policy_value_owned_bytes(&value, tight, &mut PolicySchemaCharge::new(tight)),
        Err(PolicySchemaError::WorkBytes { .. })
    ));
    let mut other = PolicySchemaCharge::new(tight);
    assert_eq!(
        complete_policy_value_owned_bytes(&value, schema, &mut other),
        Err(PolicySchemaError::ProfileMismatch)
    );
    assert_eq!(other.g(), 0);
    assert_eq!(other.p(), 0);
    let rich = f::rich_policy()?;
    assert_eq!(
        complete_policy_value_owned_bytes(&rich, schema, &mut PolicySchemaCharge::new(schema))?,
        8_339
    );
    Ok(())
}

fn exact_genesis() -> TestResult {
    let root = f::directory("genesis")?;
    let path = root.join("state.redb");
    let scope = f::scope()?;
    let placements = vec![
        (PartitionId::new(b"p".to_vec())?, scope.group().clone()),
        (
            PartitionId::new(b"z".to_vec())?,
            GroupId::new(b"other".to_vec())?,
        ),
    ];
    let policies = [(scope.tenant().clone(), minimal(b"r")?)];
    let mut store = RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(17)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?;
    assert_eq!(store.group(), scope.group());
    assert_eq!(store.recovery(), scope.recovery());
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        100,
        f::MIB,
        f::MIB,
    )?;
    assert_eq!(tx.control()?, ControlState::initial(f::clock(17)?));
    assert_eq!(tx.placement(&placements[0].0)?, Some(scope.group().clone()));
    assert_eq!(
        tx.placement(&placements[1].0)?,
        Some(placements[1].1.clone())
    );
    assert_eq!(tx.placement(&PartitionId::new(b"absent".to_vec())?)?, None);
    drop(tx);
    drop(store);
    let mut expected = BTreeMap::from([(
        key::encode_control_key(scope.group(), scope.recovery(), f::MIB)?,
        value::encode_control(
            scope.group(),
            scope.recovery(),
            ControlState::initial(f::clock(17)?),
            f::MIB,
        )?,
    )]);
    for (partition, group) in &placements {
        expected.insert(
            key::encode_placement_key(scope.recovery(), partition, f::MIB)?,
            group.as_bytes().to_vec(),
        );
    }
    expected.insert(
        key::encode_policy_key(&scope, b"r", f::MIB)?,
        policies[0].1.encoded().to_vec(),
    );
    assert_eq!(f::raw_rows(&path)?, expected);
    let packed = manifest(b"g", b"z", 17, &expected)?;
    assert_eq!(f::raw_manifest(&path)?, packed);
    assert_eq!(
        packed,
        hex(
            "46575353000300016700017a00000000000000110000000000000000001100000000000000020000000000000001e0f976e00789b0cd089958944f473308862f10357046f939ababbfe6c5c81e8d909cd168f3d5b3703f1914e40fedaa4646208b33ccc21cdfb0fadb997d2ad5a8"
        )?
    );
    let database = inventory::check(&path)?;
    let reopened = RedbStateStore::open_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    drop(reopened);
    assert_eq!(f::raw_rows(&path)?, expected);
    assert_eq!(f::raw_manifest(&path)?, packed);
    let reopened = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    drop(reopened);
    f::genesis_control_lower_bound(&path)?;
    f::missing_genesis_control(&path)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn empty_genesis<'a>() -> TestResult<StateStoreGenesis<'a>> {
    Ok(StateStoreGenesis {
        initial_time: f::clock(0)?,
        placements: &[],
        policies: &[],
    })
}

fn constructor_refusals() -> TestResult {
    let root = f::directory("refusals")?;
    let scope = f::scope()?;
    let absent = root.join("absent.redb");
    assert!(matches!(
        RedbStateStore::open(
            &absent,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::Unavailable)
    ));
    assert!(!absent.exists());
    let path = root.join("state.redb");
    drop(f::create(&path)?);
    let rows = f::raw_rows(&path)?;
    let manifest = f::raw_manifest(&path)?;
    assert!(matches!(
        RedbStateStore::create(
            &path,
            scope.group().clone(),
            scope.recovery().clone(),
            empty_genesis()?,
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::NamespaceConflict)
    ));
    assert!(matches!(
        RedbStateStore::open(
            &path,
            GroupId::new(b"foreign".to_vec())?,
            scope.recovery().clone(),
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::NamespaceConflict)
    ));
    assert_eq!(f::raw_rows(&path)?, rows);
    assert_eq!(f::raw_manifest(&path)?, manifest);
    let foreign = root.join("foreign.redb");
    let database = redb::Builder::new().create(&foreign)?;
    let write = database.begin_write()?;
    write.open_multimap_table(redb::MultimapTableDefinition::<u64, u64>::new("foreign"))?;
    write.commit()?;
    assert!(matches!(
        RedbStateStore::create_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            empty_genesis()?,
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::NamespaceConflict)
    ));
    let empty = root.join("empty.redb");
    let database = redb::Builder::new().create(&empty)?;
    assert!(matches!(
        RedbStateStore::open_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::Corrupt)
    ));
    let database = redb::Builder::new().open(&empty)?;
    assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    drop(RedbStateStore::create_database(
        database,
        scope.group().clone(),
        scope.recovery().clone(),
        empty_genesis()?,
        StateStoreLimits::default(),
    )?);
    scope_profiles(&empty)?;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn scope_profiles(empty: &Path) -> TestResult {
    let scope = f::scope()?;
    let mut store = f::open(empty)?;
    let too_large = AdmissionLimits::new(2 * f::MIB, 16 * f::MIB, 1_000)?;
    assert!(matches!(
        store.begin_apply(
            scope.group(),
            scope.recovery(),
            too_large,
            10,
            f::MIB,
            f::MIB
        ),
        Err(StateTransactionError::InvalidChanges)
    ));
    let foreign_scope = GroupId::new(b"other".to_vec())?;
    assert!(matches!(
        store.begin_apply(
            &foreign_scope,
            scope.recovery(),
            AdmissionLimits::default(),
            10,
            f::MIB,
            f::MIB
        ),
        Err(StateTransactionError::InvalidChanges)
    ));
    assert_eq!(
        store
            .begin_apply(
                scope.group(),
                scope.recovery(),
                AdmissionLimits::default(),
                10,
                f::MIB,
                f::MIB
            )?
            .control()?,
        ControlState::initial(f::clock(0)?)
    );
    drop(store);
    Ok(())
}

fn profile(genesis_bytes: usize, workspace: usize) -> TestResult<StateStoreLimits> {
    Ok(StateStoreLimits::new(
        AdmissionLimits::default(),
        PolicySchemaLimits::default(),
        (1_000, genesis_bytes),
        (1_000_000, 1024 * f::MIB, 10_000_000),
        128 * f::MIB,
        workspace,
    )?)
}

fn whole_input_admission() -> TestResult {
    let root = f::directory("input-admission")?;
    let scope = f::scope()?;
    let placement = f::placements()?;
    let schema = PolicySchemaLimits::default();
    let policies = [(scope.tenant().clone(), minimal(b"r")?)];
    // G514 + fixed2048 + placement514 + policy(512+tenant1+U3113) + C.
    let input = 4_140;
    let exact = 128 * f::MIB + 2_562 + input;
    let good = root.join("exact.redb");
    drop(RedbStateStore::create(
        &good,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placement,
            policies: &policies,
        },
        profile(input, exact)?,
    )?);
    for (name, bytes, workspace) in [
        ("input", input - 1, exact),
        ("workspace", input, exact - 1),
        ("tiny-workspace", input, 1),
    ] {
        let path = root.join(name);
        assert!(matches!(
            RedbStateStore::create(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreGenesis {
                    initial_time: f::clock(0)?,
                    placements: &placement,
                    policies: &policies
                },
                profile(bytes, workspace)?
            ),
            Err(StateTransactionError::Limit)
        ));
        assert!(!path.exists());
    }
    let mismatch = [(
        scope.tenant().clone(),
        candidate(b"r", Some(vec![0; 262_117]))?.bind_encoded_snapshot(
            0..0,
            Vec::new(),
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?,
    )];
    let path = root.join("mismatched");
    assert!(matches!(
        RedbStateStore::create(
            &path,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0)?,
                placements: &placement,
                policies: &mismatch
            },
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::InvalidChanges)
    ));
    assert!(!path.exists());
    invalid_orders(&root)?;
    let small = [
        (scope.tenant().clone(), minimal(b"a")?),
        (scope.tenant().clone(), minimal(b"b")?),
        (scope.tenant().clone(), minimal(b"c")?),
    ];
    let path = root.join("three-small");
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placement,
            policies: &small,
        },
        StateStoreLimits::default(),
    )?);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

fn invalid_orders(root: &Path) -> TestResult {
    let scope = f::scope()?;
    for (name, placements) in [
        (
            "duplicate",
            vec![
                (PartitionId::new(b"p".to_vec())?, scope.group().clone()),
                (PartitionId::new(b"p".to_vec())?, scope.group().clone()),
            ],
        ),
        (
            "unsorted",
            vec![
                (PartitionId::new(b"z".to_vec())?, scope.group().clone()),
                (PartitionId::new(b"p".to_vec())?, scope.group().clone()),
            ],
        ),
    ] {
        let path = root.join(name);
        assert!(matches!(
            RedbStateStore::create(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreGenesis {
                    initial_time: f::clock(0)?,
                    placements: &placements,
                    policies: &[]
                },
                StateStoreLimits::default()
            ),
            Err(StateTransactionError::InvalidChanges)
        ));
        assert!(!path.exists());
    }
    for names in [[b"r".as_slice(), b"r"], [b"z".as_slice(), b"a"]] {
        let policies = [
            (TenantId::new(b"t".to_vec())?, minimal(names[0])?),
            (TenantId::new(b"t".to_vec())?, minimal(names[1])?),
        ];
        let path = root.join(format!("policy-{names:?}"));
        assert!(matches!(
            RedbStateStore::create(
                &path,
                scope.group().clone(),
                scope.recovery().clone(),
                StateStoreGenesis {
                    initial_time: f::clock(0)?,
                    placements: &[],
                    policies: &policies
                },
                StateStoreLimits::default()
            ),
            Err(StateTransactionError::InvalidChanges)
        ));
        assert!(!path.exists());
    }
    for (genesis, recovery, temporary, workspace) in [
        ((0, 1), (1, 1, 1), 128 * f::MIB, 1),
        ((1, 0), (1, 1, 1), 128 * f::MIB, 1),
        ((1, 1), (0, 1, 1), 128 * f::MIB, 1),
        ((1, 1), (1, 0, 1), 128 * f::MIB, 1),
        ((1, 1), (1, 1, 0), 128 * f::MIB, 1),
        ((1, 1), (1, 1, 1), 0, 1),
        ((1, 1), (1, 1, 1), 128 * f::MIB - 1, 1),
        ((1, 1), (1, 1, 1), 128 * f::MIB, 0),
    ] {
        assert!(matches!(
            StateStoreLimits::new(
                AdmissionLimits::default(),
                PolicySchemaLimits::default(),
                genesis,
                recovery,
                temporary,
                workspace
            ),
            Err(StateTransactionError::InvalidLimits)
        ));
    }
    Ok(())
}

fn hex(text: &str) -> TestResult<Vec<u8>> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}
