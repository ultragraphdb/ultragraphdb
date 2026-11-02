# Group 1: the system ID

One crate, `ultragraphdb_core_ids` (`ultragraphdb_core/ultragraphdb_core_ids`), depending on `xxhash-rust` alone.
`IdError` lives in it: only this crate raises it (AGENTS.md: an error takes a crate of its own only when more than one
crate raises it). The fuzz package is `fuzz/ultragraphdb_core_ids`.

## Surface

`SystemId` (`new`, `resource`, `statement`, `type_prefix`, `bucket`, `sequence`, `to_u128`, `to_be_bytes`,
`from_u128`, `decode`, `BYTES`), `TypePrefix` (`new`, `get`, `RESOURCE` = 1, `STATEMENT` = 2), `Bucket` (`new`, `get`,
`MAX`, `derive`), `Sequence`, `Namespace`, `CanonicalKey` (`iri`, `as_str`, `encoded_len`, `encode_into`), `Epoch`
(`new`, `get`, `next`, `MAX`), `Version` (`commit`, `from_u64`, `decode`, `to_u64`, `to_be_bytes`, `epoch`, `sequence`,
`is_commit`, `BYTES`, `COMMIT_SEQUENCE_MAX`), `SessionToken` (`new`, `partition`, `position`, `epoch`), and
`IdError(pub IdErrorEnum)` with one variant per refusal: `Length`, `ReservedBits`, `ZeroTypePrefix`, `ZeroSequence`,
`BucketRange`, `IriTooLong`, `EpochRange`, `CommitSequenceRange`, `OddCommitSequence`.

## Phases

### 1. Surface

Every type, constant, constructor, getter, error variant and its constructor and `Display`, with bare
`unimplemented!()` bodies. No `#[expect(clippy::unimplemented)]`: `unimplemented = "warn"` in the workspace table is
deliberate, its warnings are accepted through phase 3, and phase 4 ends with none (the user, 2026-11-02). The
constants (`TypePrefix::RESOURCE`, `Bucket::MAX`, `Epoch::MAX`, `Version::COMMIT_SEQUENCE_MAX`) are values, not bodies.
`bazel build` of the library passed.

The surface was first built as two crates, `ultragraphdb_error_ids` and `ultragraphdb_core_ids`, as the design's table
then stated. The coordinator's correction (AGENTS.md's rule above; the design's table now says the same) came during
phase 3: the error module and its tests moved into `ultragraphdb_core_ids`, and the `Invariant` variant and
`From<InvariantError>` left with the error crate, since the design now gives the crate no dependency but `xxhash-rust`
and no check of the crate returns an invariant error. The emptied crate folder was moved out of the repository, to the
session scratchpad (`moved_out_of_repo/ultragraphdb_error_ids`), because an empty folder under the `ultragraphdb_error/*`
member glob stops Cargo and every Bazel build; the user may delete it there.

### 2. Suite

- `features/system-identity/system_identity.feature`: the nine scenarios of the requirements "A system ID is 128 bits
  of type, bucket, sequence and reserved bits", "The bucket comes from the namespace and the key" and "Until UGS keys
  exist, an IRI is the key of the Resource type", verbatim, bound by `tests/bdd.rs` (cucumber, `fail_on_skipped`).
