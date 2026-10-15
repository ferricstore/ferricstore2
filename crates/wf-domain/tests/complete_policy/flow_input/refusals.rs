//! Independent prospective event streams, visible caps and nonrefunding owners.

use super::{Error, Gov, PolicySchemaCharge, PolicySchemaError, Raw, Reason, Retention};
use super::{TestResult, encode, limits, normalize, snapshot};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    GovernanceField as Field, RawPolicyIndexedValue as IndexedRaw,
    normalize_complete_policy_indexed_input, restore_complete_policy,
};

#[test]
fn original_flow_input_preserves_profile_and_visible_bounds() -> TestResult {
    let profile = limits(128 * 1_024 * 1_024)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let other = limits(128 * 1_024 * 1_024 + 1)?;
    assert_eq!(
        normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, other, &mut charge),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!(
        encode(Reason::RootShape, other, &mut charge),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for maximum in [0, 1_000_001, u32::MAX] {
        for input in [Raw::Nil, Raw::Atom("nil"), Raw::Other] {
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                normalize(&Raw::Other, &input, maximum, profile, &mut charge),
                Err(Error::Schema(PolicySchemaError::InvalidAuthority))
            );
            assert_eq!((charge.g(), charge.p()), (128, 0));
        }
    }
    assert_depth(Raw::Other, 64, 64)?;
    assert_depth(Raw::Other, 65, 65)?;
    assert_node_occurrences()?;
    assert_visible_bytes()?;
    assert_queried_key_priority()?;
    Ok(())
}

fn assert_depth(input: Raw<'_>, remaining: usize, requested: usize) -> TestResult {
    if remaining != 0 {
        let child = [input];
        return assert_depth(Raw::List(&child), remaining - 1, requested);
    }
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let actual = normalize(&Raw::Binary(b"r"), &input, 17, profile, &mut charge);
    if requested == 64 {
        assert_eq!(actual?, Err(Reason::RootKeyword));
        assert_eq!(charge.p(), 51_200);
    } else {
        assert_eq!(actual, Err(Error::ItemLimit));
        assert_eq!((charge.g(), charge.p()), (8_705, 0));
    }
    Ok(())
}

fn assert_node_occurrences() -> TestResult {
    let profile = limits(256 * 1_024 * 1_024)?;
    let shared = [(Raw::Other, Raw::Nil)];
    for aliases in [false, true] {
        let count = if aliases { 16_383 } else { 32_767 };
        let leaf = if aliases { Raw::Map(&shared) } else { Raw::Nil };
        let mut rows = vec![(Raw::Other, leaf); count];
        if aliases {
            rows.push((Raw::Other, Raw::Nil));
        }
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize(
                &Raw::Binary(b"r"),
                &Raw::Map(&rows),
                17,
                profile,
                &mut charge
            )?,
            Ok(snapshot(b"r", &[])?)
        );
        rows.push((Raw::Other, Raw::Nil));
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize(
                &Raw::Binary(b"r"),
                &Raw::Map(&rows),
                17,
                profile,
                &mut charge
            ),
            Err(Error::ItemLimit)
        );
        assert_eq!((charge.g(), charge.p()), (8_388_865, 0));
        assert!(matches!(shared, [(Raw::Other, Raw::Nil)]));
    }
    Ok(())
}

fn assert_visible_bytes() -> TestResult {
    let profile = limits(256 * 1_024 * 1_024)?;
    for width in [64 * 1_024 * 1_024 - 1, 64 * 1_024 * 1_024] {
        let bytes = vec![0; width];
        let mut charge = PolicySchemaCharge::new(profile);
        let actual = normalize(
            &Raw::Binary(b"r"),
            &Raw::Binary(&bytes),
            17,
            profile,
            &mut charge,
        );
        if width == 64 * 1_024 * 1_024 - 1 {
            assert_eq!(actual?, Err(Reason::RootShape));
        } else {
            assert_eq!(actual, Err(Error::ByteLimit));
            assert_eq!((charge.g(), charge.p()), (385, 0));
        }
        assert!(bytes.iter().all(|&byte| byte == 0));
    }
    let too_wide = vec![0; 64 * 1_024 * 1_024 + 1];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Other,
            &Raw::Binary(&too_wide),
            17,
            profile,
            &mut charge
        ),
        Err(Error::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (384, 0));
    Ok(())
}

