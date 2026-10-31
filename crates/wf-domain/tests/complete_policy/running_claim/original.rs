use super::fixtures::{atom, base_rule, binary, envelope, map, running};
use super::{Enforcement, PolicySchemaCharge, StoredPolicyValue, TestResult, limits, resolve};
use wf_domain::policy::complete::RunningClaimPolicyVersion as Version;

#[test]
fn raw_binary_lookups_handle_mixed_keys_and_partial_bitstring_neighbors() -> TestResult {
    let mut before = vec![77, 0, 0, 0, 7, 7];
    before.extend_from_slice(b"runninf");
    let mut after = vec![77, 0, 0, 0, 7, 7];
    after.extend_from_slice(b"runninh");
    let rules = map(&[
        (atom("nil")?, vec![97, 1]),
        (vec![97, 0], vec![97, 1]),
        (vec![104, 0], vec![97, 1]),
        (vec![106], vec![97, 1]),
        (before, vec![97, 1]),
        (after, vec![97, 1]),
        (binary(b"running")?, map(&[(atom("limit")?, vec![97, 6])])?),
    ])?;
    let stored = StoredPolicyValue::Original(envelope(
        1,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, map(&[(atom("limits")?, rules)])?),
        ],
    )?);
    let schema = limits(128 * 1024 * 1024)?;
    let rule = resolve(
        &stored,
        b"job",
        None,
        4,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?
    .ok_or("mixed rule")?;
    assert_eq!((rule.limit(), rule.lease_size()), (6, 4));
    Ok(())
}

#[test]
fn raw_claim_version_never_falls_back_to_nested_governance_version() -> TestResult {
    let governance = map(&[
        (atom("version")?, binary(b"nested")?),
        (
            atom("limits")?,
            map(&[(binary(b"running")?, map(&[(atom("limit")?, vec![97, 6])])?)])?,
        ),
    ])?;
    let stored = StoredPolicyValue::Original(envelope(
        0,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, governance),
        ],
    )?);
    let schema = limits(128 * 1024 * 1024)?;
    let rule = resolve(
        &stored,
        b"job",
        None,
        4,
        schema,
        &mut PolicySchemaCharge::new(schema),
    )?
    .ok_or("rule")?;
    assert!(rule.config_version().is_none());
    assert!(rule.policy_version().is_none());
    Ok(())
}

#[test]
fn raw_complete_rule_merges_before_aggregation_and_borrows_original_label() -> TestResult {
    let a = running(&[
        (atom("limit")?, vec![97, 0]),
        (atom("enforcement")?, atom("strict_global")?),
    ])?;
    let b = running(&[
        (atom("limit")?, vec![97, 3]),
        (atom("lease_size")?, vec![97, 2]),
    ])?;
    let states = map(&[
        (binary(b"a")?, map(&[(atom("governance")?, a)])?),
        (binary(b"b")?, map(&[(atom("governance")?, b)])?),
    ])?;
    let stored = StoredPolicyValue::Original(envelope(
        9,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("version")?, binary(b"v\0x")?),
            (atom("governance")?, base_rule()?),
            (atom("states")?, states),
        ],
    )?);
    let saved = stored.encoded().to_vec();
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let rule = resolve(&stored, b"job", None, 4, schema, &mut charge)?.ok_or("rule")?;
    assert_eq!(
        (rule.limit(), rule.lease_size(), rule.config_version()),
        (0, 2, Some(9))
    );
    assert!(rule.enforcement() == Enforcement::StrictGlobal);
    let Some(Version::Bytes(label)) = rule.policy_version() else {
        return Err("label".into());
    };
    assert_eq!(label, b"v\0x");
    let offset = saved
        .windows(3)
        .position(|w| w == b"v\0x")
        .ok_or("label offset")?;
    assert_eq!(label.as_ptr(), stored.encoded()[offset..].as_ptr());
    let only: [&[u8]; 1] = [b"b"];
    let selected =
        resolve(&stored, b"job", Some(&only), 4, schema, &mut charge)?.ok_or("selected")?;
    assert_eq!((selected.limit(), selected.lease_size()), (3, 2));
    assert!(selected.enforcement() == Enforcement::ApproximateGlobal);
    assert_eq!(stored.encoded(), saved);
    Ok(())
}

