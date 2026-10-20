//! Raw group/scalar compatibility and independent full original wire fixtures.

use super::f::{atom, binary, envelope, fields, integer, map};
use super::{
    FieldPatch, PolicyPatch, PolicySchemaCharge, PolicySchemaLimits, StoredPolicyValue, TestResult,
    f, input, original, partial_change, patch,
};

fn keyword(rows: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = vec![108];
    let count = u32::try_from(rows.len()).unwrap_or_default();
    assert_eq!(u64::from(count), rows.len() as u64);
    out.extend_from_slice(&count.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(&[104, 2]);
        out.extend_from_slice(&f::atom(key));
        out.extend_from_slice(value);
    }
    out.push(106);
    out
}

fn keyword_retry(changed: bool, keywords: bool) -> Vec<u8> {
    let backoff = group(
        keywords,
        &[
            ("base_ms", f::integer(if changed { 9 } else { 17 })),
            ("jitter_pct", f::integer(4)),
            ("kind", f::atom("fixed")),
            ("max_ms", f::integer(33)),
            ("version", vec![110, 3, 0, 1, 0, 0]),
        ],
    );
    let mut rows = vec![
        ("backoff", backoff),
        ("exhausted_to", f::binary(b"dead")),
        ("max_retries", f::integer(if changed { 9 } else { 7 })),
        ("type", f::binary(b"\0\xffopaque")),
    ];
    if keywords {
        rows.insert(0, ("max_retries", f::integer(1)));
    }
    group(keywords, &rows)
}

pub(super) fn assert_keyword_groups(profile: PolicySchemaLimits) -> TestResult {
    use wf_domain::policy::complete::decode_original_complete_policy_entry_portable;
    assert_scalar_forms(profile);
    for keywords in [false, true] {
        let raw = keyword_wire(7, 0, keywords);
        let mut charge = PolicySchemaCharge::new(profile);
        let entry = decode_original_complete_policy_entry_portable(&raw, profile, &mut charge)
            .map_err(|error| format!("keyword donor={keywords}: {error:?}"))?
            .ok_or("healthy keyword donor did not decode")?;
        assert_eq!(
            (entry.generation(), entry.workflow_type()),
            (7, Some(b"r".as_slice()))
        );
        assert_eq!(entry.encoded(), raw);
        let old = StoredPolicyValue::Original(raw.clone());
        for phase in [1, 2] {
            let mut delta = if phase == 1 {
                PolicyPatch {
                    max_active_ms: FieldPatch::Set(3),
                    ..PolicyPatch::default()
                }
            } else {
                partial_change()
            };
            if phase == 2 {
                let FieldPatch::Set(retry) = &mut delta.retry else {
                    return Err("partial retry fixture".into());
                };
                retry.max_retries = Some(9);
            }
            let changed = patch(
                Some(&old),
                0,
                input(Some(7), delta),
                64,
                profile,
                &mut charge,
                0,
            )
            .map_err(|error| format!("keyword patch={keywords}, phase={phase}: {error:?}"))?;
            assert_eq!(original(&changed)?, keyword_wire(8, phase, keywords));
            assert_eq!(original(&old)?, raw);
        }
    }
    assert_chained_keyword_groups(profile)
}

fn group(keywords: bool, rows: &[(&str, Vec<u8>)]) -> Vec<u8> {
    if keywords {
        keyword(rows)
    } else {
        fields(rows)
    }
}

fn keyword_wire(generation: u64, phase: u8, keywords: bool) -> Vec<u8> {
    keyword_wire_full(
        generation,
        phase,
        keywords,
        keyword_retry(phase == 2, keywords),
        false,
    )
}

fn keyword_wire_full(
    generation: u64,
    phase: u8,
    keywords: bool,
    retry: Vec<u8>,
    version_patch: bool,
) -> Vec<u8> {
    let changed = phase == 2;
    let mut state_retry = vec![("max_retries", integer(if changed { 2 } else { 3 }))];
    if keywords {
        state_retry.insert(0, ("max_retries", integer(1)));
    }
    let retained = group(
        keywords,
        &[("history_max_events", integer(17)), ("ttl_ms", integer(50))],
    );
    let mut config = vec![("mode", atom("fifo"))];
    if !changed {
        config.push(("retention", retained.clone()));
    }
    config.push(("retry", group(keywords, &state_retry)));
    if !changed && keywords {
        config.push(("retention", retained));
    }
    config.push(("indexed_state_meta", vec![108, 0, 0, 0, 0, 106]));
    let mut rows = vec![
        (atom("retry"), retry),
        (binary(b"retry"), fields(&[("max_retries", integer(999))])),
        (
            atom("retention"),
            group(
                keywords,
                &[
                    ("history_max_events", integer(64)),
                    ("ttl_ms", integer(if changed { 200 } else { 100 })),
                ],
            ),
        ),
        (
            atom("states"),
            map(&[(binary(b"queued"), group(keywords, &config))]),
        ),
        (atom("type"), binary(b"r")),
    ];
    if !changed {
        rows.extend([
            (
                atom("version"),
                integer(if version_patch && phase == 1 { 9 } else { 7 }),
            ),
            (binary(b"version"), integer(8)),
        ]);
    }
    if phase > 0 && (!version_patch || changed) {
        rows.push((atom("max_active_ms"), integer(3)));
    }
    envelope(generation, &map(&rows))
}

