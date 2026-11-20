//! Partition naming and slot routing identical to the Elixir server
//! (ADR-011). Vectors were computed with Python's zlib, hashlib and base64.

#![forbid(unsafe_code)]

use wf_application::commands::queue::{Partition, SLOTS, shard_for_slot};

const SDK_FLOW_ID: &[u8] = b"py-sdk-bench-850594bc816749e1a68ce3b359c810c8:flow:11681";

#[test]
fn a_flow_without_a_partition_gets_the_elixir_auto_bucket() {
    let partition = Partition::for_new_flow(SDK_FLOW_ID, None);
    assert_eq!(partition, Partition::Auto(131));
    assert_eq!(partition.name(), b"__flow_auto__:131");
    let named = Partition::for_new_flow(SDK_FLOW_ID, Some(b"tenant-a"));
    assert_eq!(named, Partition::Named(b"tenant-a".to_vec()));
    assert_eq!(named.name(), b"tenant-a");
}

#[test]
fn only_canonical_auto_bucket_names_parse_as_auto_buckets() {
    for (name, expected) in [
        (&b"__flow_auto__:0"[..], Partition::Auto(0)),
        (b"__flow_auto__:131", Partition::Auto(131)),
        (b"__flow_auto__:255", Partition::Auto(255)),
    ] {
        assert_eq!(Partition::parse(name), expected);
        assert_eq!(Partition::parse(name).name(), name);
    }
    for name in [
        &b"__flow_auto__:0131"[..],
        b"__flow_auto__:256",
        b"__flow_auto__:",
        b"__flow_auto__:1x",
        b"__flow_auto__:-1",
        b"orders",
    ] {
        assert_eq!(Partition::parse(name), Partition::Named(name.to_vec()));
    }
}

#[test]
fn slots_match_elixir_for_auto_and_named_partitions() {
    for (partition, slot) in [
        (Partition::Auto(0), 1),
        (Partition::Auto(1), 151),
        (Partition::Auto(131), 369),
        (Partition::Auto(255), 183),
        (Partition::Named(b"tenant-a".to_vec()), 914),
        (Partition::Named(b"orders/eu".to_vec()), 878),
    ] {
        assert_eq!(partition.slot(), slot, "{partition:?}");
    }
}

#[test]
fn slots_map_to_shards_in_uniform_contiguous_ranges() {
    assert_eq!(SLOTS, 1024);
    assert_eq!(shard_for_slot(0, 16), Some(0));
    assert_eq!(shard_for_slot(63, 16), Some(0));
    assert_eq!(shard_for_slot(64, 16), Some(1));
    assert_eq!(shard_for_slot(1023, 16), Some(15));
    // 1024 = 342 + 341 + 341: the first shard takes the extra slot.
    assert_eq!(shard_for_slot(341, 3), Some(0));
    assert_eq!(shard_for_slot(342, 3), Some(1));
    assert_eq!(shard_for_slot(682, 3), Some(1));
    assert_eq!(shard_for_slot(683, 3), Some(2));
    assert_eq!(shard_for_slot(1023, 3), Some(2));
    assert_eq!(shard_for_slot(1024, 16), None);
    assert_eq!(shard_for_slot(5, 0), None);
    assert_eq!(shard_for_slot(5, 1025), None);
}
