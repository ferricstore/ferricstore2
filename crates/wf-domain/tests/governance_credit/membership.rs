use super::TestResult;
use wf_domain::governance::catalog::{CatalogMembershipAction as A, CatalogMembershipChange as C};

#[test]
fn catalog_membership_validates_raw_occurrences_and_preserves_first_binary_members() -> TestResult {
    let members: &[&[u8]] = &[b"z", b"a\0b", b"z", b"a", b"a\0b"];
    let command = C {
        catalog: b"limit",
        action: A::Register,
        members,
    };
    command.validate(8192)?;
    assert_eq!(
        command.unique_members().collect::<Vec<_>>(),
        vec![b"z".as_slice(), b"a\0b", b"a"]
    );
    let original = command
        .unique_members()
        .next()
        .ok_or("missing first member")?;
    assert!(std::ptr::eq(original.as_ptr(), members[0].as_ptr()));
    let duplicates = [b"a".as_slice(); 257];
    assert!(
        C {
            members: &duplicates,
            ..command
        }
        .validate(8192)
        .is_err()
    );
    assert!(
        C {
            members: &[b"a", b""],
            ..command
        }
        .validate(8192)
        .is_err()
    );
    assert!(
        C {
            catalog: b"12345",
            ..command
        }
        .validate(4)
        .is_err()
    );
    assert!(command.validate(0).is_err());
    Ok(())
}

#[test]
fn catalog_membership_empty_and_unregistration_rules_match_source() -> TestResult {
    for action in [A::Register, A::Unregister] {
        let empty = C {
            catalog: b"",
            action,
            members: &[],
        };
        empty.validate(8192)?;
        assert_eq!(empty.unique_members().count(), 0);
    }
    let members: &[&[u8]] = &[b"", b"a", b""];
    let command = C {
        catalog: b"c",
        action: A::Unregister,
        members,
    };
    command.validate(8192)?;
    assert_eq!(
        command.unique_members().collect::<Vec<_>>(),
        vec![b"".as_slice(), b"a"]
    );
    assert_eq!(
        C {
            action: A::Register,
            ..command
        }
        .validate(8192)
        .err()
        .ok_or("missing rejection")?
        .to_string(),
        "ERR invalid flow governance catalog registration"
    );
    let oversized = [b"a".as_slice(); 257];
    assert_eq!(
        C {
            members: &oversized,
            ..command
        }
        .validate(8192)
        .err()
        .ok_or("missing rejection")?
        .to_string(),
        "ERR invalid flow governance catalog unregistration"
    );
    Ok(())
}
