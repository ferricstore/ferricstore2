//! The 24-byte frame header.

use wf_api_tcp::frame::{
    FLAG_CUSTOM_PAYLOAD, FrameError, HEADER_LEN, Header, MAX_BODY_BYTES, Opcode,
};

use super::TestResult;

fn request_header(opcode: u16, body_len: u32) -> [u8; HEADER_LEN] {
    let mut bytes = Vec::with_capacity(HEADER_LEN);
    bytes.extend_from_slice(b"FSNP");
    bytes.extend_from_slice(&[0x01, FLAG_CUSTOM_PAYLOAD]);
    bytes.extend_from_slice(&7_u32.to_be_bytes());
    bytes.extend_from_slice(&opcode.to_be_bytes());
    bytes.extend_from_slice(&99_u64.to_be_bytes());
    bytes.extend_from_slice(&body_len.to_be_bytes());
    let mut header = [0; HEADER_LEN];
    header.copy_from_slice(&bytes);
    header
}

#[test]
fn parses_a_request_header_and_writes_the_matching_response() -> TestResult {
    let header = Header::parse_request(request_header(0x0203, 3))?;
    assert_eq!(
        (
            header.flags,
            header.lane,
            header.opcode,
            header.request_id,
            header.body_len
        ),
        (FLAG_CUSTOM_PAYLOAD, 7, 0x0203, 99, 3)
    );
    assert_eq!(Opcode::from_code(header.opcode), Some(Opcode::FlowClaimDue));
    let mut out = Vec::new();
    header.encode_response(FLAG_CUSTOM_PAYLOAD, b"xyz", &mut out)?;
    let mut expected = request_header(0x0203, 3).to_vec();
    expected[4] = 0x81;
    expected.extend_from_slice(b"xyz");
    assert_eq!(out, expected);
    Ok(())
}

#[test]
fn refuses_bad_magic_version_and_oversized_bodies() {
    let mut magic = request_header(0x0001, 0);
    magic[0] = b'X';
    assert_eq!(Header::parse_request(magic), Err(FrameError::BadMagic));
    let mut response = request_header(0x0001, 0);
    response[4] = 0x81;
    assert_eq!(
        Header::parse_request(response),
        Err(FrameError::BadVersion(0x81))
    );
    let limit = request_header(0x0001, MAX_BODY_BYTES);
    assert!(Header::parse_request(limit).is_ok());
    let over = MAX_BODY_BYTES + 1;
    assert_eq!(
        Header::parse_request(request_header(0x0001, over)),
        Err(FrameError::TooLarge(u64::from(over)))
    );
}

#[test]
fn a_reply_longer_than_a_frame_is_refused_whole() -> TestResult {
    // Elixir: "successful responses over their byte budget become bounded
    // errors": the frame writer refuses the body, appends nothing, and the
    // connection sends `ERR reply exceeds the frame limit` instead.
    let header = Header::parse_request(request_header(0x0202, 0))?;
    let limit = usize::try_from(MAX_BODY_BYTES)?;
    let mut out = Vec::new();
    header.encode_response(0, &vec![0; limit], &mut out)?;
    assert_eq!(out.len(), HEADER_LEN + limit);
    let mut out = Vec::new();
    assert_eq!(
        header.encode_response(0, &vec![0; limit + 1], &mut out),
        Err(FrameError::TooLarge(u64::from(MAX_BODY_BYTES) + 1))
    );
    assert_eq!(out, []);
    Ok(())
}

#[test]
fn opcode_codes_match_the_native_protocol() {
    for (opcode, code) in [
        (Opcode::Hello, 0x0001),
        (Opcode::Ping, 0x0003),
        (Opcode::Quit, 0x0009),
        (Opcode::Options, 0x000B),
        (Opcode::Startup, 0x000C),
        (Opcode::Flow("FLOW.CREATE"), 0x0201),
        (Opcode::Flow("FLOW.GET"), 0x0202),
        (Opcode::FlowClaimDue, 0x0203),
        (Opcode::Flow("FLOW.COMPLETE"), 0x0204),
        (Opcode::Flow("FLOW.TRANSITION"), 0x0205),
        (Opcode::Flow("FLOW.STEP_CONTINUE"), 0x0222),
        (Opcode::FlowCreateMany, 0x020F),
        (Opcode::FlowCompleteMany, 0x0210),
        (Opcode::FlowReclaim, 0x0215),
        (Opcode::CommandExec, 0x0100),
    ] {
        assert_eq!(opcode.code(), code);
        assert_eq!(Opcode::from_code(code), Some(opcode));
    }
    // Key-value commands are not part of this workflow-only server;
    // COMMAND_EXEC carries FLOW text commands only (ADR-014 step 9c).
    assert_eq!(Opcode::from_code(0x0101), None);
}
