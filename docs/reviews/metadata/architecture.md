# Metadata architecture review

Root created this approved feature review home after ScoreBound checkpoint
`ce8e225` and its full eleven-check result. The ScoreBound and legacy role
records remain immutable at their original paths. Only the three exact
`docs/reviews/metadata/{architecture,performance,domain}.md` paths become active
append-only peer notes at the coordinator's explicit custody switch.

The proposed next family is the complete pinned MetadataValidation test: three
cases, 28 expanded observations, async:true and no local setup or flow module
tag. Validators and both complete normalizers are required. Source creation is
still pending the exact input, output, error, API, helper and test roster.

The selected direction is a metadata-specific input view. Shared
`RawFlowPolicyValue` and its closed policy/ETF/admission profiles stay unchanged.
Floats, arbitrary integers, atom aliases and proper/improper lists must remain
explicit. Raised source exceptions, returned business errors and operational
reservation failures require distinct outcomes.

A future new metadata root also requires its named architecture classification,
ADR-002 rationale and a meaningful negative purity/graph fixture. This document
does not authorize those source or policy artifacts.

ARCH META-CANONICAL1 exact staged precreation selection, 2026-10-24. APPROVE the concrete roster below for Root to record and separately release; no source artifact has yet been created. Complete pinned Attributes463LF/14405B/8e64f947a61499291a487c437d0d288c43da2816219bfd165679c621818c700a, StateMeta418/12980/5ea53243bdd1e46b0833cd87993fd14e2e74d68ccce638de3a3806978a8b2469 and MetadataValidation61/2241/3e9092011e7da37267b71ac1ceb068241e1290f4d5248799cac405b38837fba7 were actually whole-read. Selected functions are the four canonical predicates and BOTH complete normalizers, not an invalid-UTF8/error-only subset. Three source cases ca42c6b45ed50200/6fbaf005c2b4dc11/d482759cd391b8c3 contain28 expanded observations19/6/3, async:true with NO flow tag or local setup. Record/index/options wrapper functions elsewhere in these modules are not represented as passing by this batch.

Actually whole/hash-read Root's original packet8536830+9477/f3f037a65265c96c08188ea4f25688391223b81facf7854bd85af5bab738f19f: original3PASS and23 edge groups establish improper-list raise versus guard-false, alias/list last occurrences, opaque binaries, input-count precedence, overwritten-row errors, whole-inner-map replacement, NBSP versus ZWS, reserved-name distinction, i64 ends/overflow/2^200, negative-zero bits and2048/2049. Five missing Keys index-helper compiler warnings remain visible outside the selected paths; none was suppressed. Root reports terminal0/1877 unchanged8676221d and zero excluded/skipped. This is standalone macOS evidence, not full Mix/project/profile/Linux validation.

Use a COMPLETE metadata-only capture; shared RawFlowPolicyValue remains unchanged. Actual Raw enum/value/preflight, closed ETF layout/writer, public-set metric and outer-frame/preflight/order paths were inspected. A shared Raw::Float edit would require explicit treatment across those closed profiles and exhaustive classifiers; Float-as-Other would erase the class. No such expansion, ETF adapter, public DTO or transport integration is selected.

Exact domain footprint, all under crates/wf-domain: NEW src/metadata.rs; NEW src/metadata/attributes.rs; NEW src/metadata/state_meta.rs; NEW tests/metadata_validation.rs; EXISTING src/lib.rs adds ONLY `pub mod metadata;`. Parent metadata.rs owns the following models, shared private helpers and private children; it reexports the six functions. Child attributes.rs owns four attribute APIs; state_meta.rs owns two state APIs. No dependency, callback, trait implementation, general term decoder, source adapter or extra file is approved.