fn assert_queried_key_priority() -> TestResult {
    let huge = vec![0; 64 * 1_024 * 1_024 + 1];
    let duplicate = [
        (Raw::Atom("mode"), Raw::Nil),
        (Raw::Atom("mode"), Raw::Binary(&huge)),
    ];
    for maximum in [1_304, 1_305] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let actual = normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&duplicate),
            17,
            profile,
            &mut charge,
        );
        assert_eq!(
            actual,
            Err(if maximum == 1_304 {
                work(1_305, maximum)
            } else {
                Error::Malformed
            })
        );
        assert_eq!(
            (charge.g(), charge.p()),
            (if maximum == 1_304 { 1_177 } else { 1_305 }, 0)
        );
    }
    let profile = limits(100_000_000)?;
    let shadowed = [
        (Raw::Atom("mode"), Raw::Nil),
        (Raw::Binary(b"mode"), Raw::Binary(&huge)),
    ];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&shadowed),
            17,
            profile,
            &mut charge
        ),
        Err(Error::ByteLimit)
    );
    let version = [(Raw::Atom("version"), Raw::Binary(b"v"))];
    let profile = limits(1_057)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&version),
            17,
            profile,
            &mut charge
        ),
        Err(work(1_058, 1_057))
    );
    assert_eq!((charge.g(), charge.p()), (1_057, 0));
    Ok(())
}

#[test]
fn original_flow_input_preserves_every_prospective_phase_refusal() -> TestResult {
    let expected = b"\x83\x74\0\0\0\x02\x77\x06states\x74\0\0\0\0\x77\x04type\x6d\0\0\0\x01r";
    assert_eq!(expected.len(), 31);
    assert_stream(&Raw::Map(&[]), &minimum_events(), expected, (9_374, 4_096))?;
    assert_nonempty_phases()?;
    assert_wrong_list_root()?;
    super::cases::assert_order_tariff()?;
    assert_body_streams()?;
    assert_invalid_reason_parameters()?;
    Ok(())
}

fn minimum_events() -> Vec<(u64, u64)> {
    let mut events = vec![
        (128, 0),
        (128, 0),
        (1, 0),
        (128, 0),
        (0, 4_096),
        (3_200, 0),
        (896, 0),
        (192, 0),
    ];
    // Source root/selectors/absent validators/indexed roots/default states:15.
    events.extend(std::iter::repeat_n((128, 0), 15));
    // Five normalized cells and four paired map edges; VERSION is no cell.
    events.extend(std::iter::repeat_n((128, 0), 9));
    events.extend([1, 133, 136, 133, 134, 134].map(|fee| (fee, 0)));
    events.push((159, 0));
    events.extend([1, 133, 136, 133, 134, 134].map(|fee| (fee, 0)));
    events.push((128, 0));
    events
}

pub(super) fn assert_stream(
    input: &Raw<'_>,
    events: &[(u64, u64)],
    expected: &[u8],
    final_account: (u64, u64),
) -> TestResult {
    let (g, p) = assert_events(input, events)?;
    assert_eq!((g, p), final_account);
    let profile = limits(g + p)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Binary(b"r"), input, 17, profile, &mut charge)?,
        Ok(expected.to_vec())
    );
    assert_eq!((charge.g(), charge.p()), final_account);
    Ok(())
}

fn assert_events(input: &Raw<'_>, events: &[(u64, u64)]) -> TestResult<(u64, u64)> {
    let (mut g, mut p) = (0, 0);
    for &(fee, peak) in events {
        let next_p = p.max(peak);
        let target = g + fee + next_p;
        if target > g + p {
            let profile = limits(target - 1)?;
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                normalize(&Raw::Binary(b"r"), input, 17, profile, &mut charge),
                Err(work(target, target - 1))
            );
            assert_eq!((charge.g(), charge.p()), (g, p));
        }
        (g, p) = (g + fee, next_p);
    }
    Ok((g, p))
}

