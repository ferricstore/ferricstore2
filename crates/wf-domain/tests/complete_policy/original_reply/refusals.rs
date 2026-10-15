//! Source-derived primitive streams, live SUM admissions and retryable views.

use super::{
    EncodingError, OriginalEtfSafety, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    Reply, ReplyContext, ReplyError, TestResult, accepted, atom, binary, defaults, envelope, field,
    list, map, minimal, profile, set_reply, tuple,
};

const HEADER: &[u64] = &[128, 1, 1, 128, 142, 134, 132];
const LEX: &[u64] = &[
    1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 6, 148, 140, 128, 1, 4, 128,
    1, 1, 4, 146, 138, 136, 128, 1, 4, 1,
];
const IDENTITY: &[u64] = &[128, 128, 128, 128, 138, 138];
const ENVELOPE: &[u64] = &[128, 156, 128, 128];
const PREFLIGHT: &[u64] = &[128, 128, 128, 6, 139, 140, 128, 128, 4, 137, 138, 144, 128];
const DEPRECATED: &[u64] = &[128, 134, 139, 132, 137, 128, 134, 140, 128, 128];
const TYPE: &[u64] = &[128, 134, 138, 132, 136, 128, 128];
const PROJECTION: &[u64] = &[
    128, 128, 138, 128, 140, 640, 136, 128, 256, 128, 139, 128, 145, 128, 147, 128, 137, 128, 139,
    128, 128, 141, 128, 143, 128, 128, 128, 150, 128, 152, 128, 128, 150, 128, 152, 128, 142, 128,
    144, 128, 128, 128, 128, 128,
];
const LEX_TWO: &[u64] = &[
    1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 6, 148, 140, 128, 1, 4, 128,
    1, 4, 1, 128, 1, 4, 128, 1, 4, 1, 128, 1, 4, 128, 1, 1, 4, 146, 138, 136, 128, 1, 4, 1,
];
const IDENTITY_TWO: &[u64] = &[128, 128, 128, 128, 128, 138, 138, 128, 128, 128, 138, 138];
const PREFLIGHT_TWO: &[u64] = &[
    128, 128, 128, 6, 139, 140, 128, 128, 1, 134, 135, 141, 128, 128, 1, 134, 135, 141, 128, 128,
    4, 137, 138, 144, 128,
];
const DEPRECATED_TWO: &[u64] = &[
    128, 134, 139, 132, 137, 128, 134, 140, 128, 128, 128, 128, 128, 128, 128, 128,
];
const LEX_COMBINED: &[u64] = &[
    1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 18, 160, 152, 150, 164, 128,
    1, 4, 128, 1, 4, 1, 128, 1, 128, 1, 1, 6, 148, 140, 128, 1, 4, 128, 1, 1, 128, 1, 4, 1, 128, 1,
    4, 128, 1, 1, 128, 1, 4, 1, 128, 1, 4, 128, 1, 128, 1, 1, 4, 146, 138, 136, 128, 1, 4, 1,
];
const IDENTITY_COMBINED: &[u64] = &[128, 128, 128, 128, 128, 128, 138, 150, 152, 152, 138];
const PREFLIGHT_COMBINED: &[u64] = &[
    128, 128, 128, 18, 151, 152, 158, 128, 128, 6, 139, 140, 128, 128, 4, 137, 138, 144, 128,
];
const DEPRECATED_COMBINED: &[u64] = &[
    128, 146, 151, 134, 139, 132, 137, 128, 146, 152, 134, 140, 128,
];
const TYPE_COMBINED: &[u64] = &[128, 146, 150, 134, 138, 132, 136, 128, 128];
const PROJECTION_COMBINED: &[u64] = &[
    128, 128, 140, 10368, 288, 128, 256, 128, 141, 128, 139, 128, 147, 128, 159, 128, 139, 128,
    151, 128, 128, 143, 128, 155, 128, 128, 128, 152, 128, 164, 128, 128, 129, 129, 129, 128, 129,
    128, 128, 128, 152, 128, 164, 128, 144, 128, 156, 128, 128, 128, 128, 128, 128, 128, 128, 128,
    128, 128, 128, 138, 16, 256, 256, 138, 128, 128, 128,
];

