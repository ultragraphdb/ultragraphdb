# Group 1: the posting format

## 1.2 The fields

Every field of the six messages, by whether code reads it. A field is carried when code decides on its value; one
read only to be copied, or only by the decoder's check that it is a known enum value, is not.

| Message | Carried | Not carried |
|---|---|---|
| `UidPack` | `block_size`, `blocks` | `alloc_ref`: Dgraph's allocator reference; the port only set it to 0 (`copy_uid_pack`, the encoder) or copied it (`bin_split`) |
| `UidBlock` | `base`, `deltas`, `num_uids` | — |
| `List` | `uids` | — |
| `PostingList` | `pack`, `postings`, `commit_ts`, `splits` | — |
| `Posting` | `uid`, `value`, `posting_type`, `lang_tag` (read by `has_delete_all`), `facets` (read by the rollup, and by the scenario "Facets survive a rollup"), `op`, `start_ts`, `commit_ts`, `statement_ids` | `val_type`: written as `UID` on the uid postings the iterator synthesizes, read by no caller, and otherwise read only by the decoder's check that it is known; field 6 (`label`) was already reserved |
| `Posting.PostingType` | `REF`, `VALUE` | `VALUE_LANG`: no code writes it |
| `api.Facet` | `key` | `value`, `val_type`, `tokens`, `alias`: read nowhere |
| `BackupPostingList` | `uids`, `postings`, `commit_ts`, `splits`, `uid_bytes` | — |

No code under `src/` writes a facet or a language tag: both are written only by tests (the facet scenario of
posting-lists, the delete-all-with-a-language test). They are carried because they are read; whether the quad layout
needs either is a question for the user, not settled here.

## Phases

### 1. Surface

