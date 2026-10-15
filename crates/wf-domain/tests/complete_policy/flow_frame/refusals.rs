//! Fixed pre-candidate primitive streams, prospective refusal and retained owners.

use super::{
    AdmissionLimits, Error, PolicySchemaCharge, PolicySchemaError, Raw, RequestError, TestResult,
    digest, fixture, hex, input, prepare_original_flow_policy_replace_frame_bytes, profile,
    put_u32, restore, restore_original_flow_policy_replace_frame_header,
    scan_original_flow_policy_replace_frame,
};
use wf_domain::policy::complete::normalize_original_complete_policy_input;

// Frozen MECHANICS1 and independently checked VECTORS2, before implementation.
const PREFLIGHT: [&[u64]; 4] = [
    &[128, 128, 14, 128, 128],
    &[128, 128, 14, 128, 128, 128, 1, 128, 128, 1, 128],
    &[
        128, 128, 14, 128, 128, 128, 1, 128, 128, 1, 128, 128, 1, 128,
    ],
    &[128, 128, 14, 128, 2, 128],
];

const MEASURE: [&[u64]; 4] = [
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 35,
        128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 128, 4, 4, 4, 128, 4, 4, 128, 4, 4, 4,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 35,
        128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 128, 128, 128, 128, 128, 128, 128, 128, 130,
        256, 128, 128, 128, 130, 4, 4, 4, 4, 4, 4, 4, 128, 4, 4, 4, 4, 4, 4, 128, 128, 4, 1, 4, 1,
        128, 4, 1, 128, 128, 4, 1, 4, 1, 128, 4, 1, 128, 4, 4, 4,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 35,
        128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 128, 128, 128, 128, 128, 128, 128, 128, 128,
        128, 130, 128, 128, 128, 130, 256, 128, 128, 128, 130, 256, 128, 128, 128, 130, 128, 128,
        128, 130, 4, 4, 4, 4, 4, 4, 4, 4, 4, 128, 4, 4, 4, 4, 4, 4, 4, 4, 128, 128, 4, 1, 4, 1,
        128, 4, 1, 128, 128, 4, 1, 4, 1, 128, 4, 1, 128, 128, 4, 1, 4, 1, 128, 4, 1, 128, 4, 4, 4,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 35,
        128, 4, 1, 128, 4, 14, 130, 128, 4, 1, 1, 4, 1, 128, 4, 1, 128, 4, 4, 4, 128, 4, 4, 128, 4,
        4, 4,
    ],
];

const EMIT: [&[u64]; 4] = [
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 132, 128, 4, 1, 128, 4, 4, 4, 128, 132, 132,
        132, 132, 132, 132,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 132, 128, 4, 1, 128, 128, 128, 128, 128, 128,
        128, 128, 130, 256, 128, 128, 128, 130, 4, 4, 4, 4, 4, 4, 4, 128, 132, 132, 132, 4, 4, 4,
        132, 4, 132, 128, 132, 128, 4, 1, 4, 1, 132, 132, 128, 4, 1, 132, 128, 132, 128, 4, 1, 4,
        1, 132, 132, 128, 4, 1, 132, 132, 132, 132,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 132, 128, 4, 1, 128, 128, 128, 128, 128, 128,
        128, 128, 128, 128, 130, 128, 128, 128, 130, 256, 128, 128, 128, 130, 256, 128, 128, 128,
        130, 128, 128, 128, 130, 4, 4, 4, 4, 4, 4, 4, 4, 4, 128, 132, 132, 132, 4, 4, 4, 4, 132, 4,
        132, 4, 132, 128, 132, 128, 4, 1, 4, 1, 132, 132, 128, 4, 1, 132, 128, 132, 128, 4, 1, 4,
        1, 132, 132, 128, 4, 1, 132, 128, 132, 128, 4, 1, 4, 1, 132, 132, 128, 4, 1, 132, 132, 132,
        132,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 35, 128, 4, 1, 128, 4, 14, 130, 128, 4, 1, 1, 4, 1, 132, 128, 4, 1, 128, 4, 4, 4,
        128, 132, 132, 132, 132, 132, 132,
    ],
];

