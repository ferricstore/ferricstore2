//! Prospective source/count priorities and independently bounded live stages.

use super::{
    ApplyError, EncodingError, ExistingError, PolicySchemaCharge, PolicySchemaError, Raw, RawError,
    Reason, RequestError, TestResult, accepted, current, invoke, invoke_for_type, profile,
    rejected,
};
use wf_domain::policy::complete::{
    GovernancePolicyInputRejection as Gov, GovernanceRuleKind as Kind,
};

#[test]
fn original_public_merge_prepays_each_joined_operation() -> TestResult {
    assert_profile_mismatch()?;
    assert_rule_count_before_capture()?;
    assert_merged_business_after_context()?;
    assert_large_pipeline_profile()?;
    assert_fresh_stream()?;
    assert_first_growth()?;
    assert_catalog_prefix()?;
    assert_exact_hit_prefix()?;
    assert_delete_prefix()?;
    Ok(())
}

pub(super) fn fresh_events() -> Vec<(u64, u64, bool)> {
    let mut out = vec![(128, 763, false)];
    out.extend([128, 142, 129, 128, 128, 128, 16, 128, 16, 128, 16, 16, 16].map(|g| (g, 0, false)));
    out.extend([(0, 827, false), (0, 13_219, false)]);
    out.extend(
        [
            4_352, 5_408, 2_240, 392, 128, 128, 128, 128, 128, 128, 1, 128,
        ]
        .map(|g| (g, 0, false)),
    );
    out.push((0, 38_147, false));
    out.extend([19_072, 4_864, 1_184].map(|g| (g, 0, false)));
    out.extend(std::iter::repeat_n((128, 0, false), 24));
    out.extend([1, 133, 136, 133, 134, 134, 129, 129, 128].map(|g| (g, 0, false)));
    out.push((0, 38_198, false));
    out.extend([179, 1, 130, 144, 130, 133, 136, 133, 134, 134].map(|g| (g, 0, false)));
    out.push((0, 38_212, false));
    out.extend([142, 128, 128, 128, 128, 179, 128].map(|g| (g, 0, false)));
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 46_000);
    out
}

fn assert_fresh_stream() -> TestResult {
    let events = fresh_events();
    assert_events(&events, true, |limits, charge| {
        invoke_for_type(b"r", None, Raw::List(&[]), 1_000_000, limits, charge)
    })
}

pub(super) fn assert_events(
    events: &[(u64, u64, bool)],
    complete: bool,
    mut run: impl FnMut(
        super::PolicySchemaLimits,
        &mut PolicySchemaCharge,
    ) -> TestResult<super::OriginalPolicySetFresh>,
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for (i, &(fee, peak, current)) in events.iter().enumerate() {
        let next_g = g + fee;
        let next_p = p.max(peak);
        let target = next_g + next_p;
        if target > g + p {
            let limits = profile(target - 1)?;
            let mut charge = PolicySchemaCharge::new(limits);
            let error = run(limits, &mut charge)
                .err()
                .ok_or("expected prospective refusal")?;
            assert_eq!(
                *error.downcast::<RawError>().map_err(|_| "error origin")?,
                work(target, target - 1, current)
            );
            assert_eq!((charge.g(), charge.p()), (g, p), "event{i}");
            if complete {
                let limits = profile(target)?;
                let mut charge = PolicySchemaCharge::new(limits);
                let result = run(limits, &mut charge);
                if let Some(&(future, next_peak, next_current)) = events.get(i + 1) {
                    let attempted = next_g + future + next_p.max(next_peak);
                    assert_eq!(
                        *result
                            .err()
                            .ok_or("next refusal")?
                            .downcast::<RawError>()
                            .map_err(|_| "next origin")?,
                        work(attempted, target, next_current)
                    );
                } else {
                    accepted(&result?, 0, 1, &current::minimal(1)?)?;
                }
                assert_eq!((charge.g(), charge.p()), (next_g, next_p));
            }
        }
        (g, p) = (next_g, next_p);
    }
    Ok(())
}

pub(super) fn work(target: u64, maximum: u64, current: bool) -> RawError {
    let error = EncodingError::Schema(PolicySchemaError::WorkBytes {
        bytes: target,
        maximum,
    });
    if current {
        RawError::Current(wf_domain::policy::complete::OriginalPolicyReadError::Encoding(error))
    } else {
        RawError::Replace(ExistingError::Request(RequestError::Encoding(error)))
    }
}