- Unit tests mirroring `src/`: `tests/types/*` and `tests/errors/id_error/*`.
- Golden buckets: `testdata/golden/buckets.json`, 14 cases, written by `testdata/golden/buckets.py` with python-xxhash
  4.0.2 over the xxHash C library 0.8.3 (Python 3.12.0, in a scratch venv run with `python3 -I`; its XXH3-64 of the
  empty input is the spec's `0x2d06800538d394c2`). The script builds each hashed byte string from the rule's text
  alone. The cases cover namespaces 0, 1, 2, 0x0102030405060708 and 2^64 − 1; prefixes 1, 2, 3 and 65,535; and IRIs
  taking the hashed input through every XXH3-64 length class (6, 11, 21, 156, 306 and 5,006 bytes, the last over several
  1,024-byte blocks) plus non-ASCII UTF-8. Read with `ultragraphdb_tool_bdd::load_golden`; `dgraph_commit` says the
  values are not from Dgraph.
- Hand-computed bytes: the scenario's packing `00 03 | AB CD EF | 00×7 2A | 00 00 00`, prefix 65,535 with bucket 0 and
  sequence 1, prefix 1 with bucket 2^24 − 1 and sequence u64::MAX, versions `10 << 22 = 41,943,040`,
  `1 << 22 | 2 = 0x400002`, the largest commit version `0xFFFF_FFFF_FFFF_FFFE`, canonical keys of 15, 0, 22 and 300
  bytes.

Corner cases enumerated before the suite, each with its test:

| Case | Test |
|---|---|
| prefix 0 | `test_prefix_zero_is_refused`, `test_a_zero_type_prefix_is_refused` |
| prefix 65,535 | `test_the_smallest_declared_and_the_largest_prefix_hold_their_value`, `test_the_largest_prefix_with_bucket_zero_and_sequence_one`, the golden case |
| bucket 0 and 2^24 − 1, and 2^24 and u32::MAX refused | `test_buckets_zero_and_the_largest_hold_their_value`, `test_a_bucket_past_24_bits_is_refused`, the two packing tests |
| sequence 0, 1 and u64::MAX | `test_sequence_zero_is_refused`, `test_sequences_one_and_the_largest_hold_their_value`, `test_a_zero_sequence_is_refused`, the packing tests |
| each of the 24 reserved bits set alone, and all 24 | `test_each_reserved_bit_set_alone_is_refused`, `test_every_reserved_bit_set_is_refused_naming_them` |
| several invalid fields at once (the check order) | `test_the_reserved_bits_are_checked_first_then_the_prefix` |
| 0, 1, 15, 17 and 32 bytes as an ID; 0, 7, 9 and 16 as a version | the two `test_bytes_of_another_length_are_refused` |
| an empty IRI | `test_the_empty_iri_encodes_as_a_zero_length`, the golden case |
| a 4 GiB IRI refused | `test_a_4_gib_iri_is_refused` |
| a length above 255, multi-byte UTF-8 | `test_a_length_above_255_takes_two_bytes`, `test_the_length_counts_utf8_bytes_not_characters` |
| epoch 0, 1, 2^42 − 1, 2^42 and u64::MAX; the next after the last | `test_epochs_zero_and_one_hold_their_value`, `test_the_largest_epoch_is_two_to_the_42_minus_one`, `test_an_epoch_past_42_bits_is_refused`, `test_no_epoch_follows_the_last` |
| commit sequence 0, 2, 4, 2^22 − 2; odd; 2^22; 2^22 − 1 (odd and past) | the version tests |
| versions 0, 1, an odd rollup version and u64::MAX | `test_every_number_is_a_version` |
| a session token at its largest fields | `test_a_token_keeps_its_largest_fields` |

Every error variant is provoked through the crate's API and asserted by variant: `Length` (`SystemId::decode`,
`Version::decode`), `ReservedBits`, `ZeroTypePrefix`, `ZeroSequence` (`decode`, `from_u128`, `TypePrefix::new`,
`Sequence::new`), `BucketRange` (`Bucket::new`), `IriTooLong` (`CanonicalKey::iri`), `EpochRange` (`Epoch::new`,
`Epoch::next`), `CommitSequenceRange` and `OddCommitSequence` (`Version::commit`).

Run against the surface (`bazel test //ultragraphdb_error/ultragraphdb_error_ids/... //ultragraphdb_core/ultragraphdb_core_ids/...`,
then two crates): error crate unit 8 tests, 8 failed; core unit 53 tests, 53 failed; BDD 9 scenarios, 9 failed. Every
failure was the `not implemented` panic of a body in `src/` (counted by file: epoch 14, canonical key 11, type prefix
10, system ID 6, version 3, sequence 3, bucket 3, namespace 2, session token 1; the error crate's constructors 7 and its
`From` 1; each BDD step "Step panicked. Captured output: not implemented"). After the merge the suite is unit 59 (the
53 and the error tests less the two of the removed `Invariant` variant) and BDD 9.

### 3. Audit

Each defect planted alone in the implementation, the suite run under Bazel, the defect reverted (the script and its
log in the session scratchpad, `audit/`). 41 defects; every one failed at least one test whose subject is the defective
behaviour.

