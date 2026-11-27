use super::wanted;

/// A leader asks for a snapshot newer than its own and no later than what
/// it applied: one at either bound's wrong side, or none, is declined.
#[test]
fn a_snapshot_answers_a_request_only_between_its_bounds() {
    assert!(wanted(Some(5), 4, 9));
    assert!(wanted(Some(9), 4, 9));
    assert!(!wanted(Some(4), 4, 9));
    assert!(!wanted(Some(10), 4, 9));
    assert!(!wanted(None, 0, 9));
}
