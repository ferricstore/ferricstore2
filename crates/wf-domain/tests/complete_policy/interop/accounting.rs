//! Independent conversion and raw-splice admission frontiers.

use super::{
    Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, StoredPolicyValue,
    TestResult, candidate, data, encode, f, limits,
};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, original_core_policy_patch_bytes,
    prepare_stored_core_policy_patch,
};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
};

const OLD: &[u8] = b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x01\
    \x74\0\0\0\x02\x77\x06states\x74\0\0\0\x01\x6d\0\0\0\x01q\
    \x74\0\0\0\x01\x77\x04mode\x77\x08parallel\x77\x04type\x6d\0\0\0\x03job";
const NEW: &[u8] = b"\x83\x68\x03\x77\x0eflow_policy_v1\x61\x02\
    \x74\0\0\0\x02\x77\x06states\x74\0\0\0\x01\x6d\0\0\0\x01q\
    \x74\0\0\0\x01\x77\x04mode\x77\x04fifo\x77\x04type\x6d\0\0\0\x03job";

fn raw_input() -> CompleteCorePolicyPatchInput {
    CompleteCorePolicyPatchInput {
        workflow_type: b"job".to_vec(),
        expected_generation: Some(1),
        patch: PolicyPatch {
            states: BTreeMap::from([(
                b"q".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    }
}

fn raw_kernel(
    old: &StoredPolicyValue,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    live: usize,
) -> Result<StoredPolicyValue, Error> {
    prepare_stored_core_policy_patch(Some(old), 0, raw_input(), 64, schema, charge, live)
}

fn raw_result(value: &StoredPolicyValue, expected: &[u8]) -> TestResult {
    let StoredPolicyValue::Original(raw) = value else {
        return Err("raw patch changed format".into());
    };
    assert_eq!(raw, expected);
    Ok(())
}

pub(super) fn assert_raw_accounting() -> TestResult {
    // ARCH RAW-STAGES1 + independent PERF8a107e33, before implementation.
    assert_eq!((OLD.len(), NEW.len()), (80, 76));
    let schema = limits(226_821)?;
    let old = StoredPolicyValue::Original(OLD.to_vec());
    let held = vec![0x5a; 64];
    let mut charge = PolicySchemaCharge::new(schema);
    prime(schema, &mut charge)?;
    assert_eq!(
        original_core_policy_patch_bytes(OLD, &raw_input(), schema, &mut charge, held.len())?,
        (84, 14_368)
    );
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (95_724, 1_088, 96_812)
    );
    raw_result(&raw_kernel(&old, schema, &mut charge, held.len())?, NEW)?;
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (210_777, 16_044, 226_821)
    );
    raw_result(&old, OLD)?;
    assert_eq!(held, [0x5a; 64]);
    for cap in [
        47_349, 47_350, 112_460, 112_461, 131_208, 131_209, 133_024, 133_025,
    ] {
        raw_frontier(cap)?;
    }
    // Measuring at its exact cap allocates neither output nor J; no credit escapes.
    let exact = limits(96_812)?;
    let mut charge = PolicySchemaCharge::new(exact);
    prime(exact, &mut charge)?;
    assert_eq!(
        original_core_policy_patch_bytes(OLD, &raw_input(), exact, &mut charge, 64)?,
        (84, 14_368)
    );
    assert_eq!((charge.g(), charge.p()), (95_724, 1_088));
    Ok(())
}

fn raw_frontier(maximum: u64) -> TestResult {
    let schema = limits(maximum)?;
    let mut charge = PolicySchemaCharge::new(schema);
    prime(schema, &mut charge)?;
    let old = StoredPolicyValue::Original(OLD.to_vec());
    let held = vec![0x5a; 64];
    let result = raw_kernel(&old, schema, &mut charge, held.len());
    if maximum == 133_025 {
        raw_result(&result?, NEW)?;
        assert_eq!((charge.g(), charge.p()), (116_981, 16_044));
    } else {
        let (needed, g, p) = match maximum {
            47_349 => (47_350, 46_262, 64),
            47_350 => (47_862, 46_262, 1_088),
            112_460 => (112_461, 97_005, 1_088),
            112_461 => (123_341, 97_005, 15_456),
            131_208 => (131_209, 115_165, 15_456),
            131_209 => (131_413, 115_165, 16_044),
            133_024 => (133_025, 116_845, 16_044),
            _ => return Err("unselected raw frontier".into()),
        };
        assert!(
            matches!(result, Err(Error::Schema(PolicySchemaError::WorkBytes {bytes, maximum: cap}))
            if bytes == needed && cap == maximum)
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
    }
    raw_result(&old, OLD)?;
    assert_eq!(held, [0x5a; 64]);
    Ok(())
}

pub(super) fn prime(profile: PolicySchemaLimits, charge: &mut PolicySchemaCharge) -> TestResult {
    let mut value = data(b"r".to_vec(), None, BTreeMap::new());
    value.indexed_attributes = Some(vec![b"a".to_vec()]);
    drop(candidate(1, value, profile, charge)?);
    assert_eq!((charge.g(), charge.p()), (1_928, 64));
    Ok(())
}
pub(super) fn assert_frontier(maximum: u64, output: bool) -> TestResult {
    let profile = limits(maximum)?;
    let value = f::minimal(profile)?;
    let held = vec![0x5a; 64];
    let mut charge = PolicySchemaCharge::new(profile);
    prime(profile, &mut charge)?;
    let result = encode(&value, profile, &mut charge, held.len(), 51);
    if output {
        assert_eq!(result?, f::minimal_wire());
        assert_eq!((charge.g(), charge.p()), (17_330, 9_491));
    } else {
        let (needed, g, p) = match maximum {
            14_859 => (14_860, 5_932, 64),
            14_860 => (21_644, 5_932, 8_928),
            25_566 => (25_567, 16_076, 8_928),
            25_567 => (25_746, 16_076, 9_491),
            26_820 => (26_821, 17_196, 9_491),
            _ => return Err("unselected frontier".into()),
        };
        assert!(
            matches!(result,Err(Error::Schema(PolicySchemaError::WorkBytes {bytes,maximum:cap}))
            if bytes==needed && cap==maximum)
        );
        assert_eq!((charge.g(), charge.p()), (g, p));
    }
    assert_eq!(held, [0x5a; 64]);
    Ok(())
}