`ultragraphdb_core_codec` gained `UidBlock`, `UidPack`, `List`, `Facet`, `PostingType`, `Posting`, `PostingList`
and `BackupPostingList`, each with private fields, a constructor or `Default`, getters, `with_*` builders and the
setters the callers need (AGENTS.md: a public type's fields are private), and `PostingList::encode`,
`PostingList::encoded_len` and `PostingList::decode(key, bytes) -> Result<PostingList, StoreError>`, every body
`unimplemented!()`. The existing UID codec (`Encoder`, `Decoder`, `codec.rs`) moved onto the new `UidPack` through
its getters. The crate's dependencies became `ultragraphdb_error_store` and `xxhash-rust`, in place of
`ultragraphdb_proto_pb`. `cargo build -p ultragraphdb_core_codec` and `bazel build //ultragraphdb_core/ultragraphdb_core_codec`
passed. The crates above did not compile from here until 1.5, which this group's order implies.

### 2. Suite

- Unit tests for every type (`tests/types/*_tests.rs`), each field given a value distinct from the others'.
- The format's suite, `tests/types/posting_list/posting_list_codec_tests.rs`: four golden encodings (empty, an
  empty pack, a delete-all delta, a list with every field set, two blocks and splits) built by hand from the
  documented layout by a script independent of the crate and checksummed with python-xxhash 3.6.0, the xxHash C
  library (its XXH3-64 of the empty input, 0x2d06800538d394c2, is the spec's); encode and decode against them;
  `encoded_len`; re-encoding; a pack absent versus empty; a 70,000-byte value (a u16 length would show); boundary
  values (`u64::MAX`, `u32::MAX`, empty and non-ASCII facet keys); the encoder's deltas kept byte for byte (uid-lists'
  byte identity); every truncation, every single-bit flip and every flipped byte of the full golden refused as
  `Corrupted` naming the key; the exact reasons for a short value, an unknown version (0, 2, 255, before the checksum),
  a failed checksum (stored and computed values from python-xxhash), unknown flags, trailing bytes, a count and a
  length past the end, every count at `u32::MAX`, unknown posting types (2, Dgraph's `VALUE_LANG`, and 255), a facet
  key that is not UTF-8; the key passed through; the fuzz seeds valid.
- The scenarios of posting-lists' "Posting lists are stored in the repository's own format", "A list round-trips"
  and "A flipped byte is corruption", in `features/posting-lists/posting_format.feature`; uid-lists' eight scenarios
  unchanged on the moved types.
- Corner cases enumerated: the empty list; a pack with no blocks against no pack; one block of one UID; a block of
  `u32::MAX` UIDs (the format carries it; `store_posting` refuses it); a value longer than a u16; every count at
  its largest; a length overrunning; an unknown enum byte; invalid UTF-8; trailing bytes; a value of 0 and 1 bytes,
  and of 25, one short of the empty list; the key empty.
- No error type was introduced or extended: a refusal is the existing `StoreError::Corrupted`.

Run against the surface: unit 64 tests, 61 failed, each at an `unimplemented!()` of the new types or the codec; the
3 that passed reach none of them (`an_overlong_varint_is_refused` and `an_encoder_without_uids_returns_no_pack`, the
codec's existing paths that never build a pack, and `a_posting_is_a_reference_by_default`, a derived `Default`).
BDD: 10 scenarios, 10 failed at `unimplemented!()`.

### 3. Audit

Each defect applied alone to the implementation, the suite run, then reverted:

| Defect | Caught by |
|---|---|
| D01 the empty length off by one | `a_value_shorter_than_an_empty_list_is_truncated`, `the_encoded_length_is_the_length_of_the_bytes` |
| D02 the version guard removed | `an_unknown_version_is_refused_before_the_checksum` |
| D03 the checksum guard removed | the three flip tests, `a_flipped_bit_fails_the_checksum`; the scenario "A flipped byte is corruption" |
| D04 the checksum over one byte less | 14 tests, the goldens among them |
| D05 the commit timestamp big-endian | the golden encodings, the seeds |
| D06 the flags guard removed | `unknown_flags_are_refused` |
| D07 the trailing-bytes guard removed | `bytes_after_the_splits_are_refused` |
| D08 the posting timestamps swapped on decode | the golden decodes, the seeds |
| D09 `encoded_len` without the splits | `the_encoded_length_is_the_length_of_the_bytes` |
| D10 `encoded_len` without a facet's length | the same |
| D11 the pack flag always set | the goldens, `a_list_without_a_pack_and_one_with_an_empty_pack_differ` |
| D12 an unknown posting type read as a value | `an_unknown_posting_type_is_refused` |
| D13 a facet key read lossily | `a_facet_key_that_is_not_utf8_is_refused` |
| D14 a field read one byte short | 14 tests |
| D15 lengths written in two bytes | `a_long_value_round_trips` |
| D16 the error naming no key | 9 tests |
| D17 the checksum seeded with 1 | the goldens and 5 more |
| D18 statement IDs dropped on decode | the golden decodes, `boundary_values_round_trip` |
| D19 `Posting::start_ts` returning `commit_ts` | `a_posting_returns_what_it_was_built_with`, the goldens |
| D20 `with_pack` dropping the pack | the posting-list tests, the goldens |
| D21 the length checked before the version | `an_unknown_version_is_refused_before_the_checksum` |
| D22 a block's base changed with its count | the golden decodes |
| D23 the overrun offset off by one | `a_count_past_the_end_is_refused`, `a_length_past_the_end_is_refused_without_reading_it` |
| D24 an empty pack read as no pack | the golden decodes, the pack-absent test |
| D25 `List::into_uids` dropping a UID | `a_list_holds_its_uids` |
| D26 `Facet::new` dropping the key | `a_facet_holds_its_key`, the goldens |

All 26 caught. A first D25 (`truncate` to the length) was no defect at all and was replaced. The audit found one
defect in the suite itself before it ran: `the_blocks_keep_the_encoders_deltas_byte_for_byte` fed the encoder an
unsorted list; it now uses an ascending one. Deviation from the protocol: the audited implementation is the one kept
in phase 4, with each defect reverted, rather than a throwaway rewritten.

### 4. Implementation

The format, in `src/types/posting_list/posting_list_codec.rs`, little-endian: version 1; flags, bit 0 for a pack;
the commit timestamp; with a pack its block size, block count and per block its base, UID count and deltas (u32
length and the bytes, as the encoder writes them); the posting count and per posting its uid, op, type, start and
commit timestamps, value, language tag, facets (each key a u32 length and UTF-8 bytes) and statement IDs; the split
count and splits; the XXH3-64 (seed 0) of every byte before it. Decoding checks the version, then the length, then
the checksum, and only then parses, every read bounded, nothing allocated ahead of the bytes that back it. Every field
is fixed, so a list has one encoding. Cargo: unit 64 passed, BDD 10 scenarios passed.

#### 1.5 The crates above

- `core_algo`, `store_posting`, `store_quads`, `store_apply`, `store_hnsw` import the types from `core_codec`;
  `ultragraphdb_proto_pb`, `ultragraphdb_proto_dgraph` and `prost` left their manifests (`store_quads`,
  `store_apply` and `store_hnsw` take `core_codec` as a direct dependency), and `with_proto_bindings` left their
  BUILD files and `core_codec`'s, `core_types`' and `core_tok`'s.
- `store_posting/src/utils/posting_codec.rs` keeps `marshal`, `unmarshal_delta` and `unmarshal_complete`: the
  bytes are `core_codec`'s format, and the checks of a list's structure stay here because they need the key's
  keyspace and the posting operations (a delta has no pack or splits, postings sorted by identity and in the pack,
  splits ascending from 1, known operations). No file moved, so there was no `git mv`.
- Behaviour changes, each from the format: empty bytes are no longer an empty complete list but corruption (no
  writer stores empty bytes as a complete version; an empty list is `BIT_EMPTY_POSTING` and not decoded); a cached
  list weighs 26 bytes more for the empty list's encoding; `should_split`'s encoded size is the new format's, so a
  list splits at a different number of postings than under protobuf; the backup's entry holds the rolled-up list in
  the posting format, where Dgraph's holds the backup list's protobuf.
- Tests that encoded protobuf by hand now encode the posting format by hand (python-xxhash checksums); the
  `val_type` refusal and an unrepresentable posting type left `posting_codec_tests.rs`; a test that every flipped
  byte of a stored delta is refused naming its key was added.

#### 1.6 The query-layer functions

`core_types`' `object_value` (with `to_binary`, `expected` and `go_value`, which only it used),
`get_geo_tokens`, `match_geo` (with `args_error`) and `sort_with_facet` (its body is now `sort`'s, the facets
field and check of `ByValue` and `check_shape` gone), and `core_tok`'s `get_index_factory_from_spec`, with their
tests: six test functions of `core_types` (`test_object_value`, `test_object_value_password_error_does_not_leak_value`,
`test_get_geo_tokens_refusals`, `test_match_geo_refuses_what_is_not_stored_geo`, `test_golden_geo_queries`,
`test_sort_with_facet_moves_facets_with_their_rows`) and the `get_geo_tokens` and `sort_with_facet` parts of two
others. `core_tok`'s `get_factory_create_spec_from_spec`, `get_index_factory_opts_from_spec` and
`get_index_factories_from_specs` also took `pb::VectorIndexSpec` and are not on the approved list, so they were
retyped, not removed: a factory name and its option pairs, the shape `core_mapping::VectorIndexSpec` holds; their
golden test of 46 hnsw option cases is unchanged. `core_types/testdata/golden/types_geo_query.json` (6 MB) and its
generator `tools/golden/types_geo.go`'s `types_geo_query` are now read by no test and were left in place.

### Counts

| Crate | Cargo | Bazel |
|---|---|---|
| `core_codec` | unit 64, BDD 10 scenarios | the same |
| `core_algo` | unit 83, BDD 7 | the same |
| `core_types` | unit 334 (340 before, 6 removed), BDD 22 | the same |
| `core_tok` | unit 96, BDD 21, doc 6 | the same |
| `store_posting` | unit 237 (236 before, 1 added), BDD 32, doc 1 | the same |
| `store_quads` | unit 13, BDD 20, doc 1 | the same |
| `store_apply` | unit 101, the three workload targets 1 each, BDD 71, doc 1 | the same |
| `store_hnsw` | unit 92, BDD 7 | the same |

`bazel test` of the eight crates' packages: 27 targets, 27 passed.

### 5. Mutation testing and fuzzing: deferred to group 11

The user moved mutation testing and fuzzing to the stage's final verification (group 11). The fuzz target and its
seeds are in place for that run:

- `ultragraphdb_core_codec/fuzz/`, as `cargo fuzz init` lays it out, a workspace of its own (no member of the
  repository's, no BUILD file), pinning `libfuzzer-sys` 0.4.13, `arbitrary` 1.5.0 and `xxhash-rust` 0.8.19. The
  target `posting_list_decode` decodes each input twice, as given and with its last eight bytes replaced by the
  checksum of the rest, so the parse behind the checksum is reached; it asserts that a decode returns a list or
  `Corrupted` naming the key, never another error and never a panic, and that an accepted list encodes back to the
  bytes it came from. `cargo +nightly fuzz build --fuzz-dir ultragraphdb_core/ultragraphdb_core_codec/fuzz` passes.
- Seeds: the four golden encodings of the suite, in `testdata/fuzz_seeds/posting_list_decode/` (`empty`,
  `empty_pack`, `delete_all`, `full`); `the_fuzz_seeds_are_valid_lists` checks they decode and re-encode.

Two runs had finished before the change arrived; their results are for group 11 to start from, not resolved here:

- `scripts/fuzz.sh ultragraphdb_core_codec` with `FUZZ_SECONDS=120`, the script's first real run: 17,244,537
  executions, coverage 741, 960 new units, peak RSS 715 MB, no failure, exit 0; the run path, the seeds directory and
  the report lines worked unchanged. No regression case exists, so no replay test was added.
- `cargo mutants -p ultragraphdb_core_codec -j 8` (the script's command, with `--output` to a scratch directory so a
  concurrent run elsewhere kept its `mutants.out`): 403 mutants, 358 caught, 16 timeouts (each a mutation that loops,
  such as an offset that stops advancing), 7 unviable, 22 missed. No survivor is in the code this group added: all
  22 are in the UID codec that predates it. 14 in `codec.rs` (`approx_len` × 2; `put_uvarint` × 3 and `uvarint` × 9,
  which no test pins byte for byte), 6 in `decoder/mod.rs` (`unpack_block`'s `<` to `<=`, `seek_to_block`'s
  backward-seek guard × 3 and the base-equals-target branch of `seek_to_block` and `seek`, each of which looks
  equivalent on reading: the other path returns the same block), 1 in `groupvarint.rs` (`|` to `^` over disjoint
  bits, equivalent) and 1 in `encoder/mod.rs` (`Encoder::block_size` to 0, which no test pins at a non-zero size).

### 1.7 The proto crates

Approved by the user. Removed with `git rm -r`: `ultragraphdb_proto/` (`ultragraphdb_proto_pb`,
`ultragraphdb_proto_dgraph`), `bazel/toolchains/prost_toolchain`, `bazel/toolchains/proto_toolchain` and
`bazel/rust/deps.bzl`. `proto_toolchain` was not named in the approval: it compiles `@com_google_protobuf//:protoc`,
so it cannot stay once `protobuf` leaves `MODULE.bazel`, and it served only the proto crates. Removed from the root
`Cargo.toml`: the `ultragraphdb_proto/*` member glob, the two path entries and the pins of `prost`, `tonic`,
`tonic-prost` and `tonic-prost-build` (`ultragraphdb_common_telemetry` names neither `tonic` nor `prost`; it reaches
gRPC through `opentelemetry-otlp`'s `grpc-tonic` feature). Removed from `MODULE.bazel`: the `protobuf` bazel_dep, the
`rules_rust_prost` extension and its `use_repo`, and the `register_toolchains` of the prost and proto toolchains.
`with_proto_bindings` left the BUILD files of `core_catalog`, `core_text`, `core_keys`, `core_terms`,
`core_mapping`, `tool_test` and `tool_bench` too, which had wrapped their dependencies in it without depending on a
proto crate.

Left in place: `MODULE.bazel`'s `rules_proto` bazel_dep, which nothing uses any more; `bazel/rust/BUILD.bazel`, an
empty package now; `AGENTS.md`'s rows and text naming the proto crates (task 13.1).

Checked: `cargo build --workspace` passes; `cargo tree -i tonic` and `cargo tree -i prost` lead only to
`opentelemetry-otlp` (and its `opentelemetry-proto`, `tonic-prost`, `tonic-types`) under
`ultragraphdb_common_telemetry`; `Cargo.lock` holds no proto crate and no `tonic-prost-build`. `bazel build` of
every group folder but `ultragraphdb_transactions/` and `ultragraphdb_server/` (168 targets) exits 0: those two fail
to load because another group's `ultragraphdb_transactions_log/BUILD.bazel` globs an empty `testdata/`, unrelated to
this change. `bazel test //ultragraphdb_core/... //ultragraphdb_store/... //ultragraphdb_tool/...`: 59 targets, 59
passed.