const SCAN: [&[u64]; 4] = [
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 4, 4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 4, 4, 4,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 4, 4, 4, 16, 4, 4, 16, 4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 128, 4, 1, 4,
        4, 4, 4, 4, 4, 4, 128, 4, 1, 4, 1, 128, 4, 1, 128, 4, 1, 4, 1, 128, 4, 1, 128, 130,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 4, 4, 4, 16, 4, 4, 16, 4, 4, 16, 4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 128,
        4, 1, 4, 4, 4, 4, 4, 4, 4, 4, 4, 128, 4, 1, 4, 1, 128, 4, 1, 128, 4, 1, 4, 1, 128, 4, 1,
        128, 4, 1, 4, 1, 128, 4, 1, 128, 130, 128, 130,
    ],
    &[
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 4, 4, 128, 4, 4,
        4, 128, 4, 4, 128, 35, 128, 4, 1, 128, 4, 14, 128, 4, 1, 1, 4, 1, 128, 4, 1, 4, 4, 4,
    ],
];

pub(super) fn assert_all_positive_events() -> TestResult {
    let two = [(Raw::Binary(b"b"), Raw::Nil), (Raw::Binary(b"a"), Raw::Nil)];
    let three = [(Raw::Binary(b"c"), Raw::Nil), two[1], two[0]];
    for (case, expected, patch) in [
        (0, Raw::Nil, Raw::Map(&[])),
        (1, Raw::Nil, Raw::Map(&two)),
        (2, Raw::Nil, Raw::Map(&three)),
        (
            3,
            Raw::BigInteger {
                negative: false,
                little_endian: &[1, 0],
            },
            Raw::Map(&[]),
        ),
    ] {
        assert_producer_prefixes(case, expected, patch)?;
        assert_verifier_prefixes(case)?;
    }
    Ok(())
}

fn producer_events(case: usize) -> Vec<(u64, u64)> {
    let peak = [272, 432, 448, 272][case];
    let (reserve, frame, intent) = (
        [144, 176, 192, 144][case],
        [187, 249, 280, 193][case],
        [80, 126, 149, 86][case],
    );
    let mut events: Vec<_> = PREFLIGHT[case].iter().map(|&g| (g, 0)).collect();
    events.push((0, peak));
    events.push((reserve, 0));
    events.extend(MEASURE[case].iter().map(|&g| (g, 0)));
    events.extend([(128 + frame, 0), (frame, 0)]);
    events.extend(EMIT[case].iter().map(|&g| (g, 0)));
    events.extend([(128 + intent, 0), (160, 0), (128 + frame, 0)]);
    events
}

fn verifier_events(case: usize) -> Vec<u64> {
    let mut events = SCAN[case].to_vec();
    events.extend([
        130,
        128 + [80, 126, 149, 86][case],
        192,
        128,
        128,
        129,
        129,
        129,
        129,
        128,
        533,
        149,
        533,
        128 + [187, 249, 280, 193][case],
    ]);
    events
}

fn assert_producer_prefixes(case: usize, expected: Raw<'_>, patch: Raw<'_>) -> TestResult {
    let events = producer_events(case);
    let (mut g, mut p) = (0, 0);
    for (index, &(delta, scratch)) in events.iter().enumerate() {
        let next_g = g + delta;
        let next_p = p.max(scratch);
        let target = next_g + next_p;
        for exact in [false, true] {
            let maximum = target - u64::from(!exact);
            let limits = profile(maximum)?;
            let mut charge = PolicySchemaCharge::new(limits);
            let actual = prepare_original_flow_policy_replace_frame_bytes(
                input(expected, patch)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            );
            if exact {
                assert_eq!(
                    (charge.g(), charge.p()),
                    (next_g, next_p),
                    "exact producer case{case} event{index}"
                );
                if index + 1 == events.len() {
                    assert!(actual.is_ok());
                } else {
                    assert!(matches!(
                        actual,
                        Err(RequestError::Encoding(Error::Schema(
                            PolicySchemaError::WorkBytes { .. }
                        )))
                    ));
                }
            } else {
                assert_eq!(
                    actual.err(),
                    Some(work(target, maximum)),
                    "tight producer case{case} event{index}"
                );
                assert_eq!((charge.g(), charge.p()), (g, p));
            }
        }
        (g, p) = (next_g, next_p);
    }
    assert_eq!(
        (g, p),
        (
            [6_473, 13_823, 19_810, 6_771][case],
            [272, 432, 448, 272][case]
        )
    );
    Ok(())
}

