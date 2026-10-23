//! Original count and final logical-byte boundaries; these are not heap bills.

use super::{E, I, attrs, states, valid_attrs, valid_states};

#[test]
fn metadata_counts_precede_collisions_and_list_deduplication() {
    let mut keys: Vec<_> = (1..=17).map(|n| format!("{}k", " ".repeat(n))).collect();
    keys.sort();
    let rows: Vec<_> = keys
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::Other))
        .collect();
    assert!(matches!(
        attrs(I::Map(&rows)),
        Err(E::SourceReturned("ERR too many flow attributes"))
    ));
    let list = [I::Other; 17];
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(b"k"), I::List(&list))])),
        Err(E::SourceReturned("ERR flow attribute list too large"))
    ));
    let sixteen: Vec<_> = (0..16).map(|n| format!("{n:02}")).collect();
    let inner: Vec<_> = sixteen
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::Nil))
        .collect();
    assert!(valid_attrs(I::Map(&inner)));
    assert!(matches!(attrs(I::Map(&inner)), Ok(out) if out.entries.len() == 16));
    assert!(
        matches!(states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])),
        Ok(out) if out.states.len() == 1 && out.states[0].1.len() == 16)
    );
    let too_many: Vec<_> = keys
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::Nil))
        .collect();
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&too_many))])),
        Err(E::SourceReturned("ERR too many flow state_meta entries"))
    ));

    // Actual OTP 28.3.1 traversal: validation log8547963+2197/e0302f8f.
    // The collapsing map captures empty tuple values as the unsupported class.
    let state_keys: Vec<_> = (0..65).map(|n| format!("s{n:02}")).collect();
    let sixty_four: Vec<_> = [
        29, 13, 17, 58, 8, 12, 34, 54, 36, 26, 52, 4, 25, 9, 47, 59, 30, 43, 57, 14, 51, 2, 18, 45,
        15, 50, 44, 40, 55, 31, 20, 39, 0, 10, 11, 41, 24, 35, 28, 32, 37, 33, 56, 61, 7, 62, 49,
        42, 48, 21, 63, 53, 5, 27, 46, 16, 23, 6, 19, 60, 38, 22, 3, 1,
    ]
    .iter()
    .map(|&index| (I::Binary(state_keys[index].as_bytes()), I::Map(&[])))
    .collect();
    assert!(valid_states(I::Map(&sixty_four)));
    assert!(
        matches!(states(I::Map(&sixty_four)), Ok(out) if out.states.len() == 64
        && out.states.iter().all(|(_, values)| values.is_empty()))
    );
    let sixty_five: Vec<_> = [
        29, 13, 17, 58, 8, 12, 34, 54, 36, 26, 52, 4, 25, 9, 47, 59, 30, 43, 57, 14, 51, 2, 18, 45,
        15, 50, 44, 40, 55, 31, 20, 39, 0, 10, 11, 41, 24, 35, 28, 32, 37, 33, 56, 61, 7, 62, 49,
        42, 48, 21, 63, 53, 5, 27, 46, 16, 23, 6, 19, 60, 38, 22, 3, 64, 1,
    ]
    .iter()
    .map(|&index| (I::Binary(state_keys[index].as_bytes()), I::Map(&[])))
    .collect();
    assert!(!valid_states(I::Map(&sixty_five)));
    assert!(matches!(
        states(I::Map(&sixty_five)),
        Err(E::SourceReturned("ERR too many flow state_meta states"))
    ));
    let collapse_keys: Vec<_> = (0..65)
        .map(|n| format!("{}queued", " ".repeat(n)))
        .collect();
    let collapse: Vec<_> = [
        30, 36, 4, 40, 57, 2, 15, 34, 1, 33, 56, 51, 50, 28, 53, 52, 17, 59, 41, 37, 48, 13, 6, 24,
        22, 54, 62, 55, 38, 25, 26, 8, 3, 64, 18, 10, 9, 14, 32, 0, 7, 45, 5, 35, 27, 12, 29, 23,
        21, 19, 44, 16, 39, 49, 63, 11, 20, 43, 46, 31, 60, 47, 42, 58, 61,
    ]
    .iter()
    .map(|&index| (I::Binary(collapse_keys[index].as_bytes()), I::Other))
    .collect();
    assert!(matches!(
        states(I::Map(&collapse)),
        Err(E::SourceReturned("ERR too many flow state_meta states"))
    ));
}