| Defect | Unit tests failing | Scenarios failing |
|---|---|---|
| D01 a zero type prefix read as 1 | `test_a_zero_type_prefix_is_refused`, `test_the_reserved_bits_are_checked_first_then_the_prefix`, `test_prefix_zero_is_refused` | A zero type prefix or sequence is refused |
| D02 a zero sequence read as 1 | `test_sequence_zero_is_refused`, `test_a_zero_sequence_is_refused` | A zero type prefix or sequence is refused |
| D03 the epoch bound off by one | `test_the_largest_epoch_is_two_to_the_42_minus_one`, `test_the_next_epoch_is_one_above` | — |
| D04 the next epoch two above | `test_no_epoch_follows_the_last`, `test_the_next_epoch_is_one_above` | — |
| D05 the bucket bound off by one | `test_buckets_zero_and_the_largest_hold_their_value`, `test_prefix_one_with_the_largest_bucket_and_sequence`, `test_the_order_is_type_then_bucket_then_sequence` | — |
| D06 the namespace hashed little-endian | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | Two namespaces separate one key |
| D07 the key's length left out of the hash | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_the_scenario_bucket_of_http_ex_org_a`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | The bucket matches the XXH3 reference; A statement shares its subject's bucket |
| D08 the namespace hash's low byte | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_the_scenario_bucket_of_http_ex_org_a`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | The bucket matches the XXH3 reference; A statement shares its subject's bucket; Two namespaces separate one key |
| D09 the key hash's two bytes swapped | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_the_scenario_bucket_of_http_ex_org_a`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | The bucket matches the XXH3 reference; A statement shares its subject's bucket |
| D10 the prefix left out of the hash | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits` | — |
| D11 the namespace hashed with seed 1 | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_the_scenario_bucket_of_http_ex_org_a`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | The bucket matches the XXH3 reference; A statement shares its subject's bucket; Two namespaces separate one key |
| D12 the IRI length truncated, unchecked | `test_a_4_gib_iri_is_refused` | — |
| D13 the key length little-endian | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_the_scenario_bucket_of_http_ex_org_a`, `test_a_length_above_255_takes_two_bytes`, `test_encoding_appends_to_the_buffer`, `test_the_canonical_encoding_of_an_iri`, `test_the_length_counts_utf8_bytes_not_characters`, `test_a_resource_takes_prefix_one_and_its_derived_bucket` | The bucket matches the XXH3 reference; A statement shares its subject's bucket; The canonical encoding of an IRI |
| D14 the encoded length one short | `test_the_canonical_encoding_of_an_iri`, `test_the_empty_iri_encodes_as_a_zero_length`, `test_the_length_counts_utf8_bytes_not_characters` | — |
| D15 the sequence packed little-endian | `test_the_fields_pack_and_unpack`, `test_the_largest_prefix_with_bucket_zero_and_sequence_one` | The fields pack and unpack; A zero type prefix or sequence is refused; IDs of one type sort together |
| D16 a reserved byte written as 1 | `test_prefix_one_with_the_largest_bucket_and_sequence`, `test_the_fields_pack_and_unpack`, `test_the_largest_prefix_with_bucket_zero_and_sequence_one` | The fields pack and unpack; A zero type prefix or sequence is refused; IDs of one type sort together |
| D17 the reserved check removed | `test_each_reserved_bit_set_alone_is_refused`, `test_every_reserved_bit_set_is_refused_naming_them`, `test_the_reserved_bits_are_checked_first_then_the_prefix` | Nonzero reserved bits are refused |
| D18 only the last reserved byte checked | `test_each_reserved_bit_set_alone_is_refused`, `test_every_reserved_bit_set_is_refused_naming_them` | — |
| D19 the prefix checked before the reserved bits | `test_the_reserved_bits_are_checked_first_then_the_prefix` | Nonzero reserved bits are refused |
| D20 a zero sequence decoded as 1 | `test_a_zero_sequence_is_refused` | A zero type prefix or sequence is refused |
| D21 bytes past 16 ignored | `test_bytes_of_another_length_are_refused` | — |
| D22 the length error's fields swapped | `test_bytes_of_another_length_are_refused` | — |
| D23 the bucket ordered before the prefix | `test_ids_of_one_type_sort_together`, `test_the_order_is_type_then_bucket_then_sequence` | — |
| D24 a statement given the Resource prefix | `test_a_statement_takes_prefix_two_and_its_subjects_bucket` | The reserved prefixes |
| D25 a statement given bucket 0 | `test_a_statement_takes_prefix_two_and_its_subjects_bucket` | A statement shares its subject's bucket |
| D26 a resource hashed under the Statement prefix | `test_a_resource_takes_prefix_one_and_its_derived_bucket` | A statement shares its subject's bucket |
| D27 the odd-sequence check removed | `test_an_odd_commit_sequence_is_refused` | — |
| D28 the commit-sequence bound off by one | `test_the_largest_commit_version`, `test_the_last_commit_of_an_epoch_is_below_the_next_epochs_first`, `test_versions_order_by_epoch_then_sequence` | — |
| D29 the epoch shifted by 21 | `test_a_version_carries_the_epoch_in_its_upper_bits`, `test_a_version_is_its_8_big_endian_bytes`, `test_the_largest_commit_version`, `test_the_last_commit_of_an_epoch_is_below_the_next_epochs_first`, `test_versions_increase_within_an_epoch`, `test_versions_order_by_epoch_then_sequence` | — |
| D30 the sequence mask one bit short | `test_every_number_is_a_version` | — |
| D31 the epoch read shifted by 21 | `test_a_version_carries_the_epoch_in_its_upper_bits`, `test_every_number_is_a_version`, `test_the_last_commit_of_an_epoch_is_below_the_next_epochs_first` | — |
| D32 is_commit inverted | `test_a_version_carries_the_epoch_in_its_upper_bits`, `test_every_number_is_a_version` | — |
| D33 a version decoded little-endian | `test_a_version_is_its_8_big_endian_bytes` | — |
| D34 bytes past 8 ignored | `test_bytes_of_another_length_are_refused` | — |
| D35 the range checked before oddness | `test_an_odd_commit_sequence_is_refused` | — |
| D36 a token's position returning its partition | `test_a_token_keeps_its_largest_fields`, `test_a_token_names_its_commit` | — |
| D37 a namespace truncated to 32 bits | `test_every_golden_bucket_matches_the_reference`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_a_namespace_holds_its_value` | — |
| D38 the epoch bound guard removed | `test_an_epoch_past_42_bits_is_refused`, `test_no_epoch_follows_the_last` | — |
| E01 reserved bits printed unpadded | `test_invalid_id_texts_name_the_field` | — |
| E02 the largest bucket off by one in the text | `test_range_texts_name_the_limit` | — |
| E04 the length constructor swapping its fields | `test_length_text`, `test_each_constructor_builds_its_variant`, `test_bytes_of_another_length_are_refused`, `test_bytes_of_another_length_are_refused` | — |

