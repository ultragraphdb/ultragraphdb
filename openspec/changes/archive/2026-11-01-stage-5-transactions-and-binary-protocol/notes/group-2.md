# Group 2: the header and checksums

## 2.1 Scenarios

`ultragraphdb_protocol_wire/features/wire-protocol/wire_protocol.feature` holds the scenarios of the six requirements
whose decision this crate makes, verbatim: "A golden header round-trips", "A checksum matches the specification's
reference", the four of "Chunked body checksums", "A frame of another cluster is refused", "An unknown command is
refused, not fatal" and "A retired number is refused". Two scenarios of these requirements are not there, because
nothing of them is this crate's: "A refused request keeps the connection" (a stale statement on one stream while
another reads: prepared statements, streams and the server) and "A large result streams" (a 5 MiB read as several
frames, the last flagged: results and the bus). They belong to groups 9 and 10, whose crates own them. For the two
validation scenarios that name the server, the steps check what the wire decides (the `ProtocolError` naming the
cluster or the command, and a later frame validated as before); answering, closing the connection and serving other
connections are bound again by the bus and server in group 10.

## 2.2 Phases 1 to 3

### Surface

- `ultragraphdb_error_protocol`: `ProtocolError(pub ProtocolErrorEnum)`, one variant per rule, in the order they are
  checked: `ShortHeader { len }`, `HeaderChecksum { stored, computed }`, `UnknownCommand { command }`,
  `ReservedNotZero { offset }`, `WrongCluster { expected, found }`, `SizeBelowHeader { size }`,
  `FrameTooLarge { size, limit }`, `LengthMismatch { size, len }`, `BodyLength { len }`,
  `ChunkTable { stored, computed }`, `Chunk { index, start, end }`; a constructor per variant, `Display`, `Error`.
- `ultragraphdb_protocol_wire`: `Header` (the fields a sender chooses, `with_*` setters, `encode(size, checksum_body)`,
  `encode_frame(body)`), `HeaderView` (`new` is the length rule, `verify(cluster)` the header's rules, returning the
  `Header`), `ChunkLayout` (`of_data`, `of_body`, `chunk_range`), `ChunkedBody` (`verify`, `encode`), `FrameView`
  (`validate(bytes, cluster)`: the whole order through the chunks), `Command` (the closed registry, `number`,
  `from_number`, `ALL`), `xxh3_64` and `xxh3_64_with_seed`, and the constants `HEADER_SIZE`, `COMMAND_HEADER_SIZE`,
  `MAX_FRAME_SIZE`, `FIRST_CHUNK_SIZE`, `CHUNK_SIZE`, `LEAF_SIZE`, `PROTOCOL_VERSION` (1), `REPLICA_COMMANDS`
  (256 to 511) and `RETIRED_COMMANDS` (empty).
- Commands 1 to 17 in the body table's order: Hello, Welcome, Register, Begin, Commit, Abort, Read, Write, Operation,
  Prepare, Execute, Describe, Result, Credit, Cancel, Error, Round. 0 is never assigned, so a zeroed header names no
  command.

### Decisions taken here, for review

- **The length rule is applied twice.** `HeaderView::new` refuses fewer than 256 bytes before anything is read; the
  frame's own `size` against the bytes received (`LengthMismatch`) is checked once the header is verified, since `size`
  is not trusted before its checksum. A bus reading the header first, then `size − 256` bytes, can rely on `verify`
  refusing `size < 256` (`SizeBelowHeader`, a rule the spec implies and does not list).
- **Byte ranges of a failed chunk are offsets within the body**, the frame of reference group 9's sections use; chunk
  k ≥ 1 is frame block k, so the frame offset is the body offset + 256.
- **A one-chunk body that fails is `Chunk { index: 0, .. }`**, not `ChunkTable`: it has no table, and its
  `checksum_body` is covered by the header checksum, so the chunk is what broke.
- **No command is retired yet**, so "A retired number is refused" is exercised through the rule: retiring removes the
  variant and records the number in `RETIRED_COMMANDS`; every number outside the registry, all 65,519, is sent and
  refused as `UnknownCommand`, and the constants test asserts `RETIRED_COMMANDS` disjoint from the registry and the
  replica range.

### Independent sources