fn assert_chained_keyword_groups(profile: PolicySchemaLimits) -> TestResult {
    use wf_domain::policy::complete::decode_original_complete_policy_entry_portable;
    use wf_domain::policy::definition::PolicyVersion;

    let raw = chained_wire(7, 0);
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode_original_complete_policy_entry_portable(&raw, profile, &mut charge)?
        .ok_or("healthy chained keyword donor did not decode")?;
    assert_eq!(
        (entry.generation(), entry.workflow_type()),
        (7, Some(b"r".as_slice()))
    );
    assert_eq!(entry.encoded(), raw);
    let old = StoredPolicyValue::Original(raw.clone());
    for phase in [1, 2] {
        let mut delta = if phase == 1 {
            PolicyPatch {
                version: FieldPatch::Set(PolicyVersion::Number(9)),
                ..PolicyPatch::default()
            }
        } else {
            partial_change()
        };
        if phase == 2 {
            let FieldPatch::Set(retry) = &mut delta.retry else {
                return Err("partial chained retry fixture".into());
            };
            retry.max_retries = Some(9);
        }
        let result = patch(
            Some(&old),
            0,
            input(Some(7), delta),
            64,
            profile,
            &mut charge,
            0,
        );
        assert_eq!(original(&old)?, raw);
        let actual = result.map_err(|error| format!("chained phase={phase}: {error:?}"))?;
        assert_eq!(original(&actual)?, chained_wire(8, phase));
    }
    Ok(())
}

fn chained_wire(generation: u64, phase: u8) -> Vec<u8> {
    let retry = if phase == 2 {
        keyword_retry(true, true)
    } else {
        let backoff = keyword(&[
            ("base_ms", integer(17)),
            ("jitter_pct", integer(4)),
            ("kind", atom("fixed")),
            ("max_ms", integer(33)),
            ("version", vec![110, 3, 0, 1, 0, 0]),
        ]);
        // Zero-LIST alias -> LIST(1) -> LIST(4) -> NIL is one proper list.
        let mut out = vec![108, 0, 0, 0, 0, 108, 0, 0, 0, 1, 104, 2];
        out.extend_from_slice(&atom("max_retries"));
        out.extend_from_slice(&integer(1));
        out.extend_from_slice(&keyword(&[
            ("backoff", backoff),
            ("exhausted_to", binary(b"dead")),
            ("max_retries", integer(7)),
            ("type", binary(b"\0\xffopaque")),
        ]));
        out
    };
    keyword_wire_full(generation, phase, true, retry, true)
}

fn assert_scalar_forms(profile: PolicySchemaLimits) {
    // Both calls execute before either result is asserted.
    let results = [
        scalar_form(profile, false),
        scalar_form(profile, true),
        numeric_form(profile, 0),
        numeric_form(profile, 1),
        numeric_form(profile, 2),
    ];
    assert!(
        results.iter().all(Result::is_ok),
        "binary kind/mode and padded max-active/retry/retention results: {results:?}"
    );
}

fn scalar_form(profile: PolicySchemaLimits, mode_case: bool) -> TestResult {
    use wf_domain::policy::BackoffKind;
    use wf_domain::policy::complete::decode_original_complete_policy_entry_portable;
    use wf_domain::policy::definition::{BackoffPatch, RetryPatch};

    let raw = scalar_wire(7, 0, mode_case);
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode_original_complete_policy_entry_portable(&raw, profile, &mut charge)?
        .ok_or("healthy binary-scalar donor did not decode")?;
    assert_eq!(
        (entry.generation(), entry.workflow_type()),
        (7, Some(b"r".as_slice()))
    );
    assert_eq!(entry.encoded(), raw);
    let old = StoredPolicyValue::Original(raw.clone());
    for phase in [1, 2] {
        let delta = if phase == 1 {
            PolicyPatch {
                max_active_ms: FieldPatch::Set(3),
                ..PolicyPatch::default()
            }
        } else if mode_case {
            super::fifo()
        } else {
            PolicyPatch {
                retry: FieldPatch::Set(RetryPatch {
                    backoff: Some(BackoffPatch {
                        kind: Some(BackoffKind::Linear),
                        base_ms: Some(9),
                        ..BackoffPatch::default()
                    }),
                    ..RetryPatch::default()
                }),
                ..PolicyPatch::default()
            }
        };
        let result = patch(
            Some(&old),
            0,
            input(Some(7), delta),
            64,
            profile,
            &mut charge,
            0,
        );
        assert_eq!(original(&old)?, raw);
        let actual =
            result.map_err(|error| format!("binary mode={mode_case}, phase={phase}: {error:?}"))?;
        assert_eq!(original(&actual)?, scalar_wire(8, phase, mode_case));
    }
    Ok(())
}