Exact public models (all fields shown are public unless the enum payload already exposes them):
```rust
#[derive(Clone, Copy, Debug)]
pub enum OriginalMetadataInput<'rows, 'leaf: 'rows> {
    Nil,
    Boolean(bool),
    Integer(i128),
    BigInteger { negative: bool, little_endian: &'leaf [u8] },
    Float(f64),
    Atom(&'leaf str),
    Binary(&'leaf [u8]),
    Map(&'rows [(Self, Self)]),
    List(&'rows [Self]),
    ImproperList { prefix: &'rows [Self], tail: &'rows Self },
    Other,
}
#[derive(Clone, Copy, Debug)]
pub enum OriginalMetadataScalar<'leaf> {
    Nil,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    Binary(&'leaf [u8]),
}
#[derive(Debug)]
pub enum OriginalAttributeValue<'leaf> {
    Scalar(OriginalMetadataScalar<'leaf>),
    List(Vec<&'leaf [u8]>),
}
#[derive(Debug)]
pub struct OriginalAttributes<'leaf> {
    pub entries: Vec<(&'leaf str, OriginalAttributeValue<'leaf>)>,
}
#[derive(Debug)]
pub struct OriginalStateMeta<'leaf> {
    pub states: Vec<(&'leaf str, Vec<(&'leaf str, OriginalMetadataScalar<'leaf>)>)>,
}
#[derive(Debug)]
pub enum OriginalMetadataError {
    SourceReturned(&'static str),
    RaisedArgumentError,
    ReserveFailed(std::collections::TryReserveError),
}
```
ReserveFailed carries the REAL try_reserve error; this explicitly refines DOMAIN's proposed unit variant. No Display/Error/Default/Clone of owned spines, Eq/Hash, constructors, accessors, conversion aliases or proof wrappers are selected. Public fields/variants are ordinary values that callers can construct incorrectly; only successful normalizer outputs establish canonical data. Nonfinite typed float controls are rejected by the source exponent rule; they do not prove that a nonfinite BEAM term was captured.

Acquisition preconditions are explicit: immutable, finite, acyclic captured descriptors; every Map is an actual source map with unique keys under Erlang MAP-KEY exact equality, in its actual Enum.reduce order. This is not loose numeric ==: integer1 and float1.0 differ as map keys. Nil/Atom("nil"), Boolean/Atom("true"/"false"), and equal mathematical integer representations are aliases of the same source keys and cannot appear as separate keys of one captured map. BigInteger permits padding/negative zero and represents arbitrary magnitude; it is never collapsed into Other. Proper lists are flattened into List, including mathematically proper fragmented spines. ImproperList has a nonempty actual prefix and an actual non-list terminal tail; Nil atom is NOT the [] list terminator. Other covers only remaining classes observationally identical across these six functions, never floats/integers/atoms/maps/lists. Acquiring/flattening such a view is outside this helper batch and is no incoming-root/current-value authority.

Two lifetimes are mandatory. Map/List/Improper descriptors use 'rows, while atom/binary/magnitude backing uses 'leaf. Normalizers return only Output<'leaf>, allowing temporary map/list descriptor arrays to drop before result use. Every leaf-extraction helper and builder must propagate 'leaf explicitly; no &Input/receiver elision or convenience helper may unify it with 'rows. Actual outputs own only row/list spines, copy integers/floats, borrow trimmed names and binary/atom bytes, and retain no descriptor container. Borrowing may retain larger original backing; output lifetimes remain conservatively tied to the shared leaf lifetime. Return Vec rows in ascending index of their surviving LAST source occurrence; inner maps use the same convention. Attribute list duplicates are removed after atom-to-byte conversion with LAST original occurrences retained, e.g[a,b,a,c,b]->[a,c,b]. This is a concrete Rust output convention, not BEAM map traversal provenance; later adapters must not recast it as captured source reduce order.

Exact six functions; the first four receive #[must_use] and return bool:
- attributes.rs: valid_original_attributes(value: OriginalMetadataInput<'_, '_>);
- attributes.rs: valid_original_attribute_scalar(value: OriginalMetadataInput<'_, '_>);
- attributes.rs: valid_original_attribute_list(value: OriginalMetadataInput<'_, '_>);
- state_meta.rs: valid_original_state_meta(value: OriginalMetadataInput<'_, '_>).
Normalizers, without redundant Result must-use attributes:
```rust
pub fn normalize_original_attributes<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalAttributes<'leaf>, OriginalMetadataError>;
pub fn normalize_original_state_meta<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalStateMeta<'leaf>, OriginalMetadataError>;
```
Canonical checks inspect borrowed input without rebuilding maps. Canonical outer roots require maps; normalizers accept Nil aliases as empty outer maps, and state normalization accepts Nil aliases as empty inner maps. Scalars preserve nil/bools/i64/finite f64/opaque binaries; other atoms normalize to their spelling bytes. In attribute LISTS all atom spellings, including nil/true/false, become bytes. Canonical lists require nonempty unique binaries of bounded size, not atoms. Scalar and list binary payloads need no UTF8 or trimming.

