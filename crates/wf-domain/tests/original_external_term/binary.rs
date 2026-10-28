use super::{DError, I, N, TestResult, decode, fixtures as f};
use wf_domain::value::{decode_original_flow_value, prepare_original_flow_value};

#[test]
fn external_term_binary_roots_keep_storage_profiles_explicit() -> TestResult {
    f::current()?;
    for bytes in [b"".as_slice(), b"plain", b"FSV2\x02typed", b"\0\xff"] {
        let nodes = [N::Binary {
            bytes: 0..bytes.len(),
        }];
        let input = I {
            bytes,
            nodes: &nodes,
            edges: &[],
            root: 0,
        };
        let bare = f::healthy(input, f::limits())?;
        let mut expected = b"\x83m".to_vec();
        expected.extend_from_slice(&u32::try_from(bytes.len())?.to_be_bytes());
        expected.extend_from_slice(bytes);
        assert_eq!(bare.as_external_term(), expected);
        let decoded = f::decoded(
            bare.as_external_term().to_vec(),
            &f::context(),
            f::decode_limits(),
        )?;
        assert!(
            matches!(decoded.root(),Some(N::Binary {bytes:range}) if decoded.byte_range(range.clone())==Some(bytes))
        );
        let stored = prepare_original_flow_value(input, f::limits())?;
        assert!(matches!(
            decode(
                stored.into_stored_bytes(),
                &f::context(),
                f::decode_limits()
            ),
            Err(DError::InvalidFlowValue)
        ));
        let as_value = decode_original_flow_value(
            bare.into_external_term(),
            &f::context(),
            f::decode_limits(),
        )
        .map_err(|error| format!("stored-value decode: {error:?}"))?;
        assert!(
            matches!(as_value.root(),Some(N::Binary {bytes:range}) if as_value.byte_range(range.clone())==Some(expected.as_slice()))
        );
    }
    let value = f::current()?;
    let mut wrapped = b"FSV2\x02".to_vec();
    wrapped.extend_from_slice(value.as_external_term());
    assert!(decode(wrapped, &f::context(), f::decode_limits()).is_err());
    Ok(())
}
