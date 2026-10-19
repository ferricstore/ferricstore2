use super::{
    BLANK, BLANK_SHA, MEMBER, MEMBER_SHA, TestResult, hex, membership, nodes, packed, scope, stamp,
};
use wf_domain::workflow::control::EntryStamp;
use wf_ports::consensus::raft_membership_owned_bytes;
use wf_ports::consensus::{
    AppliedRaftMembership, RaftMembership, RaftMembershipError, RaftMembershipLimits,
};
use wf_ports::log::{
    decode_raft_membership_body, encode_raft_blank, encode_raft_membership, measure_control_entry,
};

#[test]
fn membership_frames_preserve_complete_joint_configs() -> TestResult {
    let (g, r) = scope()?;
    let limits = RaftMembershipLimits::default();
    let value = membership()?;
    assert_eq!(raft_membership_owned_bytes(&value, limits)?, 4661);
    let blank = encode_raft_blank(&g, &r, EntryStamp::Unstamped, limits, 2061)?;
    assert_eq!(blank.as_bytes(), hex(BLANK)?);
    assert_eq!(blank.digest().as_bytes().as_slice(), hex(BLANK_SHA)?);
    let encoded = encode_raft_membership(&g, &r, EntryStamp::Unstamped, &value, limits, 6843)?;
    assert_eq!(encoded.as_bytes(), hex(MEMBER)?);
    assert_eq!(encoded.digest().as_bytes().as_slice(), hex(MEMBER_SHA)?);
    assert_eq!(
        measure_control_entry(encoded.as_bytes(), &g, &r, limits)?,
        (EntryStamp::Unstamped, Some(4661))
    );
    assert_eq!(
        measure_control_entry(blank.as_bytes(), &g, &r, limits)?,
        (EntryStamp::Unstamped, None)
    );
    for clock in [EntryStamp::Unstamped, stamp(20, 3, 19)?] {
        let bytes = packed(true, clock)?;
        let decoded = decode_raft_membership_body(
            &bytes,
            &g,
            &r,
            limits,
            7355 + usize::from(clock != EntryStamp::Unstamped) * 18,
        )?;
        drop(bytes);
        assert_eq!(decoded.configs(), value.configs());
        assert_eq!(decoded.nodes(), value.nodes());
        assert_eq!(
            encode_raft_membership(&g, &r, clock, &value, limits, 10000)?.as_bytes(),
            packed(true, clock)?
        );
        assert_eq!(
            encode_raft_blank(&g, &r, clock, limits, 10000)?.as_bytes(),
            packed(false, clock)?
        );
    }
    let initial = AppliedRaftMembership::initial();
    assert!(initial.entry().is_none());
    assert!(initial.log_id().is_none());
    assert!(initial.stamp().is_none());
    let configs: &[Vec<u64>] = &[];
    let nodes: &[(u64, String)] = &[];
    assert_eq!(initial.membership().configs(), configs);
    assert_eq!(initial.membership().nodes(), nodes);
    assert_eq!(
        raft_membership_owned_bytes(initial.membership(), limits)?,
        1024
    );
    assert!(matches!(
        encode_raft_membership(
            &g,
            &r,
            EntryStamp::Unstamped,
            initial.membership(),
            limits,
            10000
        ),
        Err(RaftMembershipError::Malformed)
    ));
    transfer_owners()
}

fn transfer_owners() -> TestResult {
    let mut configs = Vec::with_capacity(7);
    configs.push(vec![1, 2]);
    let mut node_data = nodes();
    node_data.reserve_exact(9);
    let before = (
        configs.as_ptr(),
        configs[0].as_ptr(),
        configs.capacity(),
        node_data.as_ptr(),
        node_data[0].1.as_ptr(),
        node_data.capacity(),
    );
    let value = RaftMembership::new(configs, node_data, RaftMembershipLimits::default())?;
    let (configs, node_data) = value.into_parts();
    assert_eq!(
        (
            configs.as_ptr(),
            configs[0].as_ptr(),
            configs.capacity(),
            node_data.as_ptr(),
            node_data[0].1.as_ptr(),
            node_data.capacity()
        ),
        before
    );
    Ok(())
}

