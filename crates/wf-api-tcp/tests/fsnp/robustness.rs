//! Untrusted bytes: every truncation and single-byte corruption of a real
//! request returns a value or an error and never panics.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::request;

use super::{CLAIM_DUE, COMPLETE, COMPLETE_MANY, CREATE, CREATE_MANY};

fn parse_all(body: &[u8]) -> usize {
    [
        request::create_many(body).is_ok(),
        request::create(body).is_ok(),
        request::claim_due(FLAG_CUSTOM_PAYLOAD, body).is_ok(),
        request::complete_many(FLAG_CUSTOM_PAYLOAD, body).is_ok(),
        request::complete(body).is_ok(),
    ]
    .into_iter()
    .filter(|accepted| *accepted)
    .count()
}

#[test]
fn every_strict_prefix_of_a_request_is_refused() {
    for fixture in [CREATE_MANY, CREATE, CLAIM_DUE, COMPLETE_MANY, COMPLETE] {
        assert_eq!(
            parse_all(fixture),
            1,
            "each fixture parses as exactly one request"
        );
        // Every length near either end, and a stride through the middle of
        // the large batch fixtures, keeps the quadratic sweep fast.
        let lengths = (0..fixture.len())
            .filter(|length| *length < 1024 || length % 257 == 0 || length + 1024 >= fixture.len());
        for length in lengths {
            let prefix = fixture.get(..length).unwrap_or_default();
            assert_eq!(
                parse_all(prefix),
                0,
                "prefix of {length} bytes was accepted"
            );
        }
    }
}

fn corrupted(fixture: &[u8], position: usize, replacement: u8) -> Vec<u8> {
    let mut bytes = fixture.to_vec();
    if let Some(byte) = bytes.get_mut(position) {
        *byte = replacement;
    }
    bytes
}

#[test]
fn single_byte_corruptions_never_panic() {
    for fixture in [CREATE, CLAIM_DUE, COMPLETE] {
        for position in 0..fixture.len() {
            for replacement in [0x00, 0x05, 0x06, 0x7F, 0xFF] {
                parse_all(&corrupted(fixture, position, replacement));
            }
        }
    }
}

#[test]
fn huge_declared_lengths_are_refused_without_allocating() {
    let mut map = vec![6, 0xFF, 0xFF, 0xFF, 0xFF];
    map.extend_from_slice(&[0; 16]);
    assert_eq!(parse_all(&map), 0);
    let mut nested = Vec::new();
    for _ in 0..64 {
        nested.extend_from_slice(&[5, 0, 0, 0, 1]);
    }
    nested.push(0);
    assert_eq!(parse_all(&nested), 0);
}
