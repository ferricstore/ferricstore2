//! Held original inputs, logical prior peaks and independent final byte owners.

use super::{
    PolicySchemaCharge, Raw, TestResult, accepted, current, invoke_for_type, profile, scope,
};

#[test]
fn original_public_merge_keeps_simultaneous_and_returned_owners() -> TestResult {
    assert_donors_drop()?;
    assert_held_results()?;
    assert_opaque_type()?;
    assert_small_seed()?;
    assert_larger_seed()?;
    Ok(())
}

fn assert_small_seed() -> TestResult {
    for maximum in [86_139, 86_140] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut data = super::super::data(b"r".to_vec(), None, std::collections::BTreeMap::new());
        data.indexed_attributes = Some(vec![b"x".to_vec()]);
        let held =
            wf_domain::policy::complete::restore_complete_policy(1, data, limits, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let expected = current::minimal(1)?;
        let result = invoke_for_type(b"r", None, Raw::List(&[]), 1_000_000, limits, &mut charge);
        if maximum == 86_140 {
            accepted(&result?, 0, 1, &expected)?;
            assert_eq!((charge.g(), charge.p()), (47_928, 38_212));
        } else {
            assert_eq!(
                *result
                    .err()
                    .ok_or("seed final refusal")?
                    .downcast::<super::RawError>()
                    .map_err(|_| "seed origin")?,
                super::refusals::work(86_140, maximum, false)
            );
            assert_eq!((charge.g(), charge.p()), (47_800, 38_212));
        }
        assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    }
    Ok(())
}

fn assert_larger_seed() -> TestResult {
    for maximum in [20_765, 97_937, 97_938] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        // Old standalone Other is still supported; it does not traverse frame9.
        let list = [Raw::Nil, Raw::Nil];
        let attrs = [(Raw::Other, Raw::List(&list))];
        let held = wf_domain::policy::complete::normalize_original_complete_policy_input(
            &Raw::Binary(b"r"),
            &Raw::Map(&attrs),
            1_000_000,
            limits,
            &mut charge,
        )?
        .map_err(|_| "old seed")?;
        let pointer = held.as_ptr();
        assert_eq!((charge.g(), charge.p()), (13_726, 6_912));
        let expected = current::minimal(1)?;
        let result = invoke_for_type(b"r", None, Raw::List(&[]), 1_000_000, limits, &mut charge);
        if maximum == 97_938 {
            accepted(&result?, 0, 1, &expected)?;
            assert_eq!((charge.g(), charge.p()), (59_726, 38_212));
        } else {
            let (target, retained) = if maximum == 20_765 {
                (20_766, (13_726, 6_912))
            } else {
                (97_938, (59_598, 38_212))
            };
            assert_eq!(
                *result
                    .err()
                    .ok_or("larger seed refusal")?
                    .downcast::<super::RawError>()
                    .map_err(|_| "large seed origin")?,
                super::refusals::work(target, maximum, false)
            );
            assert_eq!((charge.g(), charge.p()), retained);
        }
        assert_eq!(held.as_ptr(), pointer);
        assert_eq!(
            held,
            b"\x83t\0\0\0\x02w\x06statest\0\0\0\0w\x04typem\0\0\0\x01r"
        );
    }
    Ok(())
}

fn assert_donors_drop() -> TestResult {
    let expected = current::minimal(2)?;
    let result = {
        let bytes = current::minimal(1)?;
        let before = bytes.clone();
        let limits = profile(2_000_000_000)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let kind = b"r".to_vec();
        let root = Vec::new();
        let result = invoke_for_type(
            &kind,
            Some(&bytes),
            Raw::List(&root),
            1_000_000,
            limits,
            &mut charge,
        )?;
        accepted(&result, 1, 2, &expected)?;
        assert_eq!(bytes, before);
        let install = result.0.as_ref().ok_or("owned installation")?;
        assert_ne!(install.encoded().as_ptr(), bytes.as_ptr());
        assert_ne!(install.policy_key().as_ptr(), kind.as_ptr());
        result
    };
    accepted(&result, 1, 2, &expected)?;
    let install = result.0.as_ref().ok_or("surviving installation")?;
    assert_eq!(install.policy_key(), b"f:{f}:policy:r");
    assert_eq!(result.1.scope(), &scope()?);
    assert_eq!(result.1.request().as_bytes(), b"q");
    assert_eq!(result.1.original_time().get(), 0);
    assert_eq!(result.1.expires_at().get(), 100);
    assert_eq!(result.1.response_version(), 1);
    assert_eq!(result.1.intent().version(), 4);
    Ok(())
}

fn assert_held_results() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let original = current::two_strings()?;
    let before = original.clone();
    let expected_first = current::minimal(1)?;
    let first = invoke_for_type(
        b"r",
        Some(&original),
        Raw::List(&[]),
        1_000_000,
        limits,
        &mut charge,
    )?;
    accepted(&first, 0, 1, &expected_first)?;
    let first_g = charge.g();
    let first_p = charge.p();
    let first_install = first.0.as_ref().ok_or("held first")?;
    let first_pointer = first_install.encoded().as_ptr();
    let response_pointer = first.1.response().as_ptr();
    let expected_second = current::minimal(2)?;
    let second = invoke_for_type(
        b"r",
        Some(first_install.encoded()),
        Raw::List(&[]),
        1_000_000,
        limits,
        &mut charge,
    )?;
    accepted(&second, 1, 2, &expected_second)?;
    assert!(charge.g() > first_g);
    assert!(charge.p() >= first_p);
    assert_eq!(first_install.encoded(), expected_first);
    assert_eq!(first_install.encoded().as_ptr(), first_pointer);
    assert_eq!(first.1.response(), expected_first);
    assert_eq!(first.1.response().as_ptr(), response_pointer);
    assert_eq!(original, before);
    assert_ne!(second.1.response().as_ptr(), first.1.response().as_ptr());
    Ok(())
}

fn assert_opaque_type() -> TestResult {
    let kind = [255_u8, 0];
    let fields = [
        (Raw::Atom("states"), Raw::Map(&[])),
        (Raw::Atom("type"), Raw::Binary(&kind)),
    ];
    let expected_first = current::envelope(1, Raw::Map(&fields))?;
    let expected_second = current::envelope(2, Raw::Map(&fields))?;
    let limits = profile(2_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = invoke_for_type(&kind, None, Raw::List(&[]), 1_000_000, limits, &mut charge)?;
    accepted(&first, 0, 1, &expected_first)?;
    let prior = first.0.as_ref().ok_or("opaque type first")?.encoded();
    let second = invoke_for_type(
        &kind,
        Some(prior),
        Raw::List(&[]),
        1_000_000,
        limits,
        &mut charge,
    )?;
    accepted(&second, 1, 2, &expected_second)?;
    assert_eq!(
        second.0.as_ref().ok_or("opaque key")?.policy_key(),
        b"f:{f}:policy:\xff\0"
    );
    Ok(())
}
