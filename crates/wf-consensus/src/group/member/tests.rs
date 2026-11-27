use super::due;

/// A member takes its next snapshot once it applied `every` entries past
/// its newest, not before.
#[test]
fn a_snapshot_is_due_every_so_many_applied_entries() {
    assert!(!due(19, 0, 20));
    assert!(due(20, 0, 20));
    assert!(due(25, 0, 20));
    assert!(!due(39, 20, 20));
    assert!(due(40, 20, 20));
    // A snapshot past the applied entries (installed from the leader) is
    // not followed by another at once.
    assert!(!due(10, 30, 20));
}
