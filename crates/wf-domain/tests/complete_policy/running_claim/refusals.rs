use super::fixtures::{atom, base_rule, binary, envelope, map, running, typed};
use super::{BTreeMap, PolicySchemaCharge, StoredPolicyValue, TestResult, data, limits, resolve};
use wf_domain::policy::complete::{CompletePolicyEncodingError as Error, PolicySchemaError};

#[test]
fn effective_scalar_limit_group_refuses_instead_of_reporting_ungoverned() -> TestResult {
    let schema = limits(128 * 1024 * 1024)?;
    let local = running(&[(atom("limit")?, vec![97, 2])])?;
    let stored = StoredPolicyValue::Original(envelope(
        1,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, map(&[(atom("limits")?, vec![97, 4])])?),
            (
                atom("states")?,
                map(&[(binary(b"a")?, map(&[(atom("governance")?, local)])?)])?,
            ),
        ],
    )?);
    let names: [&[u8]; 1] = [b"a"];
    let selected = resolve(
        &stored,
        b"job",
        Some(&names),
        4,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?
    .ok_or("override")?;
    assert_eq!(selected.limit(), 2);
    assert!(matches!(
        resolve(
            &stored,
            b"job",
            None,
            4,
            schema,
            &mut PolicySchemaCharge::new(schema)
        ),
        Err(Error::UnsupportedPolicyProfile)
    ));
    Ok(())
}

#[test]
fn complete_type_corruption_and_empty_filter_never_skip_backing_validation() -> TestResult {
    let schema = limits(128 * 1024 * 1024)?;
    let raw = envelope(
        0,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, base_rule()?),
        ],
    )?;
    let stored = StoredPolicyValue::Original(raw.clone());
    assert!(matches!(
        resolve(
            &stored,
            b"wrong",
            Some(&[]),
            4,
            schema,
            &mut PolicySchemaCharge::new(schema)
        ),
        Err(Error::Schema(PolicySchemaError::InvalidAuthority))
    ));
    for cut in [raw.len() - 1, raw.len() - 2] {
        let bad = StoredPolicyValue::Original(raw[..cut].to_vec());
        assert!(matches!(
            resolve(
                &bad,
                b"job",
                Some(&[]),
                4,
                schema,
                &mut PolicySchemaCharge::new(schema)
            ),
            Err(Error::Malformed)
        ));
    }
    let value = data(b"job".to_vec(), None, BTreeMap::new());
    let candidate =
        super::super::candidate(1, value, schema, &mut PolicySchemaCharge::new(schema))?;
    let fake = super::super::bind(
        candidate,
        vec![0x5a; 17],
        2..15,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?;
    let fake = StoredPolicyValue::Complete(Box::new(fake));
    assert!(
        resolve(
            &fake,
            b"job",
            Some(&[]),
            4,
            schema,
            &mut PolicySchemaCharge::new(schema)
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn reached_invalid_rule_refuses_but_overridden_or_inactive_fields_do_not() -> TestResult {
    let schema = limits(128 * 1024 * 1024)?;
    let base = running(&[
        (atom("limit")?, vec![97, 8]),
        (atom("enforcement")?, atom("nil")?),
    ])?;
    let local = running(&[(atom("enforcement")?, atom("strict_global")?)])?;
    let stored = StoredPolicyValue::Original(envelope(
        1,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, base),
            (
                atom("states")?,
                map(&[(binary(b"a")?, map(&[(atom("governance")?, local)])?)])?,
            ),
        ],
    )?);
    let names: [&[u8]; 1] = [b"a"];
    assert!(
        resolve(
            &stored,
            b"job",
            Some(&names),
            4,
            schema,
            &mut PolicySchemaCharge::new(schema)
        )?
        .is_some()
    );
    assert!(matches!(
        resolve(
            &stored,
            b"job",
            None,
            4,
            schema,
            &mut PolicySchemaCharge::new(schema)
        ),
        Err(Error::UnsupportedPolicyProfile)
    ));
    for (limit, valid) in [(atom("nil")?, true), (vec![97, 8], false)] {
        let governance = running(&[(atom("limit")?, limit), (atom("lease_size")?, vec![97, 0])])?;
        let value = StoredPolicyValue::Original(envelope(
            1,
            &[
                (atom("type")?, binary(b"job")?),
                (atom("governance")?, governance),
            ],
        )?);
        let result = resolve(
            &value,
            b"job",
            None,
            4,
            schema,
            &mut PolicySchemaCharge::new(schema),
        );
        assert_eq!(result.is_ok(), valid);
        if valid {
            assert!(result?.is_none());
        }
    }
    Ok(())
}

#[test]
fn one_schema_account_has_exact_peak_bound_and_never_resets_between_calls() -> TestResult {
    let stored = typed(data(b"job".to_vec(), None, BTreeMap::new()), 1)?;
    let schema = limits(128 * 1024 * 1024)?;
    let mut measured = PolicySchemaCharge::new(schema);
    assert!(resolve(&stored, b"job", None, 4, schema, &mut measured)?.is_none());
    let needed = measured.total();
    assert!(needed > 0);
    let exact = limits(needed)?;
    let mut continuous = PolicySchemaCharge::new(exact);
    assert!(resolve(&stored, b"job", None, 4, exact, &mut continuous)?.is_none());
    assert_eq!(continuous.total(), needed);
    assert!(matches!(
        resolve(&stored, b"job", None, 4, exact, &mut continuous),
        Err(Error::Schema(PolicySchemaError::WorkBytes { .. }))
    ));
    assert_eq!(continuous.total(), needed);
    let low = limits(needed - 1)?;
    assert!(matches!(
        resolve(
            &stored,
            b"job",
            None,
            4,
            low,
            &mut PolicySchemaCharge::new(low)
        ),
        Err(Error::Schema(PolicySchemaError::WorkBytes { .. }))
    ));
    let mut mismatch = PolicySchemaCharge::new(low);
    assert!(matches!(
        resolve(&stored, b"job", None, 4, schema, &mut mismatch),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    ));
    assert_eq!(mismatch.total(), 0);
    Ok(())
}