#[test]
fn original_reply_prepays_each_constructor_and_observation_operation() -> TestResult {
    let bytes = minimal(1)?;
    assert_events(&bytes, None, narrow(), &minimum_events(), 18_655, 2_492)?;
    let two = two_states()?;
    let permutations = [0, 1];
    assert_events(
        &two,
        Some((5, &permutations, &permutations)),
        narrow(),
        &native_events(),
        26_383,
        4_114,
    )?;
    // Honest post-initial-RED/pre-SUT supplement: actual inline writes and
    // list heapsort/LAST compaction, independently derived before execution.
    let combined = combined()?;
    assert_eq!(combined.len(), 110);
    let safety = OriginalEtfSafety::new(
        &["flow_policy_v1", "states", "type", "indexed_attributes"],
        &[],
        None,
        &[],
    );
    assert_events(&combined, None, safety, &combined_events(), 41_578, 13_415)?;
    assert_accessor_events(&bytes)?;
    assert_matching_account(&bytes)?;
    assert_native_refusals(&two)?;
    assert_iterator_retry()?;
    assert_continuous_owners(&bytes)?;
    Ok(())
}

fn minimum_events() -> Vec<(u64, u64)> {
    let mut out = vec![(128, 0)];
    append(&mut out, HEADER);
    append(&mut out, LEX);
    out.push((0, 692));
    append(&mut out, &[384, 136, 128]);
    for group in [LEX, IDENTITY, ENVELOPE, PREFLIGHT, DEPRECATED, TYPE] {
        append(&mut out, group);
    }
    out.push((0, 884));
    append(&mut out, &[192, 32]);
    out.extend(std::iter::repeat_n((128, 0), 12));
    append(&mut out, &PROJECTION[..5]);
    out.push((0, 2_492));
    append(&mut out, &PROJECTION[5..]);
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 18_655);
    out
}

fn native_events() -> Vec<(u64, u64)> {
    let mut out = vec![(128, 0)];
    append(&mut out, HEADER);
    append(&mut out, LEX_TWO);
    out.push((0, 1_098));
    append(&mut out, &[512, 144, 128]);
    for group in [
        LEX_TWO,
        IDENTITY_TWO,
        ENVELOPE,
        PREFLIGHT_TWO,
        DEPRECATED_TWO,
        TYPE,
    ] {
        append(&mut out, group);
    }
    out.push((0, 1_322));
    append(&mut out, &[224, 48]);
    out.extend(std::iter::repeat_n((128, 0), 16));
    append(&mut out, &PROJECTION[..5]);
    out.push((0, 4_034));
    append(&mut out, &[1_664, 152]);
    append(&mut out, &PROJECTION[7..PROJECTION.len() - 2]);
    // Enum guard is already the final128 above; two fresh nonatom candidates
    // precede the reached native binding, whose safe rank order is4,16,4.
    append(&mut out, &[128, 128, 128, 128, 128, 274, 16, 16, 16]);
    out.push((0, 4_114));
    append(&mut out, &[4, 16, 4, 4, 16, 4]);
    out.extend(std::iter::repeat_n((128, 0), 8));
    append(&mut out, &[128, 128]);
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 26_383);
    out
}

fn append(out: &mut Vec<(u64, u64)>, values: &[u64]) {
    out.extend(values.iter().map(|&fee| (fee, 0)));
}