#[test]
fn membership_profiles_and_malformed_frames_are_bounded() -> TestResult {
    let (g, r) = scope()?;
    let limits = RaftMembershipLimits::default();
    let value = membership()?;
    let body = hex(MEMBER)?;
    let args = [3, 6, 4, 2, 134, 4661];
    for field in 0..6 {
        let mut zero = args;
        zero[field] = 0;
        assert!(matches!(
            RaftMembershipLimits::new(zero[0], zero[1], zero[2], zero[3], zero[4], zero[5]),
            Err(RaftMembershipError::InvalidLimits)
        ));
        let mut small = args;
        small[field] -= 1;
        let profile =
            RaftMembershipLimits::new(small[0], small[1], small[2], small[3], small[4], small[5])?;
        assert!(matches!(
            encode_raft_membership(&g, &r, EntryStamp::Unstamped, &value, profile, 10000),
            Err(RaftMembershipError::Limit)
        ));
    }
    let exact = RaftMembershipLimits::new(3, 6, 4, 2, 134, 4661)?;
    assert_eq!(
        encode_raft_membership(&g, &r, EntryStamp::Unstamped, &value, exact, 6843)?.as_bytes(),
        body
    );
    assert!(matches!(
        encode_raft_membership(&g, &r, EntryStamp::Unstamped, &value, exact, 6842),
        Err(RaftMembershipError::Limit)
    ));
    assert!(matches!(
        decode_raft_membership_body(&body, &g, &r, exact, 7354),
        Err(RaftMembershipError::Limit)
    ));
    assert!(matches!(
        encode_raft_blank(&g, &r, EntryStamp::Unstamped, limits, 2060),
        Err(RaftMembershipError::Limit)
    ));
    assert!(matches!(
        measure_control_entry(
            &body,
            &wf_types::identity::GroupId::new(b"other".to_vec())?,
            &r,
            limits
        ),
        Err(RaftMembershipError::NamespaceMismatch)
    ));
    for end in 0..body.len() {
        assert!(
            measure_control_entry(&body[..end], &g, &r, limits).is_err(),
            "prefix{end}"
        );
    }
    for offset in [0, 5, 12, 20, 28, 84, 92, 132] {
        let mut bad = body.clone();
        bad[offset] = 255;
        assert!(
            measure_control_entry(&bad, &g, &r, limits).is_err(),
            "offset{offset}"
        );
    }
    let mut bad = body.clone();
    bad.push(0);
    assert!(measure_control_entry(&bad, &g, &r, limits).is_err());
    for member in [false, true] {
        let mut bad = packed(member, stamp(20, 3, 19)?)?;
        bad[23..31].copy_from_slice(&21_u64.to_be_bytes());
        assert!(matches!(
            measure_control_entry(&bad, &g, &r, limits),
            Err(RaftMembershipError::Malformed)
        ));
    }
    invalid_models()
}

fn invalid_models() -> TestResult {
    let limits = RaftMembershipLimits::default();
    for configs in [
        vec![],
        vec![vec![]],
        vec![vec![2, 1]],
        vec![vec![1, 1]],
        vec![vec![5]],
    ] {
        assert!(matches!(
            RaftMembership::new(configs, nodes(), limits),
            Err(RaftMembershipError::Malformed)
        ));
    }
    for data in [
        vec![(1, "a".into()), (1, "b".into())],
        vec![(2, "a".into()), (1, "b".into())],
    ] {
        assert!(matches!(
            RaftMembership::new(vec![vec![1]], data, limits),
            Err(RaftMembershipError::Malformed)
        ));
    }
    let zero = RaftMembership::new(
        vec![vec![0, u64::MAX]],
        vec![(0, String::new()), (u64::MAX, String::new())],
        limits,
    )?;
    assert_eq!(zero.configs(), [vec![0, u64::MAX]]);
    let huge = RaftMembershipLimits::new(
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
        usize::MAX,
    )?;
    assert_eq!(raft_membership_owned_bytes(&zero, huge)?, 2576);
    Ok(())
}