The audit's first run found a defect in the suite: `test_a_4_gib_iri_is_refused` compared with `assert_eq!`, so
under D12 its failure message printed the 4 GiB key's `Debug` text, an 8 GiB test log, and the run never ended. It now
matches the outcome and prints a fixed message. D03, D04, D10, D12, D14, D18, D21–D23 and D27–D38 fail unit tests
alone, as no scenario states those rules. Input variety: the packing scenario's fields (3, 0xAB_CDEF, 42) are
distinct; D10 (the prefix left out of the hash) is caught only by the golden prefixes 2, 3 and 65,535, which were
enumerated for it; the session token's three fields (1, 40, 12) differ.

### 4. Implementation

- `SystemId` holds its three typed fields, declared in the order prefix, bucket, sequence, so the derived order is the
  order of the 16 big-endian bytes; its bytes are built and read by destructuring arrays, with no shift or cast.
  `from_u128` checks the reserved bits, then the prefix, then the sequence.
- `Bucket::derive` streams the prefix, the key's length and the IRI into `Xxh3Default` (seed 0), so no buffer holds
  them together; XXH3's streaming digest equals its one-shot hash, which the golden cases across every length class
  confirm.
- `Version` is the 64-bit number; `commit` checks oddness, then the 22-bit range.

`bazel test //ultragraphdb_core/ultragraphdb_core_ids/...`: unit 59 passed, BDD 9 scenarios (18 steps) passed; the
same counts under Cargo. `scripts/coverage.sh ultragraphdb_core_ids`: 0 uncovered lines. Formatting and Clippy were
run scoped to this crate, as `make format` and `make fix` run them (`buildifier` on its `BUILD.bazel`, `cargo fmt -p`
for the crate and its fuzz package, `cargo clippy -p ultragraphdb_core_ids --all-targets -- -Wclippy::all`), so the
other groups' crates in progress were left untouched: Clippy found one `shadow_unrelated` in a test, renamed; a second
run of each reports nothing and changes nothing, and no `unimplemented` warning is left.

