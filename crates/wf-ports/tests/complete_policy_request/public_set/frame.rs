//! Independently frozen wire bytes, mathematical intent and meaningful controls.

use super::{
    AdmissionLimits, EMPTY_PATCH, EncodingError, OriginalPolicySetMapOrders, PolicySchemaCharge,
    Raw, RequestError, TestResult, digest, empty_orders, frame, hex, log, profile,
    verify_original_policy_set_frame,
};

#[test]
fn public_set_frame_binds_mode_query_guard_and_canonical_patch() -> TestResult {
    let empty = hex(
        "4657435000010900000001740000000167000000017a00000001710004a58aa50ce9877a9b71cdcb801d1e50508236cf78c2a2a4427d4844cd133042160000000000000000000000000000000000000000004c0000001457462d494e54454e54004f524947494e414c2d504f4c4943592d5345540056310000000001740000000e663a7b667d3a706f6c6963793a720000000000001104000000000000000000000011000000010000000c000000140000003b00000000",
    )?;
    let nonempty = hex(
        "4657435000010900000001740000000167000000017a000000017100043e4f759a7af9f457f562ed26122221cd722b237f5a0d3e65f6dbf9aaeca33474000000000000000000000000000000000000000000770000001c57462d494e54454e54004f524947494e414c2d504f4c4943592d5345540056310000000001740000000e663a7b667d3a706f6c6963793a72010100000002ff000100000000000000070000002e04000000000000000100000019000000230000002e0000000a0200000001780000000b01000000000107000000010000000c0000001c00000049000000010000000000000000",
    )?;
    assert_eq!((empty.len(), nonempty.len()), (183, 234));
    let orders = empty_orders();
    assert_eq!(frame(Raw::List(&[]), &orders)?, empty);
    let false_pair = [Raw::Atom("replace"), Raw::Atom("false")];
    let false_opts = [Raw::Tuple(&false_pair)];
    assert_eq!(frame(Raw::List(&false_opts), &orders)?, empty);
    assert_nonempty(&nonempty)?;
    assert_control_identity(&empty)?;
    assert_malformed(&empty)?;
    Ok(())
}

fn assert_nonempty(expected: &[u8]) -> TestResult {
    let patch = hex(
        "0000002e04000000000000000100000019000000230000002e0000000a0200000001780000000b01000000000107",
    )?;
    let orders =
        OriginalPolicySetMapOrders::new(digest(&patch), None, &[(0, &[0], &[0])], &[], &[]);
    let replace = [Raw::Atom("replace"), Raw::Atom("true")];
    let state = [Raw::Atom("state"), Raw::Binary(&[255, 0])];
    let generation = [Raw::Atom("expected_generation"), Raw::Integer(7)];
    let field = [Raw::Atom("x"), Raw::Integer(7)];
    let opts = [
        Raw::Tuple(&replace),
        Raw::Tuple(&state),
        Raw::Tuple(&generation),
        Raw::Tuple(&field),
    ];
    assert_eq!(frame(Raw::List(&opts), &orders)?, expected);
    let padded = [
        Raw::Atom("x"),
        Raw::BigInteger {
            negative: false,
            little_endian: &[7, 0, 0],
        },
    ];
    let aliases = [
        Raw::Tuple(&padded),
        Raw::Tuple(&generation),
        Raw::Tuple(&state),
        Raw::Tuple(&replace),
    ];
    assert_eq!(frame(Raw::List(&aliases), &orders)?, expected);
    let mut digits = vec![0; 262_145];
    digits[0] = 7;
    let wide = [
        Raw::Atom("x"),
        Raw::BigInteger {
            negative: false,
            little_endian: &digits,
        },
    ];
    let aliases = [
        Raw::Tuple(&wide),
        Raw::Tuple(&generation),
        Raw::Tuple(&state),
        Raw::Tuple(&replace),
    ];
    // Router and wire use mathematical magnitude, not the caller's padding.
    assert_eq!(frame(Raw::List(&aliases), &orders)?, expected);
    Ok(())
}