#[test]
fn raw_presence_masks_rules_while_empty_maps_and_nonmap_governance_inherit() -> TestResult {
    let nil = atom("nil")?;
    let cases = [
        (
            b"limits_nil".as_slice(),
            map(&[(atom("limits")?, nil.clone())])?,
            false,
        ),
        (
            b"rule_nil",
            map(&[(atom("limits")?, map(&[(binary(b"running")?, nil.clone())])?)])?,
            false,
        ),
        (
            b"limit_nil",
            running(&[(atom("limit")?, nil.clone())])?,
            false,
        ),
        (b"empty", map(&[])?, true),
        (b"scalar", vec![97, 7], true),
        (b"nil", nil, true),
        (b"empty_rule", running(&[])?, true),
    ];
    let mut rows = Vec::new();
    for (name, governance, _) in &cases {
        rows.push((
            binary(name)?,
            map(&[(atom("governance")?, governance.clone())])?,
        ));
    }
    let stored = StoredPolicyValue::Original(envelope(
        0,
        &[
            (atom("type")?, binary(b"job")?),
            (atom("version")?, vec![97, 7]),
            (atom("governance")?, base_rule()?),
            (atom("states")?, map(&rows)?),
        ],
    )?);
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    for (name, _, expected) in &cases {
        let names = [*name];
        let rule = resolve(&stored, b"job", Some(&names), 4, schema, &mut charge)?;
        assert_eq!(rule.is_some(), *expected, "state {name:?}");
        if let Some(rule) = rule {
            assert_eq!(
                (rule.limit(), rule.lease_size(), rule.config_version()),
                (8, 7, Some(7))
            );
        }
    }
    Ok(())
}

#[test]
fn raw_top_level_fallback_and_mixed_map_key_search_keep_source_fields() -> TestResult {
    let running = map(&[(
        binary(b"running")?,
        map(&[(vec![97, 0], vec![97, 99]), (atom("limit")?, vec![97, 5])])?,
    )])?;
    let stored = StoredPolicyValue::Original(envelope(
        0,
        &[
            (vec![97, 0], vec![97, 1]),
            (atom("type")?, binary(b"job")?),
            (atom("governance")?, atom("nil")?),
            (atom("limits")?, running),
        ],
    )?);
    let schema = limits(128 * 1024 * 1024)?;
    let mut charge = PolicySchemaCharge::new(schema);
    let rule = resolve(&stored, b"job", None, 4, schema, &mut charge)?.ok_or("root fallback")?;
    assert_eq!(
        (rule.limit(), rule.lease_size(), rule.config_version()),
        (5, 4, None)
    );
    assert!(rule.enforcement() == Enforcement::StrictGlobal);
    Ok(())
}

#[test]
fn raw_legacy_versions_keep_generation_precedence_without_nested_fallback() -> TestResult {
    let mut huge = vec![110, 17, 0, 1];
    huge.extend_from_slice(&[0; 15]);
    huge.push(1);
    for (version, config, expected) in [
        (vec![97, 0], Some(0), Some(Version::Number(0))),
        (binary(b"v")?, None, Some(Version::Bytes(b"v"))),
        (binary(b"")?, None, None),
    ] {
        for generation in [0, 9] {
            let stored = StoredPolicyValue::Original(envelope(
                generation,
                &[
                    (atom("type")?, binary(b"job")?),
                    (atom("version")?, version.clone()),
                    (atom("governance")?, base_rule()?),
                ],
            )?);
            let schema = limits(128 * 1024 * 1024)?;
            let rule = resolve(
                &stored,
                b"job",
                None,
                4,
                schema,
                &mut PolicySchemaCharge::new(schema),
            )?
            .ok_or("version rule")?;
            assert_eq!(
                rule.config_version(),
                if generation == 0 { config } else { Some(9) }
            );
            assert_eq!(rule.policy_version(), expected);
        }
    }
    let schema = limits(128 * 1024 * 1024)?;
    for generation in [0, 9] {
        let stored = StoredPolicyValue::Original(envelope(
            generation,
            &[
                (atom("type")?, binary(b"job")?),
                (atom("version")?, huge.clone()),
                (atom("governance")?, base_rule()?),
            ],
        )?);
        let result = resolve(
            &stored,
            b"job",
            None,
            4,
            schema,
            &mut PolicySchemaCharge::new(schema),
        );
        if generation == 0 {
            // Source forwards this huge numeric configuration; this bounded
            // profile must refuse rather than manufacture an absent version.
            assert!(matches!(result, Err(wf_domain::policy::complete::CompletePolicyEncodingError::UnsupportedPolicyProfile)));
        } else {
            let rule = result?.ok_or("generation rule")?;
            assert_eq!(rule.config_version(), Some(9));
            assert!(rule.policy_version().is_none());
        }
    }
    Ok(())
}