fn assert_verifier_prefixes(case: usize) -> TestResult {
    let (frame, recomputed) = verifier_fixture(case)?;
    let events = verifier_events(case);
    let mut g = 0;
    for (index, &delta) in events.iter().enumerate() {
        let target = g + delta;
        for exact in [false, true] {
            let maximum = target - u64::from(!exact);
            let limits = profile(maximum)?;
            let mut charge = PolicySchemaCharge::new(limits);
            let actual = restore(&frame, recomputed, limits, &mut charge);
            if exact {
                assert_eq!(
                    (charge.g(), charge.p()),
                    (target, 0),
                    "exact verifier case{case} event{index}"
                );
                if index + 1 == events.len() {
                    assert!(actual.is_ok());
                } else {
                    assert!(matches!(
                        actual,
                        Err(RequestError::Encoding(Error::Schema(
                            PolicySchemaError::WorkBytes { .. }
                        )))
                    ));
                }
            } else {
                assert_eq!(
                    actual.err(),
                    Some(work(target, maximum)),
                    "tight verifier case{case} event{index}"
                );
                assert_eq!((charge.g(), charge.p()), (g, 0));
            }
        }
        g = target;
    }
    assert_eq!(g, [4_683, 5_655, 6_270, 4_701][case]);
    Ok(())
}

fn verifier_fixture(case: usize) -> TestResult<(Vec<u8>, [u8; 32])> {
    if case < 3 {
        return Ok((fixture(case)?, digest(case)?));
    }
    // Literal mathematical Integer(1): node11 replaces Nil5; Map offset63 becomes69.
    let mut frame = fixture(0)?;
    frame[29..61].fill(0);
    drop(frame.splice(145..150, [0, 0, 0, 11, 1, 0, 0, 0, 0, 1, 1]));
    put_u32(&mut frame, 79, 86);
    put_u32(&mut frame, 185, 69);
    assert_eq!(frame.len(), 193);
    Ok((frame, [0; 32]))
}

pub(super) fn assert_profiles_and_visible_caps() -> TestResult {
    let limits = profile(1_000_000)?;
    let other = profile(1_000_001)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Map(&[]))?,
            other,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::Schema(
            PolicySchemaError::ProfileMismatch
        )))
    );
    let fixed = fixture(0)?;
    assert_eq!(
        scan_original_flow_policy_replace_frame(
            &fixed,
            other,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(Error::Schema(
            PolicySchemaError::ProfileMismatch
        )))
    );
    let mut scan_charge = PolicySchemaCharge::new(limits);
    let scanned = scan_original_flow_policy_replace_frame(
        &fixed,
        limits,
        AdmissionLimits::default(),
        &mut scan_charge,
    )?;
    assert_eq!(
        restore_original_flow_policy_replace_frame_header(
            scanned,
            digest(0)?,
            other,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(Error::Schema(
            PolicySchemaError::ProfileMismatch
        )))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for width in [186, 187] {
        let admission = AdmissionLimits::new(1_048_576, width, 1_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let actual = prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Map(&[]))?,
            limits,
            admission,
            &mut charge,
        );
        if width == 187 {
            assert!(actual.is_ok());
        } else {
            assert_eq!(actual.err(), Some(RequestError::Encoding(Error::ByteLimit)));
        }
        let mut charge = PolicySchemaCharge::new(limits);
        let actual =
            scan_original_flow_policy_replace_frame(&fixed, limits, admission, &mut charge);
        if width == 187 {
            assert!(actual.is_ok());
        } else {
            assert_eq!(actual.err(), Some(RequestError::Encoding(Error::ByteLimit)));
            assert_eq!((charge.g(), charge.p()), (0, 0));
        }
    }
    assert_depth(Raw::Nil, 64, 64)?;
    assert_depth(Raw::Nil, 65, 65)?;
    assert_occurrences()?;
    assert_bytes()?;
    Ok(())
}

fn assert_depth(value: Raw<'_>, remaining: usize, requested: usize) -> TestResult {
    if remaining != 0 {
        let child = [value];
        return assert_depth(Raw::Tuple(&child), remaining - 1, requested);
    }
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let actual = prepare_original_flow_policy_replace_frame_bytes(
        input(Raw::Nil, value)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    );
    if requested == 64 {
        let (frame, _, _) = actual?;
        assert_eq!(charge.p(), 8_976);
        assert_ceiling_scaffold(&frame, limits)?;
    } else {
        assert_eq!(actual.err(), Some(RequestError::Encoding(Error::ItemLimit)));
        assert_eq!((charge.g(), charge.p()), (8_846, 0));
    }
    Ok(())
}

fn assert_occurrences() -> TestResult {
    let shared = [Raw::Nil];
    for alias in [false, true] {
        let mut children = if alias {
            vec![Raw::Tuple(&shared); 32_767]
        } else {
            vec![Raw::Nil; 65_534]
        };
        let limits = profile(1_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let (frame, _, _) = prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::List(&children))?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(charge.p(), if alias { 524_800 } else { 524_672 });
        assert_ceiling_scaffold(&frame, limits)?;
        children.push(Raw::Nil);
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare_original_flow_policy_replace_frame_bytes(
                input(Raw::Nil, Raw::List(&children))?,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RequestError::Encoding(Error::ItemLimit))
        );
        assert_eq!((charge.g(), charge.p()), (8_389_006, 0));
    }
    Ok(())
}