### 5. Fuzz

`fuzz/ultragraphdb_core_ids` with `system_id_decode` and `version_decode`, seeded from
`testdata/fuzz_seeds/<target>/` (the suite's golden and hand-computed values: valid IDs, each invalid field, a short
input; commit versions, a rollup version, all ones, a short input). Each target checks the decoder against the format
computed from the bytes: an accepted value re-encodes to its bytes and its fields match, a refusal names the first
invalid field, any other length is refused naming it. `cargo +nightly fuzz build --fuzz-dir ultragraphdb_core_ids` (in `fuzz/`) built both targets with no warning of
their own; each replayed its seeds once (`-runs=0`: 10 and 7 inputs, no failure). The time-boxed runs of
`scripts/fuzz.sh` belong to group 12.

## Decisions the spec left open (for the user's review)

1. **Scenarios of two requirements are not in this crate.** "Sequences are unique per type and leased in blocks per
   partition" (3 scenarios) needs the metadata group's leases and a partition's allocator across a crash, and "System
   IDs never leave the database" (2 scenarios) needs result batches and resume keys. Their behaviour is built in groups
   3 and 6 (the block source and its leases) and 9 (results and resume keys), so their Gherkin belongs in those crates'
   `features/system-identity/`. Task 1.1 stays open for them.
2. **The check order of a decoded ID:** reserved bits, then type prefix, then sequence. The scenario "16 bytes whose
   last byte is 0x01" names the reserved bits whether the other bytes are valid or zero.
3. **An empty IRI is accepted** and encodes as a length of 0: the IRI's syntax is the term layer's to check. A
   4 GiB IRI is refused with `IriTooLong`. The test allocates 2^32 zero bytes (about 4 GiB resident for a second on
   macOS); the accepting boundary, an IRI of u32::MAX bytes, has no test, which would double that.
4. **Every 64-bit number is a `Version`**, since a rollup writes at a free version below a delta, which may be odd;
   `Version::decode` refuses only a length. Commit versions are built by `Version::commit`, even and at most 2^22 − 2;
   an odd sequence is named before an out-of-range one. Epoch 0 is representable (version 0, before the first close).