BODY-only private roster is exactly NameKind::{Attribute,State,StateKey}, deriving Clone/Copy, plus these19 helpers. Below I/S/E denote the already named input/scalar/error for explanation ONLY, not new Rust type aliases; Rows denotes a borrowed slice of (I,I).
Common metadata.rs eight: atom_spelling1(I)->Option<&'leaf str>; integer_i641(I)->Option<i64>; normalize_name2(I,NameKind)->Result<&'leaf str,E>; canonical_name2(I,NameKind)->bool; normalize_scalar2(I,attributes:bool)->Result<S,E>; canonical_scalar1(I)->bool; scalar_size1(S)->usize; is_last_name3(Rows,index:usize,NameKind)->Result<bool,E>.
Attributes six: validate_entries1(Rows)->Result<(),E>; attribute_value_size1(I)->Result<usize,E>; list_binary1(I)->Result<&'leaf[u8],E>; is_last_list_value2(&[I],index:usize)->Result<bool,E>; build_attribute_value1(I)->Result<OriginalAttributeValue<'leaf>,E>; build_attributes1(Rows)->Result<OriginalAttributes<'leaf>,E>.
StateMeta five: validate_states1(Rows)->Result<(),E>; entry_map_size1(I)->Result<usize,E>; canonical_entry_map_size1(I)->Option<usize>; build_entry_map1(I)->Result<Vec<(&'leaf str,S)>,E>; build_state_meta1(Rows)->Result<OriginalStateMeta<'leaf>,E>.
Numeric suffixes above are ARITY notation, NOT literal Rust identifiers. No additional helper is inferred. Eight BODY-only private usize constants in metadata.rs: MAX_ATTRIBUTES=16, MAX_NAME_BYTES=64, MAX_VALUE_BYTES=256, MAX_ATTRIBUTE_LIST_VALUES=16, MAX_ATTRIBUTE_BYTES=2048, MAX_STATES=64, MAX_STATE_ENTRIES=16, MAX_STATE_META_BYTES=16384. Use native i64 bounds rather than another model/constant pair.

Source order is binding: root raw counts BEFORE collisions, inner state raw count BEFORE entries, proper-list length BEFORE dedup. Validate EVERY raw row in its captured order, including overwritten rows: normalize each name then its value; state name precedes complete inner-map normalization. The first full semantic pass allocates no output spines. Only afterward determine final winner counts/totals and reserve. A survivor helper must not inspect a future bad key before an earlier bad value during that first pass. Repeated normalized state names replace the WHOLE inner map, never merge. Logical total is computed from final winners, after collisions/dedup, without precharging dropped values or rejecting a valid collision outcome based on an earlier larger value. Source count limits still apply before that collapse.

Names require full checked UTF8, trim, then empty/byte-length/reserved checks in source order. All name limits are64 bytes AFTER trim; attributes/inner keys reject __, state names allow it. PERF actually compared pinned Elixir1.19.5 String.Break/PropList with Rust1.99 str/char tables: identical25 White_Space points0009..000D,0020,0085,00A0,1680,2000..200A,2028,2029,202F,205F,3000. Checked str.trim is approved under these pins; FEFF/200B/180E/2060 are not trimmed; no NFC/casefold. Source finite-float exponent and signed-i64 tests are exact. Invalid UTF8 names and all source diagnostics preserve the complete literal bytes/context from the read modules.

Improper handling is not narrowed: Attributes.valid_list guard fails to false when length fails; normalizing an improper attribute VALUE list raises ArgumentError even when its prefix exceeds16 or tail is nil atom. A proper outer list containing an improper MEMBER instead returns the original list-member string error. StateMeta list values, proper or improper, return the scalar diagnosis. Distinguish source-returned, source-raised and operational reserve lanes without Rust panic or copied BEAM stack/message. Source [erlang.erl](https://github.com/erlang/otp/blob/aa5b1d3366762ff8f420f2aa6a52604b481530c9/erts/preloaded/src/erlang.erl) documents is_list/length; actual original edge packet confirms these distinctions. [Elixir whitespace](https://github.com/elixir-lang/elixir/blob/v1.19.5/lib/elixir/unicode/PropList.txt) and [Rust whitespace](https://github.com/rust-lang/rust/blob/1.99.0/library/core/src/unicode/unicode_data.rs) support PERF's pinned trim comparison.

Fallible output policy: after ALL source semantics including final logical totals pass, use Vec::new plus try_reserve_exact of the final surviving count for EACH actually owned outer/inner/list spine, then push planned borrowed references/copied scalars. No temporary set/map collector or Vec-to-Box/shrink conversion is selected. Canonical checks can use bounded binary pairwise scans (list<=16); generic Eq/Hash/callback semantics are unnecessary. Empty outputs need no row allocation. Requested count is not allocator-provided capacity; do not assert equality. Output maxima are16 attribute rows and256 list refs, or64 states and1024 scalar rows. Logical totals2048/16384 exclude container/allocator/backing costs and are not admission/memory bills. No new allocator fault harness, synthetic capacity error, OOM execution, allocator-call/total-allocation bound, latency or gain claim is approved. Real reserve errors propagate; their failure branch may remain unexercised. Huge padded name/binary backing can stay borrowed after visible-size limits pass.

Exact flat integration roster, twelve functions and NO helpers/fixture module/files:
- attributes_identify_canonical_persisted_metadata_without_rebuilding_it
- state_metadata_identifies_canonical_scalar_only_persisted_metadata
- normalization_rejects_names_that_cannot_be_addressed_by_fql
- metadata_integer_and_float_classes_preserve_exact_boundaries
- attribute_normalization_preserves_atoms_opaque_binaries_and_last_list_occurrences
- metadata_names_preserve_unicode_trim_errors_and_reserved_rules
- metadata_counts_precede_collisions_and_list_deduplication
- metadata_validates_overwritten_rows_and_uses_last_normalized_key
- state_metadata_collisions_replace_whole_inner_maps
- metadata_logical_totals_follow_final_values_and_exact_boundaries
- metadata_list_classes_preserve_improper_argument_error_and_guard_refusal
- metadata_outputs_borrow_leaves_and_keep_input_graphs_unchanged
The first three retain ALL28 source observations in order, including all seven scalar-loop inputs and exact canonical shapes; source normalization's error-pattern assertions remain complete, with exact errors additionally checked in derived controls. Derived tests cover both normalizer successes, i64/BigInteger padding/negative zero/overflow, finite/nonfinite f64 and signed-zero bits, all atom aliases, opaque/empty binaries, exact count/byte/value/name/list limits and first-error ordering. Unicode controls include trimmed and excluded code points. Collision fixtures must be authentic unique source-key maps in real captured order, never fabricated duplicate raw rows. Final ownership test must return BOTH outputs from a scope holding temporary map/list descriptor arrays, then use results outside that scope; leaf owners live outside. Assert nonempty/exact output sizes before pointer checks, exact trimmed-name/opaque binary/list-member pointer identities and unchanged backing. No descriptor lifetime is allowed to escape; a vacuous zip/all is insufficient. All tests remain frozen after actual compiled R0 SAVE.

Root-owned classification footprint is also explicitly approved: EXISTING architecture/policy.toml adds ONLY packages.wf-domain.modules metadata=true (no edges/waiver); EXISTING docs/adr/002-project-foundation.md adds pure captured metadata rationale; NEW crates/wf-architecture-tests/src/tests/metadata_policy.rs; EXISTING crates/wf-architecture-tests/src/tests.rs adds ONLY mod metadata_policy. Exactly one new private test metadata_module_classification_preserves_purity_and_forbidden_edges, using existing CheckResult/source_check/metadata/package_mut/policy/root/graph helpers and json macro. Read the actual three metadata source homes and test pure captured f64/byte/trim controls; reject env/fs/io/async/global state per source_check, and forbidden declared ports/runtime edges via graph metadata rather than pretending the source checker owns dependency edges. Existing helper metadata() is not shadowed by the chosen metadata_policy module name. No checker implementation, dependency permission or new helper/fixture/type is selected. An actual unclassified positive failure may precede metadata=true; distinguish it from business R0 and from technical read/compile failures.

Stages are exact. R0 creates the models/error, parent children/reexports and lib seam, FOUR public bool functions returning false, TWO public normalizers always SourceReturned("ERR flow attributes must be a map") / SourceReturned("ERR flow state_meta must be a map"), and the twelve complete tests. These six rejecting functions live immediately in their intended children; there are NO empty child shells, NameKind, private-helper shells or unused limit constants. Root separately owns the classification/ADR/test writes. Compile the callable target, execute genuine behavioral RED (some existing error/refute controls may already pass), retain reached/masked assertions honestly, run required checks and SAVE before Root separately releases BODY. Later BODY replaces only those six functions/docs and adds exactly the19 helpers/NameKind/eight limits in the approved three source homes. No broader API follows from the feature assignment.

Production files stay400 physicalLF, integration/tooling files600 and functions100 after ordinary formatting, including comments/blanks. DOMAIN's330/300/300/550 projections are estimates only; if the flat test or a function cannot fit naturally, STOP and consult a concrete responsibility move before creating another file/helper or packing assertions. Type aliases/extra accessors are not silently authorized by a lint. Preserve test-source identity/ordering and the user-required whole selected function behavior.

Direct reciprocal consultation completed: DOMAIN accepts the two-lifetime refinement, nineteen helpers/eight constants BODY-only, real TryReserveError and exact original9477 observations; PERF independently accepts leaf-only extraction, last-survivor convention, semantic-before-reserve order and the meaningful descriptor-drop compiler test. Neither peer created source/notes/workloads for this consultation. Root original execution is separate from the future Rust tests. Exact three review docs were independently approved and Root-created after ScoreBound ce8e225/all11; active exemptions are only these three metadata files. ScoreBound/legacy role records remain immutable, preserving offsets/hashes; Root coordination stays at its existing path. This append changes ONLY metadata/architecture.md. No source/model/test/policy/ledger, VM/compiler/Git or probe was created/executed by ARCH. Root must record this concrete selection and release a designated disjoint writer; this is no extra acknowledgment loop. Full source profiles, Linux, public openingR/E-T/backing/settlement and the complete goal remain open. Finite precreation consultation stops normally.

ARCH META-CANONICAL1 precreation geometry delta, superseding ONLY the flat test path in1281+18633/eae20d16. DOMAIN's complete in-memory12-test draft cannot keep every selected boundary/ownership/improper control within the flat600LF limit after ordinary formatting. APPROVE responsibility grouping before creation; no coverage removal, helper addition, type/API/source-body change or packed assertion is selected. Own read-only inventory confirms the old flat target and all four proposed target files are absent.

Replace the approved but uncreated crates/wf-domain/tests/metadata_validation.rs with exactly:
- crates/wf-domain/tests/metadata_validation/main.rs: common imports, crate forbid(unsafe_code), private mod values/mod normalization/mod bounds seams, and the three original cases attributes_identify_canonical_persisted_metadata_without_rebuilding_it; state_metadata_identifies_canonical_scalar_only_persisted_metadata; normalization_rejects_names_that_cannot_be_addressed_by_fql.
- crates/wf-domain/tests/metadata_validation/values.rs: metadata_integer_and_float_classes_preserve_exact_boundaries; attribute_normalization_preserves_atoms_opaque_binaries_and_last_list_occurrences; metadata_outputs_borrow_leaves_and_keep_input_graphs_unchanged.
- crates/wf-domain/tests/metadata_validation/normalization.rs: metadata_names_preserve_unicode_trim_errors_and_reserved_rules; metadata_validates_overwritten_rows_and_uses_last_normalized_key; state_metadata_collisions_replace_whole_inner_maps; metadata_list_classes_preserve_improper_argument_error_and_guard_refusal.
- crates/wf-domain/tests/metadata_validation/bounds.rs: metadata_counts_precede_collisions_and_list_deduplication; metadata_logical_totals_follow_final_values_and_exact_boundaries.

This follows AGENTS' tests/<target>/main.rs plus sibling-children convention. Cargo target remains metadata_validation, so the existing planned --test metadata_validation command is unchanged. Source-original three functions stay at target root for later direct assertion mapping; derived full test names gain their ordinary private module prefixes. Children may import the already selected model/API names from super; imports are not new helpers or aliases/models. There are EXACTLY12 test functions, no test helper, fixture module/file, probe or dependency. Do not create the flat metadata_validation.rs alongside this directory. All28 original observations and the full derived controls remain binding; state integer/float cross-checks may be placed in the existing relevant case without adding a function or dropping another assertion.

Root's separately selected real pinned-VM key-order capture remains mandatory for the provisional64/65-state HAMT fixtures: author reports marked positions and stops normally; Root captures actual Enum.reduce orders after that STOP, reorders only fixture data/associated expected survivor order consistently, then formats/freezes before the FIRST Rust baseline execution. Lexical order must not be called authentic without that evidence. Actual-map strict uniqueness/order preconditions and deterministic last-survivor output convention are unchanged; no arbitrary permutation exception, fake source map, extra helper/file or timeout change follows. No original source assertion is altered by this large-map fixture integration.

Domain write set therefore changes from five paths to EIGHT: existing lib.rs; new metadata.rs/metadata/{attributes,state_meta}.rs; the four test paths above. Root-owned policy.toml/ADR002/metadata_policy.rs/tests.rs footprint is unchanged, giving12 total planned code/config/test paths. Root must update its exact write/inventory scope and record this delta before creation; no other readiness gate or new acknowledgment is added. Source models, two lifetimes, six real R0 rejectors, BODY-only19helpers/NameKind/eight constants, operational error and Root release/SAVE stages remain exactly as selected. Each new integration file remains<=600LF and every function<=100 after ordinary formatting; grouping is not a waiver, and further needed helpers/files require another concrete consult. Only this architecture note was appended; no source/test/config/Git/VM/workload action. Finite geometry decision stops normally.