fn assert_first_growth() -> TestResult {
    // F220/Cpatch72/Cmerged72, Npatch3/T35/D1. First cap0->2 table.
    let mut events = vec![(128, 808, false)];
    events.extend(
        [128, 142, 129, 128, 128, 128, 16, 128, 16, 128, 16, 16, 16].map(|g| (g, 0, false)),
    );
    events.extend([(0, 880, false), (0, 14_120, false)]);
    events.extend(
        [
            4_608, 5_728, 2_368, 408, 128, 128, 139, 128, 128, 129, 128, 128, 128, 16, 128, 128,
        ]
        .map(|g| (g, 0, false)),
    );
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 15_795);
    let pair = [Raw::Atom("x"), Raw::Integer(7)];
    let opts = [Raw::Tuple(&pair)];
    let records = [(0, &[0][..], &[0][..])];
    assert_events(&events, false, |limits, charge| {
        super::invoke_captured(
            (b"r", None, 1_000_000),
            Raw::List(&opts),
            &[],
            &records,
            (limits, charge),
        )
    })
}

fn assert_catalog_prefix() -> TestResult {
    // Existing exact reader: type-only38B, Nc6/Kc1/R3448 and two-atom safety.
    let fields = [(Raw::Atom("type"), Raw::Binary(b"r"))];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    assert_eq!(bytes.len(), 38);
    let mut events = vec![(128, 763, false)];
    events.extend([128, 142, 129].map(|g| (g, 0, false)));
    events.push((0, 801, true));
    events.extend(reader_events());
    events.extend([(128, 0, false), (130, 0, true)]);
    events.extend([128, 204, 128, 16, 128, 16, 128, 16, 16, 16].map(|g| (g, 0, false)));
    events.extend([(0, 1_313, false), (0, 1_489, true)]);
    // After reverse spans5..1, span0 appends three heads then writes header.
    events
        .extend([176, 24, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128].map(|g| (g, 0, true)));
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 9_839);
    let fixture = profile(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(fixture);
    let prepared = super::prepare_original_policy_set_request(
        &Raw::Binary(b"r"),
        &Raw::List(&[]),
        fixture,
        &mut setup,
    )?
    .map_err(|_| "catalog prepared")?;
    let patch = [(0, &[][..], &[][..])];
    let merged = [(0, &[][..], &[][..])];
    let orders =
        super::OriginalPolicySetMapOrders::new([0; 32], Some(&bytes), &patch, &[], &merged);
    assert_events(&events, false, |limits, charge| {
        super::invoke_checked(
            (
                b"r",
                Some(&bytes),
                1_000_000,
                super::OriginalEtfSafety::new(&["flow_policy_v1", "type"], &[], None, &[]),
            ),
            &prepared,
            &orders,
            (limits, charge),
        )
    })
}

fn assert_exact_hit_prefix() -> TestResult {
    // Same physical current reader; incoming type:r hits its existing atom key.
    let fields = [(Raw::Atom("type"), Raw::Binary(b"r"))];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    let mut events = vec![(128, 810, false)];
    events.extend([128, 142, 129].map(|g| (g, 0, false)));
    events.push((0, 848, true));
    events.extend(reader_events().into_iter().map(|(g, p, e)| {
        (
            g,
            match p {
                1_249 => 1_296,
                929 => 976,
                _ => p,
            },
            e,
        )
    }));
    events.extend([(128, 0, false), (130, 0, true)]);
    events.extend([128, 204, 128, 16, 128, 16, 128, 16, 16, 16].map(|g| (g, 0, false)));
    events.extend([(0, 1_368, false), (0, 1_544, true)]);
    events
        .extend([176, 24, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128].map(|g| (g, 0, true)));
    events.push((0, 16_944, false));
    events.extend(
        [
            5_376, 6_688, 2_752, 456, 128, 128, 142, 128, 128, 132, 128, 128, 128, 16, 128, 128,
            128, 132, 16, 128, 136, 128,
        ]
        .map(|g| (g, 0, false)),
    );
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 27_221);
    let fixtures = profile(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(fixtures);
    let pair = [Raw::Atom("type"), Raw::Binary(b"r")];
    let opts = [Raw::Tuple(&pair)];
    let prepared = super::prepare_original_policy_set_request(
        &Raw::Binary(b"r"),
        &Raw::List(&opts),
        fixtures,
        &mut setup,
    )?
    .map_err(|_| "hit prepared")?;
    let patch = [(0, &[0][..], &[0][..])];
    let merged = [(0, &[0][..], &[0][..])];
    let orders =
        super::OriginalPolicySetMapOrders::new([0; 32], Some(&bytes), &patch, &[], &merged);
    assert_events(&events, false, |limits, charge| {
        super::invoke_checked(
            (
                b"r",
                Some(&bytes),
                1_000_000,
                super::OriginalEtfSafety::new(&["flow_policy_v1", "type"], &[], None, &[]),
            ),
            &prepared,
            &orders,
            (limits, charge),
        )
    })
}