#[test]
fn metadata_logical_totals_follow_final_values_and_exact_boundaries() {
    let names: Vec<_> = (0..8).map(|n| n.to_string()).collect();
    let payload = vec![b'x'; 255];
    let rows: Vec<_> = names
        .iter()
        .map(|k| (I::Binary(k.as_bytes()), I::Binary(&payload)))
        .collect();
    assert!(valid_attrs(I::Map(&rows)));
    assert!(matches!(attrs(I::Map(&rows)), Ok(out) if out.entries.len() == 8));
    let extra = [b'x'; 256];
    let mut over = rows.clone();
    over[0].1 = I::Binary(&extra);
    assert!(!valid_attrs(I::Map(&over)));
    assert!(matches!(
        attrs(I::Map(&over)),
        Err(E::SourceReturned("ERR flow attributes too large"))
    ));
    let x257 = [b'x'; 257];
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(b"k"), I::Binary(&x257))])),
        Err(E::SourceReturned("ERR flow attribute value too large"))
    ));
    let list = [I::Binary(&x257)];
    assert!(matches!(
        attrs(I::Map(&[(I::Binary(b"k"), I::List(&list))])),
        Err(E::SourceReturned("ERR flow attribute value too large"))
    ));
    let inner = [(I::Binary(b"k"), I::Binary(&x257))];
    assert!(matches!(
        states(I::Map(&[(I::Binary(b"s"), I::Map(&inner))])),
        Err(E::SourceReturned("ERR flow state_meta value too large"))
    ));

    // Each state contributes 1 + 16 + 254 + 15*255 = 4096 logical bytes.
    let keys: Vec<_> = (b'a'..=b'p').map(|key| [key]).collect();
    let short = [b'x'; 254];
    let inner: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            (
                I::Binary(key),
                I::Binary(if index == 0 { &short } else { &payload }),
            )
        })
        .collect();
    let exact = [
        (I::Binary(b"a"), I::Map(&inner)),
        (I::Binary(b"b"), I::Map(&inner)),
        (I::Binary(b"c"), I::Map(&inner)),
        (I::Binary(b"d"), I::Map(&inner)),
    ];
    assert!(valid_states(I::Map(&exact)));
    assert!(
        matches!(states(I::Map(&exact)), Ok(out) if out.states.len() == 4
        && out.states.iter().all(|(_, entries)| entries.len() == 16))
    );
    let mut wider = inner.clone();
    wider[0].1 = I::Binary(&payload);
    let too_large = [
        (I::Binary(b"a"), I::Map(&wider)),
        (I::Binary(b"b"), I::Map(&inner)),
        (I::Binary(b"c"), I::Map(&inner)),
        (I::Binary(b"d"), I::Map(&inner)),
    ];
    assert!(!valid_states(I::Map(&too_large)));
    assert!(matches!(
        states(I::Map(&too_large)),
        Err(E::SourceReturned("ERR flow state_meta too large"))
    ));
    let collision = [
        (I::Binary(b" a"), I::Map(&inner)),
        (I::Binary(b"a"), I::Map(&inner)),
        (I::Binary(b"b"), I::Map(&inner)),
        (I::Binary(b"c"), I::Map(&inner)),
        (I::Binary(b"d"), I::Map(&inner)),
    ];
    assert!(
        matches!(states(I::Map(&collision)), Ok(out) if out.states.len() == 4
        && out.states.iter().all(|(_, entries)| entries.len() == 16))
    );
}
