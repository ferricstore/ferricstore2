//! Direct observations of the pinned public retry and retention response recipe.

use super::{
    PolicySchemaCharge, ReadValue, ReplyContext, ReplyValue, TestResult, atom, binary, defaults,
    envelope, field, get_reply, integer, map, profile as limits, safety, set_reply,
};

#[test]
fn original_reply_preserves_pinned_type_and_state_retry_retention_maps() -> TestResult {
    let profile = limits(100_000_000)?;
    let encoded = source_policy()?;
    let mut charge = PolicySchemaCharge::new(profile);
    let context = ReplyContext::new(safety(), defaults(), None, None, 0);
    let all = set_reply(
        b"flow-policy",
        &encoded,
        None,
        context,
        profile,
        &mut charge,
    )?
    .map_err(|_| "valid pinned reply refused")?;
    assert_eq!(all.generation(), 1);
    assert_eq!(all.root().map_len(), Some(10));
    let retry = field(all.root(), "retry", &mut charge)?;
    let retention = field(all.root(), "retention", &mut charge)?;
    assert_eq!(field(retry, "max_retries", &mut charge)?.integer(), Some(5));
    assert_eq!(
        field(all.root(), "max_active_ms", &mut charge)?.integer(),
        Some(120_000)
    );
    assert_eq!(
        field(retention, "ttl_ms", &mut charge)?.integer(),
        Some(60_000)
    );
    assert!(
        retention
            .atom_field("history_hot_max_events", &mut charge)?
            .is_none()
    );
    assert_eq!(
        field(retention, "history_max_events", &mut charge)?.integer(),
        Some(512)
    );
    let states = field(all.root(), "states", &mut charge)?;
    assert_eq!(states.map_len(), Some(1));
    let mut rows = states.map_iter().ok_or("states map")?;
    let (name, state) = rows.next(&mut charge)?.ok_or("charge_card row")?;
    assert_eq!(name.binary_bytes(), Some(&b"charge_card"[..]));
    assert!(rows.next(&mut charge)?.is_none());
    let state_retry = field(state, "retry", &mut charge)?;
    let backoff = field(state_retry, "backoff", &mut charge)?;
    let state_retention = field(state, "retention", &mut charge)?;
    assert_eq!(
        field(state_retry, "max_retries", &mut charge)?.integer(),
        Some(2)
    );
    assert_eq!(
        field(backoff, "kind", &mut charge)?.atom_bytes(),
        Some(&b"fixed"[..])
    );
    assert_eq!(
        field(state_retry, "exhausted_to", &mut charge)?.binary_bytes(),
        Some(&b"payment_failed"[..])
    );
    assert_eq!(
        field(state_retention, "ttl_ms", &mut charge)?.integer(),
        Some(30_000)
    );
    assert!(
        state_retention
            .atom_field("history_hot_max_events", &mut charge)?
            .is_none()
    );
    assert_eq!(
        field(state_retention, "history_max_events", &mut charge)?.integer(),
        Some(256)
    );
    assert_eq!(retention.map_len(), Some(2));
    assert_eq!(state_retention.map_len(), Some(2));
    drop(all);
    let selected = get_reply(
        b"flow-policy",
        ReadValue::Binary(&encoded),
        Some(b"charge_card"),
        ReplyContext::new(safety(), defaults(), None, None, 0),
        profile,
        &mut charge,
    )?
    .map_err(|_| "valid selected reply refused")?;
    let root = selected.root();
    let retention = field(root, "retention", &mut charge)?;
    assert_selected_recipe(root, &mut charge)?;
    assert!(
        retention
            .atom_field("history_hot_max_events", &mut charge)?
            .is_none()
    );
    assert_eq!(retention.map_len(), Some(2));
    assert_eq!(selected.requested_state(), Some(&b"charge_card"[..]));
    Ok(())
}

fn assert_selected_recipe(root: ReplyValue<'_, '_>, charge: &mut PolicySchemaCharge) -> TestResult {
    let retry = field(root, "retry", charge)?;
    let retention = field(root, "retention", charge)?;
    let backoff = field(retry, "backoff", charge)?;
    assert_eq!(
        field(root, "max_active_ms", charge)?.integer(),
        Some(120_000)
    );
    assert_eq!(field(retry, "max_retries", charge)?.integer(), Some(2));
    assert_eq!(field(backoff, "base_ms", charge)?.integer(), Some(10_000));
    assert_eq!(
        field(retry, "exhausted_to", charge)?.binary_bytes(),
        Some(&b"payment_failed"[..])
    );
    assert_eq!(field(retention, "ttl_ms", charge)?.integer(), Some(30_000));
    assert_eq!(
        field(retention, "history_max_events", charge)?.integer(),
        Some(256)
    );
    Ok(())
}

fn source_policy() -> TestResult<Vec<u8>> {
    let backoff = map(&[
        (atom("kind")?, atom("fixed")?),
        (atom("base_ms")?, integer(10_000)?),
        (atom("max_ms")?, integer(30_000)?),
        (atom("jitter_pct")?, integer(0)?),
    ])?;
    let retry = map(&[
        (atom("max_retries")?, integer(5)?),
        (atom("backoff")?, backoff),
        (atom("exhausted_to")?, binary(b"failed")?),
    ])?;
    let retention = map(&[
        (atom("ttl_ms")?, integer(60_000)?),
        (atom("history_max_events")?, integer(512)?),
    ])?;
    let state = map(&[
        (
            atom("retry")?,
            map(&[
                (atom("max_retries")?, integer(2)?),
                (atom("exhausted_to")?, binary(b"payment_failed")?),
            ])?,
        ),
        (
            atom("retention")?,
            map(&[
                (atom("ttl_ms")?, integer(30_000)?),
                (atom("history_max_events")?, integer(256)?),
            ])?,
        ),
    ])?;
    let policy = map(&[
        (atom("type")?, binary(b"flow-policy")?),
        (atom("max_active_ms")?, integer(120_000)?),
        (atom("retry")?, retry),
        (atom("retention")?, retention),
        (atom("states")?, map(&[(binary(b"charge_card")?, state)])?),
    ])?;
    envelope(1, &policy)
}
