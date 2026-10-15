//! Real returned owners survive their frame, preparation, current and catalogs.

use super::{
    AdmissionLimits, BTreeMap, ControlState, DispatchDecision, GateView, OriginalEtfSafety,
    PolicySchemaCharge, Raw, ReadValue, RetainedStatus, TestResult, accepted, apply, empty_orders,
    frame, minimal, profile, retained, scope, stamp, view,
};

#[test]
fn public_set_parent_owns_installation_and_retained_response_after_donors_drop() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let expected = minimal(1)?;
    let plan = {
        let orders = empty_orders();
        let bytes = frame(Raw::List(&[]), &orders)?;
        let policies = BTreeMap::new();
        let marks = BTreeMap::new();
        let atoms = ["flow_policy_v1".to_owned(), "type".to_owned()];
        let names = atoms.iter().map(String::as_str).collect::<Vec<_>>();
        let supplied = view(
            GateView {
                scope: &namespace,
                control: ControlState::initial(stamp(0)?),
                outcomes: &outcomes,
            },
            &policies,
            &marks,
            &marks,
            OriginalEtfSafety::new(&names, &[], None, &[]),
        );
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &supplied,
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&plan, 0, 1, &expected)?;
        let DispatchDecision::Fresh((Some(install), out)) = plan.decision() else {
            return Err("owned installation".into());
        };
        assert_ne!(install.policy_key().as_ptr(), bytes[129..143].as_ptr());
        assert_ne!(out.response().as_ptr(), bytes.as_ptr());
        plan
    };
    accepted(&plan, 0, 1, &expected)?;
    let out = retained(&plan)?;
    assert_eq!(out.scope(), &namespace);
    assert_eq!(out.request().as_bytes(), b"q");
    assert_eq!(out.original_time().get(), 0);
    assert_eq!(out.expires_at().get(), 100);
    assert_eq!(out.response_version(), 1);
    assert_eq!(out.status(), RetainedStatus::Accepted);
    assert_eq!(out.response(), expected);
    assert_present_current_owners()?;
    Ok(())
}

fn assert_present_current_owners() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let outcomes = BTreeMap::new();
    let expected = minimal(2)?;
    let plan = {
        let current = minimal(1)?;
        // Current preorder spans: root map3, empty states map5; logical key
        // order is states then type. These are supplied SERVER-view facts.
        let current_records = [(3, &[0_u32, 1][..], &[0_u32, 1][..]), (5, &[][..], &[][..])];
        let merged_records = [(0, &[0_u32, 1][..], &[0_u32, 1][..])];
        let orders = super::OriginalPolicySetMapOrders::new(
            super::digest(super::EMPTY_PATCH),
            Some(&current),
            &[(0, &[], &[])],
            &current_records,
            &merged_records,
        );
        let bytes = frame(Raw::List(&[]), &orders)?;
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&current))]);
        let marks = BTreeMap::new();
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(
                GateView {
                    scope: &namespace,
                    control: ControlState::initial(stamp(0)?),
                    outcomes: &outcomes,
                },
                &policies,
                &marks,
                &marks,
                super::safety(),
            ),
            &bytes,
            &orders,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&plan, 1, 2, &expected)?;
        plan
    };
    accepted(&plan, 1, 2, &expected)
}
