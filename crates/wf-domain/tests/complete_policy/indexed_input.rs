//! Original indexed-policy input classes; no public parser or storage proof.

use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyPresentation, PolicyIndexedInputRejection as Reason,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, RawPolicyIndexedValue as Raw,
    normalize_complete_policy_indexed_input, prepare_complete_policy_value_v2,
    present_complete_policy,
};
use wf_domain::retention::ResolvedRetention;

use super::{TestResult, candidate, data, limits};

mod refusals;

type IndexedOwners = (Option<Vec<Vec<u8>>>, Option<Vec<u8>>);

fn normalize(
    attributes: Option<Raw<'_>>,
    meta: Option<Raw<'_>>,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<IndexedOwners> {
    Ok(normalize_complete_policy_indexed_input(
        attributes, meta, profile, charge,
    )??)
}

fn names() -> [Raw<'static>; 4] {
    [
        Raw::Binary(b"tenant"),
        Raw::Atom("region"),
        Raw::Binary(b"campaign"),
        Raw::Binary(b"tenant"),
    ]
}

#[test]
fn indexed_input_normalizes_frozen_original_policy_names_and_presence() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let input = names();
    let expected = vec![b"tenant".to_vec(), b"region".to_vec(), b"campaign".to_vec()];
    let (attributes, meta) = normalize(Some(Raw::List(&input)), None, profile, &mut charge)?;
    assert_eq!(attributes, Some(expected.clone()));
    assert_eq!(meta, None);
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (4_784, 192, 4_976)
    );
    assert_eq!(input, names());
    let mut policy = data(b"indexed".to_vec(), None, BTreeMap::new());
    policy.indexed_attributes = attributes;
    let value = prepare_complete_policy_value_v2(
        candidate(1, policy, profile, &mut charge)?,
        profile,
        &mut charge,
    )?;
    for state in [None, Some(b"queued".as_slice())] {
        let presented = present_complete_policy(
            b"indexed",
            Some(&value),
            state,
            ResolvedRetention::default(),
            profile,
            &mut charge,
        )?;
        match presented {
            CompletePolicyPresentation::All {
                indexed_attributes,
                indexed_state_meta,
                ..
            }
            | CompletePolicyPresentation::State {
                indexed_attributes,
                indexed_state_meta,
                ..
            } => {
                assert_eq!(indexed_attributes, expected);
                assert_eq!(indexed_state_meta, None);
            }
        }
    }
    for attributes in [
        None,
        Some(Raw::Nil),
        Some(Raw::Atom("nil")),
        Some(Raw::List(&[])),
    ] {
        for meta in [
            None,
            Some(Raw::Nil),
            Some(Raw::Atom("")),
            Some(Raw::Binary(b"")),
            Some(Raw::List(&[])),
        ] {
            let result = normalize(attributes, meta, profile, &mut charge)?;
            assert_eq!(result.0, attributes.map(|_| Vec::new()));
            assert_eq!(result.1, None);
        }
    }
    let owned = {
        let source = b" tenant ".to_vec();
        let tree = [Raw::Binary(&source)];
        let result = normalize(
            Some(Raw::List(&tree)),
            Some(Raw::Atom("owner")),
            profile,
            &mut charge,
        )?;
        assert_eq!(
            result,
            (Some(vec![b"tenant".to_vec()]), Some(b"owner".to_vec()))
        );
        assert_eq!(source, b" tenant ");
        assert_ne!(
            result.0.as_ref().ok_or("names")?[0].as_ptr(),
            source.as_ptr()
        );
        result
    };
    assert_eq!(
        owned,
        (Some(vec![b"tenant".to_vec()]), Some(b"owner".to_vec()))
    );
    Ok(())
}