- XXH3-64: 1,214 vectors (607 lengths × the seeds 0 and 0x9E3779B185EBCA8D) copied from `tests/sanity_test_vectors.h`
  of xxHash v0.8.3 (commit e626a72bc2321cd320e953a0ccf1584cad60f363, blob a5025a2d…) into
  `testdata/xxh3/sanity_test_vectors_xxh3_64.txt`, over the buffer `tests/sanity_test.c`'s `fillTestBuffer` generates.
- Golden frames: `testdata/golden/generate_frames.c`, C over xxHash v0.8.3's `xxhash.h` (blob 78fc2e8d…), lays each
  header out byte by byte from the spec's offsets, splits the body into chunks and writes the leaves (seeded with the
  chunk index), the table and both checksums. Six frames: Hello (100-byte body), Read (every field distinct and
  nonzero; 20,432 bytes in six chunks and a 48-byte table, the 20 KiB body), Credit (empty body), one_chunk (3,840),
  two_chunks (3,841: the smallest table), round (7,936: two full chunks). Cross-checked with Python's `xxhash` 3.6.0
  (libxxhash 0.8.2): every checksum, `checksum_body` and leaf agrees. Credit's `checksum_body` is also
  `XSUM_XXH3_testdata[0]`.
- Values after a corruption (a checksum after a flipped header byte, a table after a flipped byte, the "table" of a body
  with its table cut off) were computed with Python's `xxhash`, not with the crate.
- Body splits: an oracle walking data lengths one at a time and counting chunks as each boundary is crossed, compared
  with `ChunkLayout::of_body` for every body length from 0 to 1,048,320.
- Header bytes of mutated frames are resealed in the tests with `xxhash_rust` directly, not the crate's wrapper.

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| 0, 1, 8, 128, 255 bytes | `header_view_tests::test_fewer_than_256_bytes_is_a_short_header` |
| Exactly a header (Credit, empty body) | `frame_view_tests::test_golden_frames_validate` |
| Every truncation of every golden frame | `frame_view_tests::test_every_truncation_is_refused_cleanly` |
| One byte past the frame | `frame_view_tests::test_bytes_past_the_frame_are_refused` |
| Every bit flip of the header; of a whole frame | `header_view_verify_tests::test_every_bit_flip_in_the_header_fails_its_checksum`, `frame_view_tests::test_every_bit_flip_is_refused` |
| Command 0, 18, 255, 256, 511, 512, 65535; every unassigned number; the replica range | `command_tests` |
| Each reserved byte 16 to 31 and 66 to 127; two at once; bytes 15, 65, 128, 255 not reserved | `header_view_verify_tests` |
| Cluster one bit off; receiver's cluster 0 | `header_view_verify_tests::test_another_cluster_is_refused_naming_both` |
| `size` 0, 1, 255, 256, 4,096, 1 MiB, 1 MiB + 1, 2 MiB, `u32::MAX` | `header_view_verify_tests::test_size_is_at_least_a_header_and_at_most_one_mebibyte` |
| The largest frame built and validated; one byte more; a body of 1,048,320 | `header_encode_tests` |
| Bodies of 0, 1, 100, 3,840, 3,841, 7,936, 7,937, 12,032, 20,432, 1,046,272 bytes | `chunk_layout_tests::test_of_data_splits_as_worked_by_hand`, `..._of_body_...` |
| Body lengths 3,841 to 3,856 and 7,953 to 7,960 (no split) | `chunk_layout_tests::test_lengths_between_splits_are_refused` |
| `usize::MAX` and near it, as body and data lengths | `chunk_layout_tests::test_lengths_past_any_frame_do_not_overflow`, `..._any_data_length_answers_without_overflowing` |
| Chunk 0, a middle chunk, the short last chunk, at their first, middle and last byte | `chunked_body_tests::test_a_corrupted_chunk_is_named_with_its_range` |
| Two chunks corrupted; chunks swapped; table corrupted; table missing; wrong `checksum_body` | `chunked_body_tests` |
| Every byte of a two-chunk body, the table included | `chunked_body_tests::test_every_byte_of_a_body_is_covered` |
| One-chunk and empty bodies with a wrong `checksum_body` | `chunked_body_tests::test_a_one_chunk_body_is_checked_against_checksum_body` |
| Rules broken together, each order pair | `header_view_verify_tests::test_rules_are_checked_in_the_protocols_order`, `frame_view_tests::test_the_length_is_checked_before_the_body`, `..._header_is_checked_before_the_length` |
| Coincidence: Hello's view, stream and replica are all zero | supplemented by Read, every field distinct and nonzero |