fn reader_events() -> Vec<(u64, u64, bool)> {
    // Unchanged reader lexical/identity stream from the earlier source freeze.
    let lex = [
        1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 4, 146, 136, 128, 1, 4,
        1,
    ];
    let mut out = [128, 1, 1, 128, 142, 132].map(|g| (g, 0, true)).to_vec();
    out.extend(lex.map(|g| (g, 0, true)));
    out.extend([
        (0, 1_249, true),
        (320, 0, true),
        (132, 0, true),
        (128, 0, true),
    ]);
    out.extend(lex.map(|g| (g, 0, true)));
    out.extend(
        [
            128, 128, 128, 156, 128, 128, 128, 128, 128, 4, 137, 138, 144, 128,
        ]
        .map(|g| (g, 0, true)),
    );
    out.push((0, 929, true));
    out.extend([128, 132, 137, 128, 132, 138, 128, 132, 136, 128, 128].map(|g| (g, 0, true)));
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 6_778);
    out
}

fn assert_delete_prefix() -> TestResult {
    let states = [(Raw::Binary(b"queued"), Raw::Nil)];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&states)),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    assert_eq!(bytes.len(), 67);
    let mut events = vec![(128, 939, false)];
    events.extend([128, 142, 129].map(|g| (g, 0, false)));
    events.push((0, 1_006, true));
    events.extend(delete_reader());
    events.extend([(128, 0, false), (130, 0, true)]);
    events.extend([128, 262, 128, 16, 128, 16].map(|g| (g, 0, false)));
    events.push((0, 2_046, true));
    events.extend(
        [
            208, 40, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128, 128,
        ]
        .map(|g| (g, 0, true)),
    );
    events.push((0, 19_734, false));
    // Right guard/map; left states first insert, left type grow2->4+reinsert;
    // exact states hit; nested guard/map/copy; queued hit; three real deletes.
    events.extend(
        [
            6_144, 7_648, 3_136, 504, 128, 128, 144, 128, 128, 134, 128, 128, 128, 16, 128, 128,
            128, 132, 128, 128, 128, 128, 16, 128, 128, 128, 16, 128, 128, 128, 134, 16, 128, 140,
            128, 128, 128, 128, 134, 128, 128, 128, 16, 128, 128, 128, 134, 16, 128, 140, 128, 128,
            128,
        ]
        .map(|g| (g, 0, false)),
    );
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 39_767);
    let fixture = profile(2_000_000_000)?;
    let mut setup = PolicySchemaCharge::new(fixture);
    let pair = [Raw::Atom("states"), Raw::Map(&states)];
    let opts = [Raw::Tuple(&pair)];
    let prepared = super::prepare_original_policy_set_request(
        &Raw::Binary(b"r"),
        &Raw::List(&opts),
        fixture,
        &mut setup,
    )?
    .map_err(|_| "delete prepared")?;
    let patch = [(0, &[0][..], &[0][..]), (1, &[0][..], &[0][..])];
    let orders = super::OriginalPolicySetMapOrders::new([0; 32], Some(&bytes), &patch, &[], &[]);
    assert_events(&events, false, |limits, charge| {
        super::invoke_checked(
            (
                b"r",
                Some(&bytes),
                1_000_000,
                super::OriginalEtfSafety::new(
                    &["flow_policy_v1", "states", "type", "nil"],
                    &[],
                    None,
                    &[],
                ),
            ),
            &prepared,
            &orders,
            (limits, charge),
        )
    })
}

