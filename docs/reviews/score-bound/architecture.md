# Score-bound architecture review

This feature review uses the approved grouping
`docs/reviews/score-bound/{architecture,performance,domain}.md`. Existing
workflow-completion review files retain their original bytes and evidence
offsets. Root creates these three paths; their owners append only after the
coordinator explicitly switches custody. No source implementation is authorized
by this document alone.

The selected scope is the complete pure pinned `ScoreBound.parse/1` binary
helper, not storage, public query authority or a replacement for HistoryValues.
The original source is `flow/score_bound.ex` at commit
`66586c596e271408d9184af32eb31cd7218c126e`, SHA-256
`13cb043c56dc5db90dd76cb59dd995ae60bf21367d5b14b6219e4ae9b6b269a4`.
Three original standalone tests pass with seven assertions. Full profiles and
Linux remain unverified.

The final prior-note consultation selects the staged API and tests. Root must
read that decision before releasing rejection-only baseline authorship, save
actual failing-test evidence, and release the substantive parser separately.
Returned source errors and raised ArgumentError origins remain distinct; Rust
must not panic to represent the latter.

ARCH SCORE-BOUND1 actual rejection R0 source/runtime review, 2026-10-24. Whole/hash-read stopped child35LF/1155B/6ed79fd839c8ef53fd27da36b5d0da41921504e09ca3cfe016af3d13e52ca74f, parent232/7453/ab455066e3d7bc552b4d1e8e2e60f7ec6d7d619faaa33a112731430ee35cbc97 and canonical test226/7137/ca7256479f5f079d0f2038fbaf4fad486e5bd2a6a3a6577bb922897b7216211f. Only the selected two enums, INVALID_FLOAT and five-line always-Returned parser exist in the new child. No number_prefix shell, conversion, sentinel behavior, additional type/helper/module or dependency was introduced. Own exact two-line113B module/reexport inverse restores ENTIRE old parent230/7340/95aec8f4c8bb67a871856e22c8009b08760787547b122e5d980978b963230e8b. The other option APIs and bodies are literal.

The API matches the recorded selection: Bound NegInf/PosInf/Inclusive(f64)/Exclusive(f64), PartialEq rather than Eq; Error Returned(&'static str)/RaisedArgumentError, Eq/PartialEq; borrowed binary input and copied numeric/static-error result. Numeric finiteness is a condition on successful parser outputs, not a proof carried by every externally constructible public enum. RaisedArgumentError represents the original source exception class/channel without a Rust panic or copied BEAM stack/message. The current rejected input is unused and no scanner/allocator operation is present. Future core conversion, total allocation/stack/latency/RSS and libc equivalence are not proved by this baseline.

Independently reread complete pinned production20LF/484B/13cb043c and test22/732B/e680d5ef. Source async:true/flow/no-setup metadata and all seven independent observations remain represented in order: source8/9 -> Rust16/17 (-inf/+inf kinds); source13/14/15 -> Rust22/26/30 (inclusive42/-1.5 and exclusive3.25 with literal expected IEEE bits); source19/20 -> Rust38/42 (bad/(bad exact Returned bytes). Tests derive no expected float from the parser under test. The six selected test names are exact; spans are15..18,21..34,37..46,49..125,128..166,169..226, lengths4/14/10/77/39/58. No test helpers or fixture module exist. Natural child35/parent232/test226 and max77 satisfy production400/test600/function100.

Derived controls meaningfully distinguish complete prefix grammar, exact sentinels, arbitrary binary/invalid-UTF8 suffixes, signed zero, underflow/subnormal/finite boundaries and the two halfway rounding vectors. Overflow fixtures distinguish no-complete-exponent RaisedArgumentError from complete-exponent Returned, including remainder suffixes; they prevent a future early full-input rejection from masking a source conversion exception. Test repeat/format allocations are explicitly fixture-owned and provide no production allocation evidence. The &[u8] acquisition boundary does not manufacture nonbinary FunctionClauseError coverage. The selected later scanner/inline conversion must preserve the pinned Float.parse conversion-before-remainder order; whole-input unchecked FromStr is not licensed. Supported nearest/ties-even bit observations and original source exceptions constrain implementation; arbitrary libc/locale/FENV, Linux and full profile equivalence remain open.

Actually whole/hash-read Root's target packet8393808+4314/0f80d71e024272bcc755edd8f8948b70d9315ec4b50df71d152a0c739937c695 plus step8 metadata/terminal footer. It compiles the real callable target and runs six named tests: one PASS, five assertion FAIL, zero ignored/measured/filtered, EXIT101/UNCHANGED True. Own name/status Counter equals exactly the six source test functions. parse_rejects_non_float_bounds passes BOTH original bad/(bad assertions. The other originals fail at -inf/Rust16 and42/Rust22; +inf/-1.5/exclusive3.25 are masked. Derived grammar fails its first+1 at62, IEEE its first0 at157, overflow its first400-nine RaisedArgumentError expectation at197; later derived tails are masked. These are genuine behavior failures of the five-line Returned stub, not missing symbols, compiler/lint failures or a claimed pass of all edge vectors. Root reports the first seven quality steps all0, including strict Clippy, architecture51, reference480, progress114/126/521 and diff; authoritative step8 metadata records1556 inputs/a3f0c61f4e160e0a32c36202475b6c7451813fd804b1b81f6a310ae700043407 unchanged. The full first-seven packet was not independently reread here.

Actually whole/hash-read NEW DOMAIN1144+1294/702a2a8417c548a42cb70496d2fe40784ead61fddd1da3b0dfbe425de7863fb2 and NEW PERF1148+2667/18b8adc283955c8e867fe980a84ca29ef79a3adfbb52e8e355871ecdecb1773f. Independent source/API/parent-inverse/oracle/geometry/resource findings agree. Author/Root's held1553/89002152 proof is attributed. Own post-runtime source/test hashes remain exact. Own OLD ARCH full4843946/f723c33f2dcb8003846a6ef6328803a5fb196d1e1689243660834c2c7984b4eb remains exact; OLD PERF/DOMAIN ends3692846/5090177 and their final6399/92670c19 and1487/12bc57a3 spans also remain exact. Sole append here preserves the document cutover and old receipt offsets.

Current ledger remains ENTIRE1373820/e3733235c9d22fe16d95bf313c3ba6f44c80732a9c780a876195a6574366cda7; all three Score declarations302351265d6e869f/ad80a44b66da3f16/1be2c83f0aed7528 still have empty rust_assertions and unchanged runtime IDs. No parity, performance gain, public query/budget authority or Linux credit is assigned. Scoped R0 review has no source defect. Root owns genuine RED SAVE before separate BODY release; frozen six tests and the already-selected one private scanner helper govern that later phase. No extra acknowledgment/readiness gate is added. ARCH ran no compiler/tests/VM/Git/probe and changed no source, ledger or other document. This finite source/runtime review stops normally.