fn assert_nonempty_phases() -> TestResult {
    let leaves = [Raw::Binary(b"a"), Raw::Binary(b"b"), Raw::Binary(b"a")];
    let effects = [(Raw::Atom("allowed"), Raw::List(&leaves))];
    let governance = [(Raw::Atom("effects"), Raw::Map(&effects))];
    let attrs = [(Raw::Atom("governance"), Raw::Map(&governance))];
    let input = Raw::Map(&attrs);
    // Fixed46 Map-only classification lengths, independent of candidate tables.
    let queries = [
        4, 7, 13, 5, 9, 18, 18, 10, 6, 12, 11, 7, 12, 4, 7, 6, 10, 6, 22, 18, 7, 5, 6, 7, 8, 9, 7,
    ];
    let mut events = vec![(128, 0), (128, 0), (1, 0), (128, 0)];
    for (width, comparisons) in [(10, 8), (7, 21), (7, 27)] {
        events.extend([(128, 0), (width, 0)]);
        events.extend(queries[..comparisons].iter().map(|&q| (128 + width + q, 0)));
        events.extend([(128, 0), (128, 0)]); // membership, child container
    }
    for _ in 0..3 {
        events.extend([(128, 0), (1, 0)]);
    }
    assert_eq!(assert_events(&input, &events)?, (10_082, 0));
    events.extend([(0, 10_400), (7_808, 0), (2_048, 0), (480, 0)]);
    assert_eq!(assert_events(&input, &events)?, (20_418, 10_400));
    // First governance list has no earlier normalized cell placeholders.
    events.push((128, 0));
    for (query, validator) in [
        (4, 0),
        (7, 128),
        (13, 128),
        (5, 0),
        (9, 0),
        (18, 128),
        (18, 128),
        (10, 0),
    ] {
        super::cases::selector_events(&mut events, 10, query);
        if validator != 0 {
            events.push((validator, 0));
        }
    }
    events.push((128, 0));
    for query in [4, 7] {
        super::cases::selector_events(&mut events, 7, query);
    }
    events.push((128, 0));
    super::cases::selector_events(&mut events, 7, 7);
    events.extend([128, 128, 129, 128, 128, 129, 16, 128, 128, 129, 16, 130].map(|fee| (fee, 0)));
    assert_eq!(assert_events(&input, &events)?, (27_159, 10_400));
    let expected = super::snapshot(
        b"r",
        &[(
            "governance",
            super::map(&[(
                "effects",
                super::map(&[(
                    "allowed",
                    super::list(&[super::binary(b"a")?, super::binary(b"b")?])?,
                )])?,
            )])?,
        )],
    )?;
    extend_nonempty_tail(&mut events);
    assert_stream(&input, &events, &expected, (38_541, 10_400))
}

fn extend_nonempty_tail(events: &mut Vec<(u64, u64)>) {
    events.extend(std::iter::repeat_n((128, 0), 5)); // list cells/edges
    for query in [6, 23] {
        super::cases::selector_events(events, 7, query);
    }
    events.extend(std::iter::repeat_n((128, 0), 4)); // effects Map
    for query in [5, 6, 7, 8, 9] {
        super::cases::selector_events(events, 7, query);
    }
    events.extend(std::iter::repeat_n((128, 0), 4)); // governance Map
    let governance_measure = [1, 133, 137, 133, 137, 133, 134, 134, 1];
    events.extend(governance_measure.map(|fee| (fee, 0)));
    super::cases::selector_events(events, 10, 6);
    events.push((128, 0)); // default states map walk
    events.extend(std::iter::repeat_n((128, 0), 12)); // remaining root layout
    let writer = [
        1, 133, 140, 133, 137, 133, 137, 133, 134, 134, 1, 136, 133, 134, 134,
    ];
    events.extend(writer.map(|fee| (fee, 0)));
    events.push((217, 0));
    events.extend(writer.map(|fee| (fee, 0)));
    events.push((128, 0));
}