5. **`SessionToken`'s partition is a `u32`**: `PartitionId` belongs to `ultragraphdb_cluster_placement`, which depends
   on this crate; a partition owns at least one of 2^24 buckets, so 32 bits hold every partition. The token has no wire
   encoding here (PROTOCOL.md, task 0.3) and no checks: position 0 is accepted. It is not `Copy` (24 bytes, above Ultra
   Style's 16).
6. **`SystemId` is three typed fields, not `SystemId(u128)`** as the design writes it: the same order and bytes, with no
   unreachable error path when a getter rebuilds a non-zero prefix or sequence. `to_u128` and `from_u128` give the
   integer form the packed lists of group 2 delta-encode.
7. **`CanonicalKey` borrows the IRI** and encodes into the caller's buffer (`encode_into`), so the data plane need not
   allocate for a bucket.
8. **`IdError` has no `Invariant` variant**: the crate raises no invariant error (its invariants are its types and
   `debug_assert!`s), and the design gives it no dependency but `xxhash-rust`.
9. **`SystemId::resource` and `SystemId::statement`** encode the two built-in rules (prefix 1 and the derived bucket;
   prefix 2 and the subject's bucket) so the dictionary of group 3 does not restate them.

## Deviations

- Phase 3 audited the implementation kept for phase 4, each defect planted and reverted in turn, rather than a
  throwaway rewritten afterwards (as stage 5's group 1 did).
- The held-back lints were dropped from this change by the user (2026-11-02); the crate meets the workspace table as it
  stands.

## Files

- `ultragraphdb_core/ultragraphdb_core_ids/` (new): `Cargo.toml`, `BUILD.bazel`, `src/`, `tests/`, `features/system-identity/`,
  `testdata/golden/buckets.{py,json}`, `testdata/fuzz_seeds/{system_id_decode,version_decode}/`.
- `fuzz/ultragraphdb_core_ids/` (new); `fuzz/Cargo.toml` (the crate's path).
- Root `Cargo.toml` (`[workspace.dependencies]`), `Cargo.lock`, `AGENTS.md` (the crate's row; `xxhash-rust`'s users).

## After review (2026-11-02)

The test of the 4 GiB refusal held a 4 GiB string in memory, which `make test`'s parallel targets cannot afford. The
check moved into `CanonicalKey::length(len)`, which `CanonicalKey::iri` calls and a decoder can call before reading an
IRI's bytes; both sides of the bound (`u32::MAX` accepted, `2^32` refused) are now tested on the length alone. The
user approved the change, and asked that every test stay small and fast for parallel runs.

## Field order: sequence above bucket (2026-11-02)

The user approved a new field order. A `SystemId` is still 128 bits, big-endian; from the most significant bit it is
now a 16-bit type prefix, a 64-bit sequence, a 24-bit bucket and 24 reserved bits (zero): bytes 0–1 type, 2–9
sequence, 10–12 bucket, 13–15 reserved. IDs sort by type, then sequence, then bucket. The reason: adjacency lists hold
IDs whose buckets are hashed, so random; with the bucket below the sequence, the IDs of a sorted list differ mostly in
their sequences, and their LEB128 deltas shrink from about 11–12 bytes to about 7–8 per entry. The requirement is
renamed "A system ID is 128 bits of type, sequence, bucket and reserved bits" in the spec; its pack scenario now
expects `00 03`, `00 00 00 00 00 00 00 2A`, `AB CD EF`, `00 00 00`, and its sort scenario (1, 900, 7), (1, 3, 99),
(2, 9, 1).

- Tests first. The feature file's rule and both scenarios were copied from the edited spec, with their step bindings in
  `tests/bdd.rs`. The unit tests' packed bytes and `u128` values, the zero-sequence byte (offset 9, was 12), the sort's
  expected order and the order test (now `test_the_order_is_type_then_sequence_then_bucket`: each pair differs in the
  field named while every field below it runs the other way, checked through `Ord`, the bytes and `to_u128`) were
  rewritten by hand from the new layout. Eight of the nine `system_id_decode` seeds were re-packed; the ninth,
  prefix 1 with every bucket and sequence bit set, has the same bytes in both layouts. The golden buckets are the
  hash derivation alone, which names no ID layout, and are unchanged.
- Against the old implementation under Bazel: unit 60 tests, 5 failed (`test_the_fields_pack_and_unpack`,
  `test_the_largest_prefix_with_bucket_zero_and_sequence_one`, `test_a_zero_sequence_is_refused`,
  `test_ids_of_one_type_sort_together`, `test_the_order_is_type_then_sequence_then_bucket`); BDD 9 scenarios, 3 failed
  (The fields pack and unpack; A zero type prefix or sequence is refused; IDs of one type sort together), 3 of 18
  steps. Every failure was an assertion on the layout or a decode that should have refused.
- Implementation. `SystemId`'s fields are declared prefix, sequence, bucket, so the derived order stays the order of
  the 16 big-endian bytes; `to_be_bytes` and `from_u128` destructure the new positions; the checks and their order
  (reserved bits, prefix, sequence) are unchanged. Planting the old field order with the new bytes fails
  `test_ids_of_one_type_sort_together` and `test_the_order_is_type_then_sequence_then_bucket`.
- Under Bazel: unit 60 passed, BDD 9 scenarios (18 steps) passed. `scripts/coverage.sh ultragraphdb_core_ids`: 0
  uncovered lines. `cargo fmt -p ultragraphdb_core_ids` changed nothing and `cargo clippy -p ultragraphdb_core_ids
  --all-targets -- -Wclippy::all` reported nothing.
- Fuzz: `system_id_decode` reads the sequence from bytes 2–9 and the bucket from 10–12.
  `cargo +nightly fuzz build --fuzz-dir ultragraphdb_core_ids` built; the target replayed its seeds once (`-runs=0`,
  10 inputs, no failure).
- `SystemId::new` keeps its parameter order (prefix, bucket, sequence), the order the scenarios name the fields in;
  only the bytes moved.