fn assert_control_identity(empty: &[u8]) -> TestResult {
    let orders = empty_orders();
    for (name, value) in [
        ("replace", Raw::Atom("true")),
        ("state", Raw::Binary(b"")),
        ("state", Raw::Binary(&[255, 0])),
        ("expected_generation", Raw::Integer(0)),
    ] {
        let pair = [Raw::Atom(name), value];
        let opts = [Raw::Tuple(&pair)];
        let changed = frame(Raw::List(&opts), &orders)?;
        assert_ne!(&changed[29..61], &empty[29..61]);
    }
    for value in [Raw::Nil, Raw::Atom("nil")] {
        let state = [Raw::Atom("state"), value];
        let generation = [Raw::Atom("expected_generation"), value];
        let opts = [Raw::Tuple(&state), Raw::Tuple(&generation)];
        assert_eq!(frame(Raw::List(&opts), &orders)?, empty);
    }
    Ok(())
}

fn assert_malformed(original: &[u8]) -> TestResult {
    let limits = profile(2_000_000_000)?;
    let orders = empty_orders();
    for (offset, value, error) in [
        (6, 8, EncodingError::UnsupportedTag),
        (28, 3, EncodingError::UnsupportedVersion),
        (162, 0, EncodingError::Malformed),
    ] {
        let mut bytes = original.to_vec();
        bytes[offset] = value;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            verify_original_policy_set_frame(
                &bytes,
                &orders,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RequestError::Encoding(error))
        );
    }
    let mut bytes = original.to_vec();
    bytes.push(0);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        verify_original_policy_set_frame(
            &bytes,
            &orders,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RequestError::Encoding(EncodingError::TrailingBytes))
    );
    Ok(())
}

pub(super) fn assert_reply_profile() -> TestResult {
    let orders = empty_orders();
    for reply in [&[255, 0][..], &[][..]] {
        let pair = [Raw::Atom("state"), Raw::Binary(reply)];
        let opts = [Raw::Tuple(&pair)];
        let bytes = frame(Raw::List(&opts), &orders)?;
        assert_eq!(bytes.len(), 187 + reply.len());
    }
    let reply = vec![255; 262_145];
    let pair = [Raw::Atom("state"), Raw::Binary(&reply)];
    let opts = [Raw::Tuple(&pair)];
    let bytes = frame(Raw::List(&opts), &orders)?;
    assert_eq!(bytes.len(), 262_332);
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let verified = verify_original_policy_set_frame(
        &bytes,
        &orders,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(verified.header(log(0)).intent.version(), 4);
    assert_eq!(&verified.original_intent()[62..262_207], reply);
    assert_eq!(&verified.original_intent()[262_208..], EMPTY_PATCH);
    assert_maximum_reply_profile()?;
    Ok(())
}

fn assert_maximum_reply_profile() -> TestResult {
    let limits = profile(2_000_000_000)?;
    // Original visible bytes: type r1 + state5 + replyB + x1; Integer adds0.
    // Wire magnitude contributes16 derived bytes, beyond the original raw cap.
    let reply = vec![255; 67_108_857];
    let pair = [Raw::Atom("state"), Raw::Binary(&reply)];
    let integer = [Raw::Atom("x"), Raw::Integer(i128::MAX)];
    let opts = [Raw::Tuple(&pair), Raw::Tuple(&integer)];
    let mut patch = hex(
        "0000003d04000000000000000100000019000000230000003d0000000a0200000001780000001a010000000010",
    )?;
    patch.extend_from_slice(&i128::MAX.to_le_bytes());
    assert_eq!(patch.len(), 61);
    let prepared = super::prepared(Raw::List(&opts))?;
    let orders =
        OriginalPolicySetMapOrders::new(digest(&patch), None, &[(0, &[0], &[0])], &[], &[]);
    let admission = AdmissionLimits::new(1_048_576, 134_217_728, 1_000)?;
    let input = super::OriginalPolicySetFrameInput::new(
        super::scope()?,
        super::RequestId::new(b"q".to_vec())?,
        super::stamp(0)?,
        &prepared,
        &orders,
    );
    let mut charge = PolicySchemaCharge::new(limits);
    let bytes = super::prepare_original_policy_set_frame(input, limits, admission, &mut charge)?;
    assert_eq!(bytes.as_bytes().len(), 67_109_096);
    let mut charge = PolicySchemaCharge::new(limits);
    let verified = verify_original_policy_set_frame(
        bytes.as_bytes(),
        &orders,
        limits,
        admission,
        &mut charge,
    )?;
    assert_eq!(verified.original_intent().len(), 67_108_981);
    assert_eq!(&verified.original_intent()[62..67_108_919], reply);
    assert_eq!(verified.original_intent()[67_108_919], 0);
    assert_eq!(&verified.original_intent()[67_108_920..], patch);
    Ok(())
}
