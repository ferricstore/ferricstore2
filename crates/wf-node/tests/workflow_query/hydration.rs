//! Current record-to-values composition over actual native publication/read.
use super::*;
mod fixtures;
mod ports;
mod resources;
mod values;
use fixtures::*;
use wf_application::queries::query_strong_workflow_record_with_values as hydrated;
use wf_domain::history::values::{HistoryValue, HistoryValueField as F, HistoryValueKind as K};
use wf_domain::value::OriginalValueNode;

#[test]
fn record_values_hydrates_actual_stored_payload_and_stored_size() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-live", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 4)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(fixture.value_reads, 1);
    assert!(result.record().record().is_some());
    let writes = result.value_writes();
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].entry_index, 0);
    assert_eq!(writes[0].field, F::Value(K::Payload));
    let HistoryValue::Decoded(value) = &writes[0].value else {
        return Err("decoded payload".into());
    };
    assert!(matches!(
        value.root(),
        Some(OriginalValueNode::Binary { .. })
    ));
    assert_eq!(value.encoded(), b"data");
    assert_eq!(writes[1].field, F::Size(K::Payload));
    assert!(matches!(writes[1].value, HistoryValue::StoredSize(4)));
    fixture.finish()
}

#[test]
fn record_values_omits_actual_stored_payload_at_the_requested_byte_threshold() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-omit", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 3)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(fixture.value_reads, 1);
    assert_eq!(result.value_writes().len(), 2);
    assert_eq!(result.value_writes()[0].field, F::Omitted(K::Payload));
    assert!(matches!(
        result.value_writes()[0].value,
        HistoryValue::Boolean(true)
    ));
    assert_eq!(result.value_writes()[1].field, F::Size(K::Payload));
    assert!(matches!(
        result.value_writes()[1].value,
        HistoryValue::StoredSize(4)
    ));
    fixture.finish()
}

#[test]
fn record_values_disabled_return_does_not_enter_value_acquisition() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-disabled", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, false, 4)?,
        &context(),
        value_limits(),
    )?;
    assert!(result.record().record().is_some());
    assert!(result.value_writes().is_empty());
    assert_eq!(fixture.value_reads, 0);
    fixture.finish()
}