fn combined_events() -> Vec<(u64, u64)> {
    let mut out = vec![(128, 0)];
    append(&mut out, HEADER);
    append(&mut out, &[146]);
    append(&mut out, LEX_COMBINED);
    out.push((0, 1_519));
    append(&mut out, &[736, 140, 128]);
    for group in [
        LEX_COMBINED,
        IDENTITY_COMBINED,
        ENVELOPE,
        PREFLIGHT_COMBINED,
        DEPRECATED_COMBINED,
        TYPE_COMBINED,
    ] {
        append(&mut out, group);
    }
    out.push((0, 1_799));
    append(&mut out, &[280, 76]);
    out.extend(std::iter::repeat_n((128, 0), 34));
    append(&mut out, &PROJECTION_COMBINED[..3]);
    out.push((0, 13_415));
    append(&mut out, &PROJECTION_COMBINED[3..]);
    assert_eq!(out.iter().map(|event| event.0).sum::<u64>(), 41_578);
    out
}

fn assert_events(
    bytes: &[u8],
    order: Option<(u32, &[u32], &[u32])>,
    safety: OriginalEtfSafety<'_>,
    events: &[(u64, u64)],
    final_g: u64,
    final_p: u64,
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for (i, &(fee, peak)) in events.iter().enumerate() {
        let (next_g, next_p) = (g + fee, p.max(peak));
        let target = next_g + next_p;
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let context = ReplyContext::new(safety, defaults(), order.map(|_| bytes), order, 0);
        let result = set_reply(b"r", bytes, None, context, limits, &mut charge);
        assert!(
            matches!(result, Err(error) if error == work(target, target - 1)),
            "tight event{i}"
        );
        assert_eq!((charge.g(), charge.p()), (g, p), "tight event{i}");
        let limits = profile(target)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = set_reply(b"r", bytes, None, context, limits, &mut charge);
        if let Some(&(following, following_peak)) = events.get(i + 1) {
            let attempted = next_g + following + next_p.max(following_peak);
            assert!(
                matches!(result, Err(error) if error == work(attempted, target)),
                "exact event{i}"
            );
        } else {
            let value = accepted(result)?;
            assert_eq!(value.generation(), 1);
            assert_eq!(value.root().map_len(), Some(10));
        }
        assert_eq!((charge.g(), charge.p()), (next_g, next_p), "exact event{i}");
        (g, p) = (next_g, next_p);
    }
    assert_eq!((g, p), (final_g, final_p));
    Ok(())
}

fn run<'input>(
    bytes: &'input [u8],
    order: Option<(u32, &'input [u32], &'input [u32])>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Reply<'input>, super::ReplyRejection>, ReplyError> {
    set_reply(
        b"r",
        bytes,
        None,
        ReplyContext::new(narrow(), defaults(), order.map(|_| bytes), order, 0),
        limits,
        charge,
    )
}

const fn narrow() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(&["flow_policy_v1", "states", "type"], &[], None, &[])
}

fn work(bytes: u64, maximum: u64) -> ReplyError {
    ReplyError::Encoding(EncodingError::Schema(PolicySchemaError::WorkBytes {
        bytes,
        maximum,
    }))
}

fn assert_accessor_events(bytes: &[u8]) -> TestResult {
    let mut iter = Vec::new();
    for _ in 0..10 {
        iter.extend([128, 128]);
    }
    iter.push(128);
    let operations = [
        vec![128, 139, 145, 142],
        iter,
        vec![128, 137, 143, 140, 146, 138, 128, 146, 142, 128, 136],
    ];
    for (kind, events) in operations.iter().enumerate() {
        let mut paid = 0;
        for &fee in events {
            let target = 18_655 + paid + fee + 2_492;
            let limits = profile(target - 1)?;
            let mut charge = PolicySchemaCharge::new(limits);
            let owner = accepted(run(bytes, None, limits, &mut charge))?;
            assert_eq!(
                perform(&owner, kind, &mut charge),
                Err(EncodingError::Schema(PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum: target - 1
                }))
            );
            assert_eq!((charge.g(), charge.p()), (18_655 + paid, 2_492));
            paid += fee;
        }
        let limits = profile(18_655 + paid + 2_492)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let owner = accepted(run(bytes, None, limits, &mut charge))?;
        perform(&owner, kind, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (18_655 + paid, 2_492));
    }
    Ok(())
}

