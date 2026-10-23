# Metadata performance review

This approved review path was created after ScoreBound `ce8e225`. Peer append
custody begins only when Root switches the exact three active review paths.
Previous feature and legacy review records remain immutable.

The finite consultation read complete pinned Attributes (463 lines), StateMeta
(418 lines) and MetadataValidation tests (61 lines). Canonical validation must
inspect borrowed data without rebuilding maps. Normalizers may borrow names,
binary values and list members; output row/list spines need ownership. Borrowing
may retain substantially larger original backing.

Raw counts precede collisions/deduplication; all rows validate even when later
overwritten. Lists retain last normalized occurrences. State collisions replace
whole inner maps. Logical totals of 2,048 and 16,384 bytes are not heap or
admission bills. Fallible reservation should follow semantic validation and use
final surviving counts; capacity, total allocation and speed claims need further
evidence. No automatic Vec-to-Box conversion is selected.

Pinned Elixir and Rust use the same 25 whitespace characters. Names require
checked UTF-8 before trimming; value bytes may remain opaque. Improper attribute
list normalization raises ArgumentError, while canonical validation returns
false. Actual original executions and the complete source roster remain pending.