Every `ProtocolErrorEnum` variant is constructed through the public API and asserted as that variant:
`ShortHeader` (`header_view_tests::test_fewer_than_256_bytes_is_a_short_header`), `HeaderChecksum`
(`header_view_verify_tests::test_a_changed_checksum_names_both_values`), `UnknownCommand`
(`command_tests::test_unassigned_numbers_are_unknown_commands`, `header_view_verify_tests::test_an_unknown_command_is_refused_by_number`),
`ReservedNotZero` (`header_view_verify_tests::test_each_reserved_byte_must_be_zero`), `WrongCluster`
(`header_view_verify_tests::test_another_cluster_is_refused_naming_both`), `SizeBelowHeader` and `FrameTooLarge`
(`header_view_verify_tests::test_size_is_at_least_a_header_and_at_most_one_mebibyte`; the builder's
`header_encode_tests::test_a_frame_past_one_mebibyte_is_refused_naming_the_limit`), `LengthMismatch`
(`frame_view_tests::test_every_truncation_is_refused_cleanly`), `BodyLength`
(`frame_view_tests::test_a_body_length_with_no_split_is_refused_naming_it`), `ChunkTable`
(`chunked_body_tests::test_a_corrupted_table_trusts_no_chunk`), `Chunk`
(`chunked_body_tests::test_a_corrupted_chunk_is_named_with_its_range`).

### Phase 2: the failing run

`cargo test -p ultragraphdb_error_protocol -p ultragraphdb_protocol_wire --no-fail-fast` against the surface:

- `ultragraphdb_error_protocol` unit: 9 tests, 9 failed.
- `ultragraphdb_protocol_wire` unit: 62 tests, 59 failed, 3 passed. `bdd`: 9 scenarios, 9 failed (1 of 10 steps run
  passed: reading the Hello file).
- Every failure is an `unimplemented!()` panic in `src/`; no compile error and no panic elsewhere.
- The three that passed: `constants_tests::test_sizes_are_the_specs` and `command_tests::test_all_lists_every_command_in_number_order`
  pin constants, which are part of the declared surface; `test_retired_numbers_are_unknown_commands` passed over the
  empty `RETIRED_COMMANDS`, vacuously. It was removed and its rule folded into `test_exactly_the_registry_is_known`,
  which sends every number outside the registry. The suite then had 61 unit tests.

### Phase 3: the audit

A throwaway implementation, its body-length split found by searching chunk counts (a different shape from phase 4's
closed form). Against it the suite failed `test_lengths_past_any_frame_do_not_overflow` (`body_len` overflowed on
`usize::MAX`); the same hazard in `of_data(usize::MAX).body_len()` and `chunk_range`'s last end took a test of its own
(`test_a_layout_of_any_data_length_answers_without_overflowing`), 62 unit tests from then. Both saturate.

42 defects, one at a time, each restored after its run (`cargo test` of both crates); every one failed at least one
test whose subject is the defective behaviour, none survived:

| Class | Defects |
|---|---|
| Off-by-one | chunk count rounding down; range past the last chunk; chunk start; reserved offset + 1; cluster read at 33; replica read at 65; a field reader one byte short; last chunk unchecked |
| Flipped comparison or boundary | a table on one chunk (`>=`); 3,840 not one chunk (`<`); `size` 256 refused (`<=`); 1 MiB refused by the receiver and by the builder (`>=`); trailing bytes accepted (`>`) |
| Constant changed | one chunk too many; the table one leaf short; a one-chunk body seeded 1; leaves seeded `index + 1` on encode; leaves unseeded on verify; the root seeded 1; the checksum over all 256 bytes; the checksummed range from 0 |
| Guard removed | header checksum; reserved bytes; cluster; length mismatch; the last chunk's clipping |
| Early return | the table check skipped |
| Order | cluster checked before command |
| Plausible neighbour | view and release swapped on encode; replica and flags swapped; the chunk index + 1 reported; the arguments of `WrongCluster`, `HeaderChecksum`, `ChunkTable` swapped; `verify` returning release as view; Round numbered 18; Round dropped; a replica number accepted as Hello; `Display` of an inclusive range and of the clusters swapped; a constructor swapping start and end |