fn perform(
    owner: &Reply<'_>,
    kind: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), EncodingError> {
    match kind {
        0 => {
            owner.root().atom_field("version", charge)?;
        }
        1 => {
            let mut rows = owner.root().map_iter().ok_or(EncodingError::Malformed)?;
            while rows.next(charge)?.is_some() {}
        }
        _ => {
            let retry = owner
                .root()
                .atom_field("retry", charge)?
                .ok_or(EncodingError::Malformed)?;
            let backoff = retry
                .atom_field("backoff", charge)?
                .ok_or(EncodingError::Malformed)?;
            backoff.atom_field("kind", charge)?;
        }
    }
    Ok(())
}

fn assert_matching_account(bytes: &[u8]) -> TestResult {
    let limits = profile(1_000_000)?;
    let mut paid = PolicySchemaCharge::new(limits);
    let owner = accepted(run(bytes, None, limits, &mut paid))?;
    let mut rows = owner.root().map_iter().ok_or("root map")?;
    let different = profile(999_999)?;
    let mut wrong = PolicySchemaCharge::new(different);
    assert!(matches!(
        rows.next(&mut wrong),
        Err(EncodingError::Schema(PolicySchemaError::ProfileMismatch))
    ));
    assert_eq!((wrong.g(), wrong.p()), (0, 0));
    let mut fresh = PolicySchemaCharge::new(limits);
    let (key, _) = rows.next(&mut fresh)?.ok_or("first row after mismatch")?;
    assert_eq!(key.atom_bytes(), Some(&b"type"[..]));
    assert_eq!((fresh.g(), fresh.p()), (256, 2_492));
    Ok(())
}

fn two_states() -> TestResult<Vec<u8>> {
    envelope(
        1,
        &map(&[
            (
                atom("states")?,
                map(&[(binary(b"a")?, map(&[])?), (binary(b"b")?, map(&[])?)])?,
            ),
            (atom("type")?, binary(b"r")?),
        ])?,
    )
}

fn combined() -> TestResult<Vec<u8>> {
    let pair = tuple(&[binary(b"a")?, map(&[])?])?;
    envelope(
        1,
        &map(&[
            (atom("indexed_attributes")?, list(&[binary(b"a")?], None)?),
            (atom("states")?, list(&[pair.clone(), pair], None)?),
            (atom("type")?, binary(b"r")?),
        ])?,
    )
}