fn assert_ceiling_scaffold(frame: &[u8], limits: super::PolicySchemaLimits) -> TestResult {
    let mut charge = PolicySchemaCharge::new(limits);
    let header = restore(frame, [0; 32], limits, &mut charge)?;
    assert_eq!(header.encoded().as_ptr(), frame.as_ptr());
    assert_eq!(charge.p(), 0);
    Ok(())
}

fn assert_bytes() -> TestResult {
    for extra in [false, true] {
        let width = 64 * 1_024 * 1_024 - super::KEY.len() + usize::from(extra);
        let bytes = vec![0; width];
        // Exact B is admitted, then its prospective payload walk refuses before copying.
        let limits = profile(526)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let actual = prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Binary(&bytes))?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        let expected = if extra {
            RequestError::Encoding(Error::ByteLimit)
        } else {
            work(526 + u64::try_from(width)?, 526)
        };
        assert_eq!(actual.err(), Some(expected));
        assert_eq!((charge.g(), charge.p()), (526, 0));
    }
    Ok(())
}

pub(super) fn assert_continuous_owners() -> TestResult {
    let limits = profile(33_203)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let (first, _, _) = prepare_original_flow_policy_replace_frame_bytes(
        input(Raw::Nil, Raw::Map(&[]))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let first_pointer = first.as_ptr();
    let rows = [
        (Raw::Binary(b"c"), Raw::Nil),
        (Raw::Binary(b"a"), Raw::Nil),
        (Raw::Binary(b"b"), Raw::Nil),
    ];
    let (second, _, _) = prepare_original_flow_policy_replace_frame_bytes(
        input(Raw::Nil, Raw::Map(&rows))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let second_pointer = second.as_ptr();
    assert_eq!((charge.g(), charge.p()), (26_283, 448));
    assert_eq!(
        prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Map(&[]))?,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(work(33_204, 33_203))
    );
    assert_eq!((charge.g(), charge.p()), (32_441, 448));
    let mut expected_first = fixture(0)?;
    expected_first[29..61].fill(0);
    let mut expected_second = fixture(2)?;
    expected_second[29..61].fill(0);
    assert_eq!(
        (first.as_ptr(), second.as_ptr()),
        (first_pointer, second_pointer)
    );
    assert_eq!(first, expected_first);
    assert_eq!(second, expected_second);
    assert_inherited_snapshot_peak()?;
    Ok(())
}

fn assert_inherited_snapshot_peak() -> TestResult {
    let limits = profile(38_266)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let donor = {
        let ignored = [Raw::Nil, Raw::Nil];
        let attrs = [(Raw::Other, Raw::List(&ignored))];
        normalize_original_complete_policy_input(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            17,
            limits,
            &mut charge,
        )?
        .map_err(|_| "fixed accepted ignored donor")?
    };
    let donor_pointer = donor.as_ptr();
    assert_eq!((charge.g(), charge.p()), (13_726, 6_912));
    let (frame, _, _) = prepare_original_flow_policy_replace_frame_bytes(
        input(Raw::Nil, Raw::Map(&[]))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let pointer = frame.as_ptr();
    assert_eq!((charge.g(), charge.p()), (20_199, 6_912));
    let fixed = fixture(0)?;
    let header = restore(&fixed, digest(0)?, limits, &mut charge)?;
    assert_eq!(header.original_intent(), &fixed[87..167]);
    assert_eq!((charge.g(), charge.p()), (24_882, 6_912));
    assert_eq!(
        prepare_original_flow_policy_replace_frame_bytes(
            input(Raw::Nil, Raw::Map(&[]))?,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(work(38_267, 38_266))
    );
    assert_eq!((charge.g(), charge.p()), (31_040, 6_912));
    assert_eq!((donor.as_ptr(), frame.as_ptr()), (donor_pointer, pointer));
    assert_eq!(
        donor,
        hex("837400000002770673746174657374000000007704747970656d0000000172")?
    );
    let mut expected = fixture(0)?;
    expected[29..61].fill(0);
    assert_eq!(frame, expected);
    Ok(())
}

fn work(bytes: u64, maximum: u64) -> RequestError {
    RequestError::Encoding(Error::Schema(PolicySchemaError::WorkBytes {
        bytes,
        maximum,
    }))
}
