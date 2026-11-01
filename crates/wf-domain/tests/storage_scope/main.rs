//! Pinned `StorageScope` behavior; original source at commit 66586c596e27.
#![forbid(unsafe_code)]
use wf_domain::metadata::OriginalMetadataInput as I;
use wf_domain::metadata::scope::*;
mod bounds;
mod system;

fn limits() -> OriginalScopeLimits {
    OriginalScopeLimits {
        maximum_output_bytes: 100_000,
        maximum_work_units: 10_000_000,
        maximum_temporary_bytes: 1_000_000,
    }
}

fn physical(partition: &[u8], scope: &[u8]) -> Vec<u8> {
    let mut result = b"\0\xfeflow-scope\0".to_vec();
    assert!(scope.len() <= 65_535);
    let length = scope.len().to_be_bytes();
    result.extend_from_slice(&length[length.len() - 2..]);
    result.extend_from_slice(scope);
    result.extend_from_slice(partition);
    result
}

#[test]
fn same_scope_is_idempotent_and_foreign_scope_wraps_again() -> Result<(), String> {
    let scope = 42_u64.to_be_bytes();
    let expected = physical(b"tenant-a", &scope);
    let result = original_physical_partition(b"tenant-a", Some(&scope), limits())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(result.value.as_ref(), expected);
    let again = original_physical_partition(&expected, Some(&scope), limits())
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(again.value, std::borrow::Cow::Borrowed(_)));
    assert_eq!(again.value.as_ref(), expected);
    let foreign = 99_u64.to_be_bytes();
    let wrapped = original_physical_partition(&expected, Some(&foreign), limits())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(wrapped.value.as_ref(), physical(&expected, &foreign));
    assert_eq!(
        original_logical_partition(&expected, &scope, limits())
            .map_err(|error| format!("{error:?}"))?
            .value,
        b"tenant-a"
    );
    Ok(())
}

#[test]
fn record_empty_binary_and_nil_follow_their_distinct_source_overloads() -> Result<(), String> {
    assert!(original_physical_partition(b"", None, limits()).is_err());
    let empty = original_record_physical_partition(Some(I::Binary(b"")), None, limits())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(empty.value.ok_or("expected source value")?.as_ref(), b"");
    assert!(
        original_record_physical_partition(Some(I::Nil), None, limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .is_none()
    );
    assert!(original_record_physical_partition(None, None, limits()).is_err());
    Ok(())
}