#[test]
fn indexed_input_trims_only_source_whitespace_before_bounds_and_deduplication() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for ch in [
        '\u{0009}', '\u{000a}', '\u{000b}', '\u{000c}', '\u{000d}', '\u{0020}', '\u{0085}',
        '\u{00a0}', '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}',
        '\u{2005}', '\u{2006}', '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2028}',
        '\u{2029}', '\u{202f}', '\u{205f}', '\u{3000}',
    ] {
        let padded = format!("{ch}key{ch}");
        let input = [Raw::Binary(padded.as_bytes()), Raw::Atom("key")];
        let result = normalize(
            Some(Raw::List(&input)),
            Some(Raw::Binary(padded.as_bytes())),
            profile,
            &mut charge,
        )?;
        assert_eq!(result, (Some(vec![b"key".to_vec()]), Some(b"key".to_vec())));
        assert_eq!(padded, format!("{ch}key{ch}"));
    }
    for ch in ['\u{180e}', '\u{200b}', '\u{feff}'] {
        let retained = format!("{ch}key{ch}");
        let input = [Raw::Binary(retained.as_bytes())];
        let result = normalize(
            Some(Raw::List(&input)),
            Some(Raw::Binary(retained.as_bytes())),
            profile,
            &mut charge,
        )?;
        assert_eq!(
            result,
            (
                Some(vec![retained.as_bytes().to_vec()]),
                Some(retained.as_bytes().to_vec())
            )
        );
    }
    let ordered = [
        Raw::Binary(b" z "),
        Raw::Atom("a"),
        Raw::Binary(b"z"),
        Raw::Binary(b" a "),
    ];
    assert_eq!(
        normalize(Some(Raw::List(&ordered)), None, profile, &mut charge)?.0,
        Some(vec![b"z".to_vec(), b"a".to_vec()])
    );
    for length in [64, 65] {
        let bytes = vec![b'x'; length];
        let padded = [b"  ".as_slice(), bytes.as_slice(), b"  "].concat();
        let input = [Raw::Binary(&padded)];
        if length == 64 {
            assert_eq!(
                normalize(
                    Some(Raw::List(&input)),
                    Some(Raw::Binary(&padded)),
                    profile,
                    &mut charge
                )?,
                (Some(vec![bytes.clone()]), Some(bytes))
            );
        } else {
            assert_reason(
                Some(Raw::List(&input)),
                None,
                Reason::AttributeBytes,
                b"ERR flow attribute key too large",
                profile,
                &mut charge,
            )?;
            assert_reason(
                None,
                Some(Raw::Binary(&padded)),
                Reason::StateMetaBytes,
                b"ERR flow state_meta key too large",
                profile,
                &mut charge,
            )?;
        }
    }
    let multibyte = "é".repeat(33);
    assert_reason(
        None,
        Some(Raw::Binary(multibyte.as_bytes())),
        Reason::StateMetaBytes,
        b"ERR flow state_meta key too large",
        profile,
        &mut charge,
    )?;
    Ok(())
}

#[test]
fn indexed_input_preserves_nested_meta_local_priority_and_nil_atom_semantics() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for (raw, expected) in [
        (Raw::Binary(b"version"), Some(b"version".to_vec())),
        (Raw::Binary(b"owner"), Some(b"owner".to_vec())),
        (Raw::Nil, None),
    ] {
        let result = normalize(None, Some(raw), profile, &mut charge)?;
        assert_eq!(result, (None, expected));
    }
    let nils = [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Atom(""),
        Raw::List(&[]),
        Raw::Binary(b""),
    ];
    let nested = [
        Raw::List(&nils),
        Raw::Atom("version"),
        Raw::Binary(b" version "),
    ];
    assert_eq!(
        normalize(None, Some(Raw::List(&nested)), profile, &mut charge)?.1,
        Some(b"version".to_vec())
    );
    assert_eq!(
        normalize(None, Some(Raw::List(&nils)), profile, &mut charge)?.1,
        None
    );
    assert_eq!(
        normalize(None, Some(Raw::Atom(" nil ")), profile, &mut charge)?.1,
        Some(b"nil".to_vec())
    );
    let attributes = [Raw::Nil, Raw::Atom("nil"), Raw::Binary(b"nil")];
    assert_eq!(
        normalize(
            Some(Raw::List(&attributes)),
            Some(Raw::Binary(b"nil")),
            profile,
            &mut charge
        )?,
        (Some(vec![b"nil".to_vec()]), Some(b"nil".to_vec()))
    );
    let inner = [Raw::Binary(b"version"), Raw::Binary(b"owner")];
    let outer = [Raw::List(&inner), Raw::Other];
    assert_reason(
        None,
        Some(Raw::List(&inner)),
        Reason::StateMetaCount,
        b"ERR flow indexed_state_meta supports at most 1 key",
        profile,
        &mut charge,
    )?;
    assert_reason(
        None,
        Some(Raw::List(&outer)),
        Reason::StateMetaCount,
        b"ERR flow indexed_state_meta supports at most 1 key",
        profile,
        &mut charge,
    )?;
    let flat = [Raw::Binary(b"version"), Raw::Binary(b"owner"), Raw::Other];
    assert_reason(
        None,
        Some(Raw::List(&flat)),
        Reason::StateMetaShape,
        b"ERR flow indexed_state_meta must be a string or one-item list",
        profile,
        &mut charge,
    )?;
    let late = [
        Raw::Binary(b"a"),
        Raw::Binary(b"b"),
        Raw::Binary(b"c"),
        Raw::Binary(b"d"),
        Raw::Binary(&[255]),
    ];
    assert_reason(
        Some(Raw::List(&late)),
        None,
        Reason::AttributeUtf8,
        b"ERR flow attribute key must be valid UTF-8",
        profile,
        &mut charge,
    )?;
    Ok(())
}

