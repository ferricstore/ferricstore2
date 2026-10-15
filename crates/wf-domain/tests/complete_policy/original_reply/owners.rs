//! Borrowed donors and bounded metadata despite repeated projected payloads.

use super::{
    OriginalEtfSafety, PolicySchemaCharge, ReplyContext, TestResult, accepted, atom, binary,
    context, defaults, envelope, field, integer, map, minimal, profile, safety, set_reply,
};

#[test]
fn original_reply_borrows_donors_and_shares_large_type_defaults() -> TestResult {
    assert_descriptor_lifetime()?;
    assert_large_query()?;
    assert_repeated_target_sharing()?;
    assert_shared_backoff_rows()?;
    Ok(())
}

fn assert_descriptor_lifetime() -> TestResult {
    let dynamic = String::from("opaque");
    let atoms = [
        "flow_policy_v1",
        "type",
        "states",
        "version",
        dynamic.as_str(),
    ];
    let bytes = envelope(
        3,
        &map(&[
            (atom("type")?, binary(b"stored")?),
            (atom("states")?, map(&[])?),
            (atom("version")?, atom(&dynamic)?),
        ])?,
    )?;
    let requested_type = b"caller".to_vec();
    let query = vec![0xff, 0];
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = {
        let descriptor = ReplyContext::new(
            OriginalEtfSafety::new(&atoms, &[], None, &[]),
            defaults(),
            None,
            None,
            0,
        );
        accepted(set_reply(
            &requested_type,
            &bytes,
            Some(&query),
            descriptor,
            limits,
            &mut charge,
        ))?
    };
    assert_eq!(result.generation(), 3);
    assert!(std::ptr::eq(
        result.requested_type().as_ptr(),
        requested_type.as_ptr()
    ));
    assert!(std::ptr::eq(
        result.requested_state().ok_or("query donor")?.as_ptr(),
        query.as_ptr()
    ));
    assert_eq!(
        field(result.root(), "version", &mut charge)?.atom_bytes(),
        Some(dynamic.as_bytes())
    );
    assert_eq!(
        field(result.root(), "state", &mut charge)?.binary_bytes(),
        Some(query.as_slice())
    );
    Ok(())
}

fn assert_large_query() -> TestResult {
    let bytes = minimal(1)?;
    let query = vec![0xff; 64 * 1_024 * 1_024];
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(
        b"r",
        &bytes,
        Some(&query),
        context(),
        limits,
        &mut charge,
    ))?;
    let state = field(result.root(), "state", &mut charge)?
        .binary_bytes()
        .ok_or("large state")?;
    assert_eq!(state.len(), 67_108_864);
    assert!(std::ptr::eq(state.as_ptr(), query.as_ptr()));
    assert_eq!(result.root().map_len(), Some(11));
    assert!(charge.p() < 67_120_000);
    Ok(())
}