fn assert_wrong_list_root() -> TestResult {
    let effects = [(Raw::Atom("allowed"), Raw::Other)];
    let governance = [(Raw::Atom("effects"), Raw::Map(&effects))];
    let attrs = [(Raw::Atom("governance"), Raw::Map(&governance))];
    for maximum in [31_886, 31_887] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge,
        );
        if maximum == 31_886 {
            assert_eq!(result, Err(work(31_887, maximum)));
            assert_eq!((charge.g(), charge.p()), (23_439, 8_320));
        } else {
            assert_eq!(
                result?,
                Err(Reason::Governance(Gov::StringList(Field::Allowed)))
            );
            assert_eq!((charge.g(), charge.p()), (23_567, 8_320));
        }
    }
    let governance = [(Raw::Atom("limits"), Raw::Other)];
    let attrs = [(Raw::Atom("governance"), Raw::Map(&governance))];
    let profile = limits(25_367)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            17,
            profile,
            &mut charge
        ),
        Err(work(25_368, 25_367))
    );
    assert_eq!((charge.g(), charge.p()), (18_328, 6_912));
    Ok(())
}

fn assert_body_streams() -> TestResult {
    for (reason, body, digits) in [
        (
            Reason::RootShape,
            b"ERR flow policy must be a map or keyword list".as_slice(),
            0,
        ),
        (
            Reason::Governance(Gov::PolicyBytes),
            b"ERR flow governance policy exceeds 1048576-byte durable limit",
            0,
        ),
        (
            Reason::Retention(Retention::History { maximum: 1 }),
            b"ERR flow retention history_max_events must be between 1 and 1",
            1,
        ),
        (
            Reason::Retention(Retention::History { maximum: 17 }),
            b"ERR flow retention history_max_events must be between 1 and 17",
            2,
        ),
        (
            Reason::Retention(Retention::History { maximum: 1_000_000 }),
            b"ERR flow retention history_max_events must be between 1 and 1000000",
            7,
        ),
    ] {
        let mut g = 0;
        for fee in [128, digits, 128 + u64::try_from(body.len())?] {
            if fee != 0 {
                let target = g + fee;
                let profile = limits(target - 1)?;
                let mut charge = PolicySchemaCharge::new(profile);
                assert_eq!(
                    encode(reason, profile, &mut charge),
                    Err(work(target, target - 1))
                );
                assert_eq!((charge.g(), charge.p()), (g, 0));
            }
            g += fee;
        }
        let profile = limits(g)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(encode(reason, profile, &mut charge)?, body);
        assert_eq!((charge.g(), charge.p()), (g, 0));
    }
    Ok(())
}

fn assert_invalid_reason_parameters() -> TestResult {
    use wf_domain::policy::complete::GovernanceField as Field;
    for reason in [
        Reason::Governance(Gov::StringList(Field::Limit)),
        Reason::Governance(Gov::ListBytes(Field::Unit)),
        Reason::Governance(Gov::Positive(Field::Allowed)),
        Reason::Retention(Retention::History { maximum: 0 }),
        Reason::Retention(Retention::History { maximum: 1_000_001 }),
    ] {
        let profile = limits(10_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            encode(reason, profile, &mut charge),
            Err(Error::Schema(PolicySchemaError::InvalidAuthority))
        );
        assert_eq!((charge.g(), charge.p()), (128, 0));
    }
    Ok(())
}

fn work(bytes: u64, maximum: u64) -> Error {
    Error::Schema(PolicySchemaError::WorkBytes { bytes, maximum })
}