fn scalar_wire(generation: u64, phase: u8, mode_case: bool) -> Vec<u8> {
    let changed = phase == 2;
    let kind = if mode_case {
        atom("fixed")
    } else if changed {
        atom("linear")
    } else {
        binary(b"fixed")
    };
    let mode = if !mode_case {
        atom("parallel")
    } else if changed {
        atom("fifo")
    } else {
        binary(b"PaRaLlEl")
    };
    let retry = fields(&[(
        "backoff",
        fields(&[
            (
                "base_ms",
                integer(if changed && !mode_case { 9 } else { 17 }),
            ),
            ("jitter_pct", integer(4)),
            ("kind", kind),
            ("max_ms", integer(33)),
            ("version", vec![110, 3, 0, 1, 0, 0]),
        ]),
    )]);
    let config = fields(&[
        ("mode", mode),
        ("indexed_state_meta", vec![108, 0, 0, 0, 0, 106]),
    ]);
    let mut rows = vec![
        (atom("retry"), retry),
        (atom("states"), map(&[(binary(b"queued"), config)])),
        (atom("type"), binary(b"r")),
        (binary(b"opaque"), binary(b"\0\xffraw")),
    ];
    if phase == 1 {
        rows.push((atom("max_active_ms"), integer(3)));
    }
    envelope(generation, &map(&rows))
}

fn numeric_form(profile: PolicySchemaLimits, field: u8) -> TestResult {
    use wf_domain::policy::complete::decode_original_complete_policy_entry_portable;
    use wf_domain::policy::definition::{BackoffPatch, PolicyVersion, RetryPatch};
    use wf_domain::retention::RetentionOverride;

    let raw = numeric_wire(7, 0, field);
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode_original_complete_policy_entry_portable(&raw, profile, &mut charge)?
        .ok_or("healthy padded numeric donor did not decode")?;
    assert_eq!(
        (entry.generation(), entry.workflow_type()),
        (7, Some(b"r".as_slice()))
    );
    assert_eq!(entry.encoded(), raw);
    let old = StoredPolicyValue::Original(raw.clone());
    for phase in [1, 2] {
        let delta = if phase == 1 {
            PolicyPatch {
                version: FieldPatch::Set(PolicyVersion::Number(9)),
                ..PolicyPatch::default()
            }
        } else {
            match field {
                0 => PolicyPatch {
                    max_active_ms: FieldPatch::Set(3),
                    ..PolicyPatch::default()
                },
                1 => PolicyPatch {
                    retry: FieldPatch::Set(RetryPatch {
                        max_retries: Some(9),
                        backoff: Some(BackoffPatch {
                            base_ms: Some(9),
                            ..BackoffPatch::default()
                        }),
                        ..RetryPatch::default()
                    }),
                    ..PolicyPatch::default()
                },
                2 => PolicyPatch {
                    retention: FieldPatch::Set(RetentionOverride {
                        ttl_ms: Some(200),
                        ..RetentionOverride::default()
                    }),
                    ..PolicyPatch::default()
                },
                _ => return Err("unknown padded numeric fixture".into()),
            }
        };
        let result = patch(
            Some(&old),
            0,
            input(Some(7), delta),
            64,
            profile,
            &mut charge,
            0,
        );
        assert_eq!(original(&old)?, raw);
        let actual =
            result.map_err(|error| format!("padded field={field}, phase={phase}: {error:?}"))?;
        assert_eq!(original(&actual)?, numeric_wire(8, phase, field));
    }
    Ok(())
}

fn padded(n: u8) -> Vec<u8> {
    let mut raw = vec![110, 17, 0, n];
    raw.extend_from_slice(&[0; 16]);
    raw
}

fn numeric_wire(generation: u64, phase: u8, field: u8) -> Vec<u8> {
    let changed = phase == 2;
    let (key, value) = match field {
        0 => (
            "max_active_ms",
            if changed { integer(3) } else { padded(17) },
        ),
        1 => (
            "retry",
            fields(&[
                (
                    "backoff",
                    fields(&[
                        ("base_ms", if changed { integer(9) } else { padded(17) }),
                        ("jitter_pct", padded(17)),
                        ("kind", atom("fixed")),
                        ("max_ms", padded(50)),
                        ("version", vec![110, 3, 0, 1, 0, 0]),
                    ]),
                ),
                ("max_retries", if changed { integer(9) } else { padded(17) }),
            ]),
        ),
        2 => (
            "retention",
            fields(&[
                ("history_max_events", padded(17)),
                ("ttl_ms", if changed { integer(200) } else { padded(50) }),
                ("indexed_state_meta", vec![108, 0, 0, 0, 0, 106]),
            ]),
        ),
        _ => return Vec::new(),
    };
    let mut rows = vec![
        (atom(key), value),
        (atom("states"), map(&[])),
        (atom("type"), binary(b"r")),
        (binary(b"opaque"), binary(b"\0\xffraw")),
    ];
    if phase == 1 {
        rows.push((atom("version"), integer(9)));
    }
    envelope(generation, &map(&rows))
}