Input variety: Hello's zero view, stream and replica coincide; Read gives every field a distinct nonzero value, and the
setter test does the same for `Header`.

## 2.3 Phase 4

The throwaway's split search (and the 1 MiB guard it needed to stay bounded) was replaced by the closed form in
`ChunkLayout::of_body`: n ≥ 2 chunks with their table span 4,104n − 4,351 to 4,104n − 256 bytes, the ranges of
successive n do not meet, so n = ⌈(len + 256) / 4,104⌉, at least 2, is the one candidate. The rest of the throwaway,
reviewed after the audit, is the implementation: its code was written against the suite, and rewriting it word for
word would have audited nothing new. Every read is `from_le_bytes` over `get(..)`; the header is written by
concatenating its fields; no `unsafe`, no index that can panic.

Cargo: `ultragraphdb_error_protocol` unit 9 passed; `ultragraphdb_protocol_wire` unit 62 passed, `bdd` 9 scenarios
(18 steps) passed; doc tests 0. Bazel `//ultragraphdb_error/ultragraphdb_error_protocol:ultragraphdb_error_protocol_tests`
9 passed, `//ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire_tests` 62 passed, `:bdd`
9 scenarios (18 steps) passed, both `doc_test`s pass. `cargo fmt --all -- --check` and `cargo clippy --all-targets` of
both crates clean.

## 2.4 Phase 5

### Mutation testing

- `scripts/mutants.sh ultragraphdb_protocol_wire`: 169 mutants, 144 caught, 0 missed, 0 timed out, 25 unviable.
- `scripts/mutants.sh ultragraphdb_error_protocol`: 12 mutants, 1 caught, 0 missed, 11 unviable.
- Every unviable mutant replaces a body with `Default::default()` or a `Range` constructor that the type does not
  implement (`ProtocolError`, `Command`, `ChunkLayout`, `Header`, `HeaderView`, `ChunkedBody`, `FrameView` have no
  `Default`), so it does not compile; none needs an entry in `.cargo/mutants.toml`, and none was added.

### Fuzzing

`ultragraphdb_protocol_wire/fuzz/`, its own `[workspace]`, no `BUILD.bazel`, `arbitrary` `=1.5.0` and
`libfuzzer-sys` `=0.4.13`. Three targets:

- `frame`: any bytes through `FrameView::validate`; the refusal equals the header's own (`HeaderView::new` then
  `verify`), and an accepted frame is rebuilt byte for byte by `Header::encode_frame`. Seeds: the six golden frames,
  `testdata/fuzz_seeds/frame/`.
- `chunk_table`: the first 8 bytes as `checksum_body`, the rest as a body, through `ChunkLayout::of_body` and
  `ChunkedBody::verify`; a split adds up, a failed chunk's range is the layout's, an accepted body re-encodes to the same
  bytes and `checksum_body`. Seeds: each golden body behind its `checksum_body`, `testdata/fuzz_seeds/chunk_table/`.
- `frame_fields`, structure-aware through `arbitrary`: a valid frame from arbitrary fields and body must validate and
  read back its header and body; then an arbitrary command number, reserved byte, cluster and size are written and the
  header resealed, and the refusal must be the first rule broken in the protocol's order, predicted independently in the
  target; then up to 16 arbitrary flips, truncations, appends, chunk swaps and reseals, which must not panic.

`FUZZ_SECONDS=120 scripts/fuzz.sh ultragraphdb_protocol_wire` on nightly (aarch64-apple-darwin):

| Target | Executions | Coverage | Failures |
|---|---|---|---|
| `chunk_table` | 27,270,572 | 333 | none |
| `frame` | 15,233,028 | 411 | none |
| `frame_fields` | 8,474,937 | 576 | none |

No input failed, so `testdata/fuzz_regressions/` holds nothing and no replay test was added; the first finding adds
the test through `ultragraphdb_tool_bdd::fuzz_regressions("<target>")`, and the unit target's `data` already globs
`testdata/**`.