fn delete_reader() -> Vec<(u64, u64, bool)> {
    // Independent pinned reader derivation by PERF, before any SUT/result.
    let lex = [
        1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 6, 148, 140, 128, 1, 4,
        128, 1, 4, 6, 128, 1, 1, 3, 145, 137, 135, 134, 128, 1, 1, 4, 146, 138, 136, 128, 1, 4, 1,
    ];
    let mut out = [128, 1, 1, 128, 142, 134, 132, 131]
        .map(|g| (g, 0, true))
        .to_vec();
    out.extend(lex.map(|g| (g, 0, true)));
    out.extend([
        (0, 1_838, true),
        (448, 0, true),
        (140, 0, true),
        (128, 0, true),
    ]);
    out.extend(lex.map(|g| (g, 0, true)));
    out.extend(
        [
            128, 128, 128, 128, 128, 138, 138, 128, 156, 128, 128, 128, 128, 128, 6, 139, 140, 128,
            128, 6, 139, 140, 146, 128, 128, 4, 137, 138, 144, 128, 128, 134, 139, 132, 137, 128,
            134, 140, 128, 128, 128, 128, 128, 134, 138, 132, 136, 128, 128,
        ]
        .map(|g| (g, 0, true)),
    );
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 13_164);
    out
}

fn assert_profile_mismatch() -> TestResult {
    let limits = profile(1_000_000)?;
    let other = profile(999_999)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        invoke_for_type(b"r", None, Raw::List(&[]), 1_000_000, other, &mut charge)
            .err()
            .and_then(|e| e.downcast::<RawError>().ok())
            .map(|e| *e),
        Some(schema(PolicySchemaError::ProfileMismatch)),
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}

fn assert_rule_count_before_capture() -> TestResult {
    let names = (0..1_001)
        .map(|n| format!("rule-{n:04}"))
        .collect::<Vec<_>>();
    let rules = names
        .iter()
        .map(|name| (Raw::Binary(name.as_bytes()), Raw::Map(&[])))
        .collect::<Vec<_>>();
    let governance = [(Raw::Atom("limits"), Raw::Map(&rules))];
    let fields = [
        (Raw::Atom("governance"), Raw::Map(&governance)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    // No current source-order record is supplied: map_size1001 must decide
    // BEFORE actual Enum dispatch asks for such a record or visits any rule.
    rejected(
        &invoke(Some(&bytes), Raw::List(&[]), 1_000_000)?,
        Reason::Governance(Gov::RuleCount(Kind::Limits)),
        b"ERR flow governance limits policy must contain at most 1000 rules",
    );
    Ok(())
}

fn assert_merged_business_after_context() -> TestResult {
    let fields = [
        (Raw::Atom("max_active_ms"), Raw::Integer(0)),
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(b"r")),
    ];
    let bytes = current::envelope(1, Raw::Map(&fields))?;
    assert_eq!(
        invoke(Some(&bytes), Raw::List(&[]), 0)
            .err()
            .and_then(|e| e.downcast::<RawError>().ok())
            .map(|e| *e),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::InvalidAuthority
        ))),
    );
    rejected(
        &invoke(Some(&bytes), Raw::List(&[]), 10)?,
        Reason::MaxActive,
        b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
    );
    Ok(())
}

fn assert_large_pipeline_profile() -> TestResult {
    // Public N131072, converted Npatch131070, canonical patch2228187.
    // O+S alone is135300528 before F/H/G, above default134217728.
    let leaves = vec![Raw::List(&[]); 131_067];
    let field = [Raw::Atom("x"), Raw::List(&leaves)];
    let opts = [Raw::Tuple(&field)];
    let limits = super::PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let failure = invoke_for_type(b"r", None, Raw::List(&opts), 1_000_000, limits, &mut charge)
        .err()
        .and_then(|e| e.downcast::<RawError>().ok())
        .map(|e| *e);
    assert!(matches!(
        failure,
        Some(RawError::Replace(ExistingError::Request(
            RequestError::Encoding(EncodingError::Schema(PolicySchemaError::WorkBytes { .. }))
        )))
    ));
    let ample = profile(2_000_000_000)?;
    let mut admitted = PolicySchemaCharge::new(ample);
    let expected = current::minimal(1)?;
    let result = invoke_for_type(
        b"r",
        None,
        Raw::List(&opts),
        1_000_000,
        ample,
        &mut admitted,
    )?;
    accepted(&result, 0, 1, &expected)?;
    assert_eq!(leaves.len(), 131_067);
    Ok(())
}

fn schema(error: PolicySchemaError) -> RawError {
    RawError::Replace(ExistingError::Request(RequestError::Encoding(
        EncodingError::Schema(error),
    )))
}