fn assert_native_refusals(bytes: &[u8]) -> TestResult {
    assert_eq!(bytes.len(), 73);
    let p = [0, 1];
    let q = [1, 0];
    for context in [
        ReplyContext::new(narrow(), defaults(), None, Some((5, &p, &p)), 0),
        ReplyContext::new(narrow(), defaults(), Some(b"wrong"), Some((5, &p, &p)), 0),
        ReplyContext::new(narrow(), defaults(), Some(bytes), None, 0),
        ReplyContext::new(narrow(), defaults(), Some(bytes), Some((6, &p, &p)), 0),
        ReplyContext::new(narrow(), defaults(), Some(bytes), Some((5, &p[..1], &p)), 0),
        ReplyContext::new(narrow(), defaults(), Some(bytes), Some((5, &[2, 0], &p)), 0),
        ReplyContext::new(narrow(), defaults(), Some(bytes), Some((5, &p, &q)), 0),
    ] {
        let limits = profile(1_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert!(matches!(
            set_reply(b"r", bytes, None, context, limits, &mut charge),
            Err(ReplyError::Encoding(EncodingError::Schema(
                PolicySchemaError::InvalidAuthority
            )))
        ));
    }
    Ok(())
}

fn seed(charge: &mut PolicySchemaCharge, limits: PolicySchemaLimits) -> TestResult {
    let mut data = super::super::data(b"r".to_vec(), None, std::collections::BTreeMap::new());
    data.indexed_attributes = Some(vec![b"x".to_vec()]);
    let value = wf_domain::policy::complete::restore_complete_policy(1, data, limits, charge)?;
    assert_eq!(value.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    Ok(())
}

fn assert_iterator_retry() -> TestResult {
    let local = map(&[(
        atom("retry")?,
        map(&[(atom("backoff")?, map(&[(atom("kind")?, atom("nil")?)])?)])?,
    )])?;
    let bytes = envelope(
        1,
        &map(&[
            (atom("type")?, binary(b"r")?),
            (atom("states")?, map(&[(binary(b"queued")?, local)])?),
            (
                atom("retry")?,
                map(&[(atom("backoff")?, map(&[(atom("kind")?, atom("fixed")?)])?)])?,
            ),
        ])?,
    )?;
    assert_eq!(bytes.len(), 143);
    // N22/K9/D5/B2: V143+type1+query6+R31984+R4304+S2384=4822.
    // Three results cost384,256,256. Fourth skips a type-shadowed key under
    // [128,128,128,136], then refuses before local candidate128.
    let limits = profile(199_165)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for _ in 0..100 {
        seed(&mut charge, limits)?;
    }
    assert_eq!((charge.g(), charge.p()), (192_800, 64));
    let mut construction = PolicySchemaCharge::new(limits);
    let owner = accepted(set_reply(
        b"r",
        &bytes,
        Some(b"queued"),
        super::context(),
        limits,
        &mut construction,
    ))?;
    let backoff = field(
        field(owner.root(), "retry", &mut construction)?,
        "backoff",
        &mut construction,
    )?;
    let mut rows = backoff.map_iter().ok_or("backoff map")?;
    for expected in [b"base_ms".as_slice(), b"max_ms", b"jitter_pct"] {
        let (key, _) = rows.next(&mut charge)?.ok_or("default row")?;
        assert_eq!(key.atom_bytes(), Some(expected));
    }
    assert_eq!(
        rows.next(&mut charge).err(),
        Some(EncodingError::Schema(PolicySchemaError::WorkBytes {
            bytes: 199_166,
            maximum: 199_165
        }))
    );
    assert_eq!((charge.g(), charge.p()), (194_216, 4_822));
    let mut fresh = PolicySchemaCharge::new(limits);
    let (key, value) = rows.next(&mut fresh)?.ok_or("retry local row")?;
    assert_eq!(key.atom_bytes(), Some(&b"kind"[..]));
    assert_eq!(value.atom_bytes(), Some(&b"nil"[..]));
    assert_eq!((fresh.g(), fresh.p()), (648, 4_822));
    Ok(())
}

fn assert_continuous_owners(bytes: &[u8]) -> TestResult {
    // Each dormant minimal owner retains R3640+R4192+State512+word8+fixed1024
    // =2376; shared input52 and the active64D stack are counted only once.
    for maximum in [63_208, 63_209] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let first = accepted(run(bytes, None, limits, &mut charge))?;
        let second = accepted(set_reply(
            b"r",
            bytes,
            None,
            ReplyContext::new(narrow(), defaults(), None, None, 2_376),
            limits,
            &mut charge,
        ))?;
        let third = set_reply(
            b"r",
            bytes,
            None,
            ReplyContext::new(narrow(), defaults(), None, None, 4_752),
            limits,
            &mut charge,
        );
        if maximum == 63_209 {
            let third = accepted(third)?;
            assert_eq!(third.generation(), 1);
            assert_eq!((charge.g(), charge.p()), (55_965, 7_244));
        } else {
            assert!(matches!(third, Err(error) if error == work(63_209, 63_208)));
            assert_eq!((charge.g(), charge.p()), (55_837, 7_244));
        }
        assert_eq!((first.generation(), second.generation()), (1, 1));
    }
    Ok(())
}