#[test]
fn original_flow_input_preserves_continuous_work_and_held_owners() -> TestResult {
    for (maximum, attempts, seeded, expected_g) in [
        (13_470, 1, false, 9_374),
        (13_469, 1, false, 9_246),
        (22_844, 2, false, 18_748),
        (22_843, 2, false, 18_620),
        (15_398, 1, true, 11_302),
        (15_397, 1, true, 11_174),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let held = if seeded {
            let mut value = super::super::data(b"r".to_vec(), None, BTreeMap::new());
            value.indexed_attributes = Some(vec![b"x".to_vec()]);
            let owner = restore_complete_policy(1, value, profile, &mut charge)?;
            assert_eq!((charge.g(), charge.p()), (1_928, 64));
            Some(owner)
        } else {
            None
        };
        let expected = snapshot(b"r", &[])?;
        let mut first = None;
        for attempt in 0..attempts {
            let actual = normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, profile, &mut charge);
            if attempt + 1 == attempts && maximum % 2 == 1 {
                assert_eq!(actual, Err(work(maximum + 1, maximum)));
            } else {
                let bytes = actual?.map_err(|reason| format!("unexpected {reason:?}"))?;
                assert_eq!(bytes, expected);
                if first.is_none() {
                    first = Some(bytes);
                }
            }
            if let Some(bytes) = &first {
                assert_eq!(bytes, &expected);
            }
        }
        assert_eq!((charge.g(), charge.p()), (expected_g, 4_096));
        if let Some(owner) = held {
            assert_eq!(owner.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        }
    }
    assert_indexed_seed()?;
    assert_larger_peak_owners()?;
    super::cases::assert_captured_history()?;
    super::cases::assert_old_primitive_fees()?;
    Ok(())
}

fn assert_indexed_seed() -> TestResult {
    let profile = limits(18_254)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let names = [
        IndexedRaw::Binary(b"tenant"),
        IndexedRaw::Atom("region"),
        IndexedRaw::Binary(b"campaign"),
        IndexedRaw::Binary(b"tenant"),
    ];
    let held = normalize_complete_policy_indexed_input(
        Some(IndexedRaw::List(&names)),
        None,
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("seed {reason:?}"))?;
    assert_eq!((charge.g(), charge.p()), (4_784, 192));
    assert_eq!(
        normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, profile, &mut charge)?,
        Ok(snapshot(b"r", &[])?)
    );
    assert_eq!((charge.g(), charge.p()), (14_158, 4_096));
    assert_eq!(
        held,
        (
            Some(vec![
                b"tenant".to_vec(),
                b"region".to_vec(),
                b"campaign".to_vec()
            ]),
            None
        )
    );
    Ok(())
}

fn assert_larger_peak_owners() -> TestResult {
    // Fresh source arithmetic: N6/D2, G13726/P6912, then MIN adds9374.
    let profile = limits(38_586)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let leaves = [Raw::Nil, Raw::Nil];
    let unknown = [(Raw::Other, Raw::List(&leaves))];
    let expected = snapshot(b"r", &[])?;
    let first = normalize(
        &Raw::Binary(b"r"),
        &Raw::Map(&unknown),
        17,
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("first {reason:?}"))?;
    let first_pointer = first.as_ptr();
    assert_eq!(first, expected);
    assert_eq!((charge.g(), charge.p()), (13_726, 6_912));
    let second = normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, profile, &mut charge)?
        .map_err(|reason| format!("second {reason:?}"))?;
    let second_pointer = second.as_ptr();
    assert_eq!(second, expected);
    assert_eq!(first, expected);
    assert_eq!(first.as_ptr(), first_pointer);
    assert_eq!((charge.g(), charge.p()), (23_100, 6_912));
    assert_eq!(
        normalize(&Raw::Binary(b"r"), &Raw::Map(&[]), 17, profile, &mut charge),
        Err(work(38_587, 38_586))
    );
    assert_eq!((charge.g(), charge.p()), (31_516, 6_912));
    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert_eq!(
        (first.as_ptr(), second.as_ptr()),
        (first_pointer, second_pointer)
    );
    assert!(matches!(leaves, [Raw::Nil, Raw::Nil]));
    Ok(())
}