#[test]
fn indexed_input_preserves_all_source_diagnostics_and_untouched_inputs() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let oversized = [b'x'; 65];
    let four = [
        Raw::Binary(b"a"),
        Raw::Binary(b"b"),
        Raw::Binary(b"c"),
        Raw::Binary(b"d"),
    ];
    let reason = b"ERR flow indexed_attributes supports at most 3 keys";
    assert_reason(
        Some(Raw::List(&four)),
        Some(Raw::Other),
        Reason::AttributesCount,
        reason,
        profile,
        &mut charge,
    )?;
    let actual = normalize_complete_policy_indexed_input(
        Some(Raw::List(&four)),
        None,
        profile,
        &mut charge,
    )?
    .err()
    .ok_or("attributes count")?;
    assert_eq!(actual, Reason::AttributesCount);
    assert_eq!(
        actual.message(),
        b"ERR flow indexed_attributes supports at most 3 keys"
    );
    assert_reason(
        Some(Raw::Binary(b"key")),
        None,
        Reason::AttributesList,
        b"ERR flow indexed_attributes must be a list",
        profile,
        &mut charge,
    )?;
    assert_reason(
        Some(Raw::Atom("")),
        None,
        Reason::AttributesList,
        b"ERR flow indexed_attributes must be a list",
        profile,
        &mut charge,
    )?;
    assert_attribute_reasons(&oversized, profile, &mut charge)?;
    assert_meta_reasons(&oversized, profile, &mut charge)?;
    let two = [Raw::Binary(b"version"), Raw::Binary(b"owner")];
    assert_reason(
        None,
        Some(Raw::List(&two)),
        Reason::StateMetaCount,
        b"ERR flow indexed_state_meta supports at most 1 key",
        profile,
        &mut charge,
    )?;
    let actual =
        normalize_complete_policy_indexed_input(None, Some(Raw::List(&two)), profile, &mut charge)?
            .err()
            .ok_or("metadata count")?;
    assert_eq!(actual, Reason::StateMetaCount);
    assert_eq!(
        actual.message(),
        b"ERR flow indexed_state_meta supports at most 1 key"
    );
    let invalid_large = [255; 65];
    assert_reason(
        None,
        Some(Raw::Binary(&invalid_large)),
        Reason::StateMetaUtf8,
        b"ERR flow state_meta key must be valid UTF-8",
        profile,
        &mut charge,
    )?;
    let reserved_large = [b"__".as_slice(), &oversized].concat();
    assert_reason(
        None,
        Some(Raw::Binary(&reserved_large)),
        Reason::StateMetaBytes,
        b"ERR flow state_meta key too large",
        profile,
        &mut charge,
    )?;
    assert_eq!(
        normalize(None, Some(Raw::Atom("owner")), profile, &mut charge)?.1,
        Some(b"owner".to_vec())
    );
    Ok(())
}

fn assert_meta_reasons(
    oversized: &[u8],
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (raw, reason, message) in [
        (
            Raw::Other,
            Reason::StateMetaShape,
            b"ERR flow indexed_state_meta must be a string or one-item list".as_slice(),
        ),
        (
            Raw::Binary(&[255]),
            Reason::StateMetaUtf8,
            b"ERR flow state_meta key must be valid UTF-8",
        ),
        (
            Raw::Binary(b" \t"),
            Reason::StateMetaEmpty,
            b"ERR flow state_meta key must not be empty",
        ),
        (
            Raw::Binary(oversized),
            Reason::StateMetaBytes,
            b"ERR flow state_meta key too large",
        ),
        (
            Raw::Binary(b" __key "),
            Reason::StateMetaReserved,
            b"ERR flow state_meta key is reserved",
        ),
    ] {
        assert_reason(None, Some(raw), reason, message, profile, charge)?;
    }
    Ok(())
}

fn assert_attribute_reasons(
    oversized: &[u8],
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (raw, reason, message) in [
        (
            Raw::Other,
            Reason::AttributeString,
            b"ERR flow attribute key must be a string".as_slice(),
        ),
        (
            Raw::Binary(&[255]),
            Reason::AttributeUtf8,
            b"ERR flow attribute key must be valid UTF-8",
        ),
        (
            Raw::Binary(b" \t"),
            Reason::AttributeEmpty,
            b"ERR flow attribute key must not be empty",
        ),
        (
            Raw::Atom(""),
            Reason::AttributeEmpty,
            b"ERR flow attribute key must not be empty",
        ),
        (
            Raw::Binary(oversized),
            Reason::AttributeBytes,
            b"ERR flow attribute key too large",
        ),
        (
            Raw::Binary(b" __key "),
            Reason::AttributeReserved,
            b"ERR flow attribute key is reserved",
        ),
    ] {
        let list = [raw];
        assert_reason(
            Some(Raw::List(&list)),
            Some(Raw::Other),
            reason,
            message,
            profile,
            charge,
        )?;
        assert_eq!(list, [raw]);
    }
    let inner = [Raw::Binary(b"version"), Raw::Binary(b"owner")];
    let leaf_list = [Raw::List(&inner)];
    assert_reason(
        Some(Raw::List(&leaf_list)),
        None,
        Reason::AttributeString,
        b"ERR flow attribute key must be a string",
        profile,
        charge,
    )?;
    Ok(())
}

fn assert_reason(
    attributes: Option<Raw<'_>>,
    meta: Option<Raw<'_>>,
    reason: Reason,
    expected: &[u8],
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let result = normalize_complete_policy_indexed_input(attributes, meta, profile, charge)?;
    let actual = result
        .err()
        .ok_or("expected source normalization rejection")?;
    assert_eq!(actual, reason);
    assert_eq!(actual.message(), expected);
    assert_eq!(actual.to_string().as_bytes(), expected);
    Ok(())
}