fn assert_repeated_target_sharing() -> TestResult {
    let names = (0..9_000).map(|n| format!("{n:04}")).collect::<Vec<_>>();
    let states = names
        .iter()
        .map(|name| Ok((binary(name.as_bytes())?, map(&[])?)))
        .collect::<TestResult<Vec<_>>>()?;
    let target = vec![b'z'; 130_000];
    let retry = map(&[(atom("exhausted_to")?, binary(&target)?)])?;
    // This explicit order makes the physical states-map span ID7. The supplied
    // identity record is represented capture input; this is no VM-order claim.
    let bytes = envelope(
        1,
        &map(&[
            (atom("type")?, binary(b"r")?),
            (atom("states")?, map(&states)?),
            (atom("retry")?, retry),
        ])?,
    )?;
    let ordinals = (0..9_000).collect::<Vec<u32>>();
    let capture = ReplyContext::new(
        safety(),
        defaults(),
        Some(&bytes),
        Some((7, &ordinals, &ordinals)),
        0,
    );
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(bytes.len(), 256_082);
    // The pinned ETF expansion is322+247*S+(B-6)*(S+1), but is never allocated.
    let states_count = u64::try_from(names.len())?;
    let target_bytes = u64::try_from(target.len())?;
    assert_eq!(
        322 + 247 * states_count + (target_bytes - 6) * (states_count + 1),
        1_172_299_316
    );
    let result = accepted(set_reply(b"r", &bytes, None, capture, limits, &mut charge))?;
    assert_eq!(charge.p(), 6_883_067);
    let root_retry = field(result.root(), "retry", &mut charge)?;
    let root_target = field(root_retry, "exhausted_to", &mut charge)?
        .binary_bytes()
        .ok_or("root target")?;
    assert_eq!(root_target, target);
    assert!(std::ptr::eq(
        root_target.as_ptr(),
        bytes[bytes.len() - target.len()..].as_ptr()
    ));
    let states = field(result.root(), "states", &mut charge)?;
    assert_eq!(states.map_len(), Some(9_000));
    let mut rows = states.map_iter().ok_or("large states iterator")?;
    let mut count = 0;
    while let Some((key, row)) = rows.next(&mut charge)? {
        assert_eq!(key.binary_bytes(), Some(names[count].as_bytes()));
        let retry = field(row, "retry", &mut charge)?;
        let value = field(retry, "exhausted_to", &mut charge)?
            .binary_bytes()
            .ok_or("row target")?;
        assert!(std::ptr::eq(value.as_ptr(), root_target.as_ptr()));
        assert_eq!(value.len(), 130_000);
        count += 1;
    }
    assert_eq!(count, 9_000);
    assert_eq!(charge.p(), 6_883_067);
    Ok(())
}

fn assert_shared_backoff_rows() -> TestResult {
    let names = (0..1_000).map(|n| format!("{n:04}")).collect::<Vec<_>>();
    let states = names
        .iter()
        .map(|name| Ok((binary(name.as_bytes())?, map(&[])?)))
        .collect::<TestResult<Vec<_>>>()?;
    let unknown = names
        .iter()
        .map(|name| Ok((binary(name.as_bytes())?, integer(7)?)))
        .collect::<TestResult<Vec<_>>>()?;
    let retry = map(&[(atom("backoff")?, map(&unknown)?)])?;
    let bytes = envelope(
        1,
        &map(&[
            (atom("type")?, binary(b"r")?),
            (atom("states")?, map(&states)?),
            (atom("retry")?, retry),
        ])?,
    )?;
    let ordinals = (0..1_000).collect::<Vec<u32>>();
    let capture = ReplyContext::new(
        safety(),
        defaults(),
        Some(&bytes),
        Some((7, &ordinals, &ordinals)),
        0,
    );
    let limits = super::PolicySchemaLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let result = accepted(set_reply(b"r", &bytes, None, capture, limits, &mut charge))?;
    assert!(charge.p() < 2 * 1_024 * 1_024);
    let states = field(result.root(), "states", &mut charge)?;
    let mut state_rows = states.map_iter().ok_or("states iterator")?;
    let mut total_states = 0;
    let mut first_backoff = None;
    while let Some((_, row)) = state_rows.next(&mut charge)? {
        let backoff = field(field(row, "retry", &mut charge)?, "backoff", &mut charge)?;
        assert_eq!(backoff.map_len(), Some(1_004));
        first_backoff.get_or_insert(backoff);
        total_states += 1;
    }
    assert_eq!(total_states, 1_000);
    let backoff = first_backoff.ok_or("first state backoff")?;
    assert_eq!(backoff.map_len(), Some(1_004));
    let mut rows = backoff.map_iter().ok_or("shared backoff iterator")?;
    let mut observed = 0;
    while let Some((key, value)) = rows.next(&mut charge)? {
        if key.binary_bytes().is_some() {
            assert_eq!(value.integer(), Some(7));
            observed += 1;
        }
    }
    assert_eq!(observed, 1_000);
    assert!(charge.p() < 2 * 1_024 * 1_024);
    Ok(())
}
