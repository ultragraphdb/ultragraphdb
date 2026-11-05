# Group 2: keys and ID lists

Three existing crates move to 128-bit system IDs with no compatibility layer: `ultragraphdb_core_keys`,
`ultragraphdb_core_codec` and `ultragraphdb_core_algo`, each now depending on `ultragraphdb_core_ids`. The fuzz packages
are `fuzz/ultragraphdb_core_keys` (new) and `fuzz/ultragraphdb_core_codec` (a target added).

## 2.1 The Ultra Style lift, its own step

The lift was made, and the suite run under Bazel, before any behaviour changed. Its diff is kept apart in the session
scratchpad (`group2/lift.diff`, 7 files, +155 −97); it lands in this group's commit, not in a commit of its own as the
task's text asks (the brief for this wave asks for the group's commit). If the history should show it apart, the user
can commit that diff first.

Bazel before and after, the same counts:

| Crate | Unit | BDD | Doc |
|---|---|---|---|
| `core_keys` | 12 | 9 scenarios, 18 steps | 1 |
| `core_codec` | 71 | 10 scenarios, 39 steps | — |
| `core_algo` | 89 | 7 scenarios, 18 steps | — |

`cargo fmt -p` changed nothing and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` reported nothing, before and
after. The rules no lint checks, reviewed:

- **Rule 1 (acyclic control flow).** `core_algo` had two recursions. `bin_intersect`, Go's recursive binary
  intersection, now walks an explicit stack of `BinTask`s (a pair of slices, the longer first, or a match to append once
  the left halves are done), bounded by `BIN_INTERSECT_STACK_MAX = 2 × 65 + 1`: the probed half at least halves at
  each level, so a path is at most 65 deep, and each level leaves at most a right half and a match pending. The output
  order is the recursion's. `merge_sorted_more_mem` recursed into 10 shares; it now merges each share on its thread
  and the 10 results once, the same union with a call graph of depth one. No recursion in `core_keys` or `core_codec`.
- **Rule 2 (bounded loops).** Every `loop` became a `while` whose condition names its end, each with a comment saying
  why it ends (`intersect_compressed_with_bin`'s two loops, `intersect_with_lin_packed`, `Uint64Heap::down`). The other
  loops iterate collections or ranges, or advance an index bounded by a length.
- **Rule 3 (data-plane memory).** Reviewed, not changed in the lift: `QuadKey::to_bytes` and `key_prefix` allocate per
  call, a `Decoder` its ID buffer once, the posting decoder per field. Taking caller buffers is an API change, left to
  the store's groups; the new code allocates with capacities bounded by the input (see phase 4).
- **Rule 5 (paired checks).** `debug_assert!`s added for the set operations' preconditions (sorted inputs) and
  postconditions (sorted, no duplicate after a merge, a difference no longer than its first list), and for the key
  parser and split key. No release check (`invariant`) was added: none of the three crates depends on
  `ultragraphdb_error_invariant`, and the decoders return their own errors for bad bytes.
- **Rule 7 (no implicit panics in decoders).** The key parser read through `key[..end]` slices and the posting decoder
  through `take(1)?[0]` and `b[0..8]` indexing; both now read fixed arrays through `get` and `try_from`, and the
  decoder's `blocks[idx]` became `get(idx)`.
- **Rules 8 and 9.** No `cfg`, no `Arc`, no interior mutability, no `unsafe`. The two existing
  `#[expect(clippy::disallowed_methods)]` on `core_algo`'s threaded merges stay, with their reason.

## Phases

### 1. Surface

Every changed item with a bare `unimplemented!()` body, no `#[expect]`; constants are values (`UidPack::VERSION`,
`PostingList::VERSION`, `SPLIT_FLAG`). Items whose behaviour did not change kept their bodies (`CountDirection`,
`Facet`, `PostingType`, the unchanged getters of `Posting` and `PostingList`). The surfaces built under Bazel and Cargo.

- `core_keys`: `Graph` (`Default`, `Named(SystemId)`, `to_be_bytes`), `SplitStart` (`Id`, `Fingerprint`), `Keyspace`
  with `Marks` (0x19) and `Referrers` (0x1A) and `has_graph`, `QuadKey` over `SystemId` and `Graph` with `Mark` and
  `Referrers`, `split_bytes(SplitStart)`, `parse` returning `Option<SplitStart>`, `key_prefix` over `Graph` and
  `SystemId`, `Region`; `QuadKeyError` with `InvalidId` (in place of `ZeroUid`), `GroupRange` and `FingerprintStart`.
- `core_codec`: `encode` and `Encoder::add` returning `Result`; `UidBlock::new` and `UidPack::new` checked; `UidPack`'s
  `encode`, `encode_into`, `encoded_len`, `decode`; `Decoder` over `SystemId` with 128-bit targets; `List` of
  `SystemId`; `Posting` keyed by `u128` with `SystemId` statement IDs; `PostingList` and `BackupPostingList` splits as
  `u128`; the posting format, version 2; `CodecError` with eleven variants.
- `core_algo`: every operation over `SystemId`, targets `u128`; `IdHeap` (renamed from `Uint64Heap`, file moved with
  `git mv`) and `Elem`, no longer `Copy` (24 bytes, above Ultra Style's 16).

### 2. Suite

Scenarios, verbatim from the change's specs, bound by each crate's `bdd` target (`fail_on_skipped`):

- `core_keys/features/key-encoding/quad_key_encoding.feature`: key-encoding, all three requirements, 9 scenarios, the
  referrer key and the mark key included.
- `core_keys/features/quad-storage/quad_storage_keys.feature`: quad-storage's "Forward keys of a predicate sort by
  subject" (the rewritten text), "Two subjects of one predicate share a region" and "Referrer keys share their
  tenant's region", the scenarios the keys decide alone.
- `core_keys/features/port-testing/port_testing.feature`: unchanged.
- `core_codec/features/uid-lists/codec.feature`: "ID lists pack into the repository's own format" (3), "Packed lists
  decode, seek and round-trip" (2), and of "Decoders seek without panicking or hanging" the 4 the codec decides.
- `core_codec/features/posting-lists/posting_format.feature`: the requirement's 3 scenarios; "A list of the previous
  format is refused" reads stage 5's own golden encoding of the empty list.
- `core_algo/features/uid-lists/algo.feature`: "Sorted-set operations over UID lists" (2), "Packed set operations equal
  the plain ones" (4) and "Intersecting from past a pack's last ID returns nothing".

Golden values (task 2.4): `core_codec/testdata/golden/formats.py` writes `id_lists.json` (16 packs and 4 lists out of
order) and `posting_lists.json` (4 lists in the posting format, version 2) from the formats' specifications alone,
independent of the codec: system IDs from their bit layout, LEB128 from its definition, the pack's and the posting
format's bytes from their layouts, the checksum with python-xxhash 4.0.2 over the xxHash C library 0.8.3 (Python
3.12.0, a scratch venv run with `python3 -I`; its XXH3-64 of the empty input is the spec's `0x2d06800538d394c2`). Read
with `ultragraphdb_tool_bdd::load_golden`; `dgraph_commit` says they are not from Dgraph. Hand-computed bytes besides:
the keys' bytes (`10 | 00×7 85 | 00×16 | 00 01 00×7 07 00 00 07 00 00 00 | …`), the mark's `02 AB CD`, LEB128 deltas
(`80 80 80 80 08`, `01 F8 FF FF 07`, `81 80 80 08`), a 128-bit buffer delta of 17 bytes.

Corner cases enumerated before the suite, each with its test:

| Case | Test |
|---|---|
| keys: the default graph's 16 zero bytes, and the same bytes in an ID field | `test_16_zero_bytes_are_the_default_graph_and_nothing_else` |
| keys: every ID field invalid three ways (zero sequence, zero prefix, a reserved bit) | `test_an_invalid_id_is_refused_in_every_field_naming_it` |
| keys: IDs at their bounds, tenant `u64::MAX` | `test_ids_at_their_bounds_round_trip` |
| keys: groups 0 and 2^18 − 1 held; 2^18 and 2^24 − 1 refused | `test_a_mark_holds_every_group_of_18_bits`, `test_a_mark_group_past_18_bits_is_refused` |
| keys: split starts of 0 to 7, 9 to 15, 17, 24 and 32 bytes; ID starts at their bounds; fingerprints 0, 1, `u64::MAX` | `test_a_split_start_of_neither_8_nor_16_bytes_is_refused`, the two split round trips |
| keys: a value-index key's fingerprint start, its invalid ID start, a flag with no room for a start | `test_a_value_index_key_splits_at_an_id_alone`, `test_a_split_start_that_is_no_valid_id_is_refused`, `test_a_split_flag_with_no_room_for_a_start_is_truncated` |
| keys: every proper prefix of every key; each of the 15 inner cuts of two IDs; a byte after each key | `test_every_proper_prefix_…`, `test_a_cut_inside_any_id_is_truncated_naming_the_key`, `test_a_byte_after_any_key_but_a_value_index_key_is_refused` |
| keys: an empty token, an empty key, the flag on a keyspace without lists | the key round trip, the error tests, `test_only_a_list_key_splits` |
| codec: an empty list, one ID, block sizes 0 to 3 and 256, an exact multiple | the golden cases, `test_block_size_zero_packs_one_id_per_block`, `test_a_one_id_block_holds_no_delta` |
| codec: a delta of every LEB128 length from 1 to 13 bytes and the largest, 15 | the golden cases |
| codec: across bucket, sequence and type, a carry into the next type valid and into a zero sequence | the golden case, `test_a_delta_reaching_an_invalid_id_is_refused` |
| codec: out of order equal, below, at position 1 and across a block boundary; the encoder unchanged after | the golden cases, `test_a_refused_id_leaves_the_encoder_as_it_was` |
| codec: a block of 0 IDs with and without deltas, fewer deltas than its count, a cut delta, bytes after, a delta of 16 bytes and of exactly 15 at the end, a redundant group, a zero delta, a delta past 104 bits | `uid_block_tests` |
| codec: blocks equal, overlapping, falling and around an empty block | `test_blocks_must_rise_above_the_block_before` |
| codec: the empty pack, every prefix of a pack, versions 0, 2 and 255, a trailing byte, counts at 2^32 − 1 | `uid_pack_codec_tests` |
| codec: targets 0, `u128::MAX`, past the end, no valid ID (an ID's value ± 1, type 0), backward, on an empty block | `decoder_tests`, the decoder scenarios |
| codec: buffer varints cut, of 19 and 20 bytes, past 128 bits, a sum past 2^128, redundant, the value 1, a zero delta | `codec_tests` |
| codec: posting format version 0, 1 (stage 5's), 3 and 255; every truncation and bit flip; counts and lengths at 2^32 − 1; an invalid statement ID; a corrupt pack | `posting_list_codec_tests` |
| algo: Go's tests over IDs; duplicates in plain lists; lists across types and buckets against a set oracle; intersections from past the end; empty blocks in every packed operation | `uidlist_tests`, `packed_tests` |

Every error variant is provoked through the public API and asserted by variant. `QuadKeyError`: `Truncated` (prefixes),
`TrailingBytes` (a byte after a key), `UnknownKeyspace` (byte 0x42, the flag on a keyspace without lists), `InvalidId`
(every ID field), `UnknownDirection` (direction 7), `BadObject` (tag 0x7f), `VectorObject` (a vector literal),
`NotAList` (`split_bytes` of a statement key), `GroupRange` (group 2^18), `FingerprintStart` (a value-index key).
`CodecError`: `OutOfOrder` (`encode`, `Encoder::add`, a zero delta in a block and in a buffer), `TruncatedVarint`,
`OverflowingVarint` and `NonCanonicalVarint` (a block's deltas, a buffer), `DeltaOverflow` (a block), `InvalidId` (a
buffer's value 1, a block's carry, a pack's first ID), `TrailingDeltas` (a block), `Truncated`, `UnknownVersion` and
`TrailingBytes` (a pack's bytes), `BlocksOutOfOrder` (`UidPack::new`, a pack's bytes).

Run against the surface under Bazel (logs in the scratchpad, `group2/phase2_*.log`):

| Crate | Unit | BDD |
|---|---|---|
| `core_keys` | 35 tests, 33 failed | 13 scenarios, 12 failed |
| `core_codec` | 87 tests, 84 failed | 12 scenarios, 12 failed |
| `core_algo` | 29 tests, 29 failed | 7 scenarios, 7 failed |

Every failure was the `not implemented` panic of a body in `src/`; the keys' doc test failed the same way. The tests
that passed test unchanged or derived behaviour: `CountDirection`, the derived order of `Graph`, the port-testing
scenario (no golden file under the keys), `Facet`, `PostingType`, and `PostingList::VERSION`, a constant.

### 3. Audit

57 defects, each planted alone in the implementation, the crate's suite run under Bazel, the file restored (the script
and its results in the scratchpad, `group2/audit/`). The first run left two survivors:

- C04 (a varint of exactly its longest length, still promising another group, called truncated rather than
  overflowing): the suite had no varint of exactly 19 or 15 bytes ending the input. Two tests were added
  (`[0x80; 19]` as a buffer, 15 continuation bytes ending a block's deltas); C04 now fails both.
- A10 (packed lists intersected in their given order rather than shortest first): equivalent. The port intersects
  every list (ledger 1), and intersection is commutative; the sort is a performance heuristic, smallest first.

The final run, over the implementation as it stands (every defect but A10 fails at least one test whose subject is the
defective behaviour):

| Defect | Unit tests failing | Scenarios failing |
|---|---|---|
| K01 the default graph written as 16 one bytes | `test_the_default_graph_is_16_zero_bytes`, `test_16_zero_bytes_are_the_default_graph_and_nothing_else`, `test_a_byte_after_any_key_but_a_value_index_key_is_refused` and 10 more | `A forward key round-trips`, `A literal object round-trips` and 2 more |
| K02 the marks keyspace given the statistics' byte | `test_each_keyspace_has_its_byte_and_properties`, `test_each_keyspace_byte_reads_back_with_and_without_the_split_flag`, `test_a_mark_group_past_18_bits_is_refused` and 7 more | `A mark key round-trips and stays apart from object predicates` |
| K03 the referrers' byte read as no keyspace | `test_each_keyspace_byte_reads_back_with_and_without_the_split_flag`, `test_a_byte_after_any_key_but_a_value_index_key_is_refused`, `test_an_invalid_id_is_refused_in_every_field_naming_it` and 2 more | `A referrer key round-trips`, `Referrer keys share their tenant's region` |
| K04 marks keys said to name a graph | `test_each_keyspace_has_its_byte_and_properties`, `test_a_prefix_is_the_keyspace_tenant_graph_and_predicate_bytes`, `test_every_keyspace_round_trips_and_reports_its_fields` | — |
| K05 a mark's group written in 4 bytes | `test_every_proper_prefix_of_every_key_is_refused_unless_a_shorter_token_makes_it_a_key`, `test_a_mark_holds_every_group_of_18_bits`, `test_every_keyspace_round_trips_and_reports_its_fields` and 2 more | `A mark key round-trips and stays apart from object predicates` |
| K06 the group bound off by one | `test_a_mark_group_past_18_bits_is_refused` | — |
| K07 16 zero bytes not read as the default graph | `test_16_zero_bytes_are_the_default_graph_and_nothing_else`, `test_unreadable_objects_unknown_keyspaces_and_directions_are_refused`, `test_a_byte_after_any_key_but_a_value_index_key_is_refused` and 7 more | `A forward key round-trips`, `A literal object round-trips` and 2 more |
| K08 a forward key's subject refusal naming the predicate | `test_an_invalid_id_is_refused_in_every_field_naming_it` | — |
| K09 a fingerprint start refused | `test_a_list_key_but_a_value_index_key_splits_at_a_fingerprint` | — |
| K10 a value-index key split at a fingerprint | `test_a_value_index_key_splits_at_an_id_alone` | — |
| K11 a short split start called trailing | `test_a_split_start_of_neither_8_nor_16_bytes_is_refused` | — |
| K12 the split flag accepted on a keyspace without lists | `test_only_a_list_key_splits`, `test_the_region_of_a_key_that_does_not_parse_is_refused` | — |
| K13 a prefix naming a graph its keyspace does not have | `test_a_prefix_is_the_keyspace_tenant_graph_and_predicate_bytes` | — |
| K14 the predicate left out of a key | `test_a_cut_inside_any_id_is_truncated_naming_the_key`, `test_an_invalid_id_is_refused_in_every_field_naming_it`, `test_a_byte_after_any_key_but_a_value_index_key_is_refused` and 9 more | `A forward key round-trips`, `An invalid subject ID is refused` and 5 more |
| K15 a region without its graph | `test_a_region_is_the_keyspace_tenant_graph_and_predicate_of_its_key` | — |
| K16 bytes after the fields accepted | `test_a_byte_after_any_key_but_a_value_index_key_is_refused` | — |
| K17 a value-index start refusal naming another field | `test_a_split_start_that_is_no_valid_id_is_refused` | — |
| K18 a truncation not naming the key | `test_each_error_names_the_keys_bytes` | `Every prefix of a 128-bit ID field fails cleanly` |
| K19 a vector object accepted | `test_unreadable_objects_unknown_keyspaces_and_directions_are_refused` | — |
| K20 a count written little-endian | `test_a_list_key_but_a_value_index_key_splits_at_a_fingerprint`, `test_a_list_key_splits_at_an_id_and_parses_back_with_it`, `test_every_keyspace_round_trips_and_reports_its_fields` | — |
| C01 LEB128 without its continuation bit | `test_a_buffer_value_that_is_no_valid_id_is_refused`, `test_approx_len_is_the_block_count_times_the_block_size`, `test_buffer_deltas_are_leb128_of_the_ids_values` and 29 more | `A list round-trips`, `A flipped byte is corruption` and 8 more |
| C02 a redundant last group accepted | `test_a_buffer_varint_with_a_redundant_group_is_refused`, `test_a_delta_with_a_redundant_group_is_refused` | — |
| C03 a varint past 128 bits by one bit accepted | `test_a_buffer_varint_past_128_bits_is_refused` | — |
| C04 a varint at its longest called truncated | `test_a_buffer_varint_past_128_bits_is_refused`, `test_a_delta_longer_than_15_bytes_overflows` | — |
| C05 the deltas over the first 105 bits | `test_approx_len_is_the_block_count_times_the_block_size`, `test_buffer_deltas_are_leb128_of_the_ids_values`, `test_a_new_decoder_holds_the_first_block` and 37 more | `A list round-trips`, `A flipped byte is corruption` and 8 more |
| C06 a zero delta accepted | `test_a_zero_delta_is_out_of_order` | — |
| C07 a delta past 104 bits accepted | `test_a_delta_carrying_an_id_past_104_bits_is_refused` | — |
| C08 bytes after the last delta accepted | `test_bytes_after_the_last_delta_are_refused` | — |
| C09 an empty block with deltas accepted | `test_an_empty_block_holds_no_delta` | — |
| C10 an equal ID accepted by the encoder | `test_a_buffer_value_that_is_no_valid_id_is_refused`, `test_a_refused_id_leaves_the_encoder_as_it_was`, `test_a_list_out_of_order_is_refused_at_its_first_position_out_of_order` | `A list out of order is refused` |
| C11 a block closing one past the block size | `test_a_new_decoder_holds_the_first_block`, `test_seek_to_block_returns_the_target_block_whole`, `test_unpack_block_returns_the_whole_block_after_a_seek_into_it` and 12 more | `A block closes at the block size`, `Decoding and seeking from a target` and 2 more |
| C12 a pack written with version 2 | `test_a_list_without_a_pack_and_one_with_an_empty_pack_differ`, `test_a_list_appended_to_a_buffer_is_its_golden_bytes_after_what_was_there`, `test_boundary_values_round_trip` and 13 more | `A list round-trips` |
| C13 a block starting at the previous block's last accepted | `test_blocks_out_of_order_are_refused`, `test_blocks_must_rise_above_the_block_before` | — |
| C14 bytes after a pack accepted | `test_bytes_after_the_last_block_are_refused` | — |
| C15 Current taken as Start | `test_seek` | — |
| C16 an empty block not stepped over | `test_seeks_step_over_an_empty_block` | — |
| C17 a linear seek stopping before a block it reaches | `test_seeks_step_over_an_empty_block`, `test_linear_seek` | — |
| C18 a seek keeping the ID before the target | `test_unpack_block_returns_the_whole_block_after_a_seek_into_it`, `test_a_target_that_is_no_valid_id_seeks_by_its_value`, `test_seek` and 2 more | `Decoding and seeking from a target` |
| C19 a posting's key written little-endian | `test_a_list_appended_to_a_buffer_is_its_golden_bytes_after_what_was_there`, `test_a_list_encodes_to_its_golden_bytes`, `test_golden_bytes_decode_to_their_list_and_back` and 2 more | `A list round-trips` |
| C20 the posting format's version 1 | `test_a_corrupt_pack_is_corruption_naming_the_key`, `test_a_length_past_the_end_is_refused_without_reading_it`, `test_a_count_past_the_end_is_refused` and 13 more | `A list of the previous format is refused` |
| C21 the checksum not checked | `test_a_flipped_bit_fails_the_checksum`, `test_every_bit_flip_is_corruption_naming_the_key` | `A flipped byte is corruption` |
| C22 a split counted as 8 bytes | `test_a_list_appended_to_a_buffer_is_its_golden_bytes_after_what_was_there`, `test_a_list_encodes_to_its_golden_bytes`, `test_boundary_values_round_trip` and 2 more | `A list round-trips`, `A flipped byte is corruption` |
| C23 a buffer's running sum wrapping | `test_a_buffer_varint_past_128_bits_is_refused` | — |
| C24 the buffer's first delta from 1 | `test_a_buffer_value_that_is_no_valid_id_is_refused`, `test_buffer_deltas_are_leb128_of_the_ids_values`, `test_every_golden_list_round_trips_through_a_buffer` | — |
| C25 the exact length counting blocks | `test_approx_len_is_the_block_count_times_the_block_size`, `test_buffer_deltas_are_leb128_of_the_ids_values`, `test_a_new_decoder_holds_the_first_block` and 26 more | `A list round-trips`, `A flipped byte is corruption` and 8 more |
| A01 the binary intersection dropping its matches | `test_operations_across_types_and_buckets_match_a_plain_oracle`, `plain_operations_match_golden`, `test_uid_list_intersect_by_every_strategy` | `Intersection of packed and plain lists agrees` |
| A02 longer_first taking the first of two equal slices | `plain_operations_match_golden` | — |
| A03 a difference dropping its first list's tail | `test_difference_packed`, `test_packed_operations_step_over_an_empty_block`, `test_difference` and 3 more | `A packed difference reads every block of the first list`, `Subtracting an empty pack keeps the first list` |
| A04 a merge keeping duplicates | `test_merge_sorted`, `test_operations_across_types_and_buckets_match_a_plain_oracle`, `plain_operations_match_golden` | `Merging several lists removes duplicates` |
| A05 a packed merge keeping duplicates | `test_merge_sorted_packed_takes_the_first_present_lists_block_size`, `test_packed_operations_step_over_an_empty_block`, `test_merge_sorted_packed` and 1 more | `Merging several lists removes duplicates` |
| A06 an intersection from past the pack's end going on | `plain_operations_match_golden` | — |
| A07 index-of missing a block's IDs | `test_index_of_packed`, `packed_operations_match_golden` | — |
| A08 a max-heap in place of a min-heap | `fix_moves_a_decreased_leaf_up_to_the_root`, `test_merge_sorted_packed`, `push_sifts_each_element_up_past_its_parents` and 10 more | `Merging several lists removes duplicates` |
| A09 apply_filter's index not advancing | `test_apply_filter_uint` | — |
| A10 packed lists intersected in their given order | — (equivalent, see above) | — |
| A11 a jump past an equal ID | `plain_operations_match_golden` | — |
| A12 a packed intersection adding nothing | `test_intersect_sorted_packed`, `test_packed_operations_step_over_an_empty_block`, `test_uid_list_intersect_packed` and 1 more | `Intersection of packed and plain lists agrees`, `Three packed lists intersect to their common IDs` |

Input variety: the scenario values are distinct (predicate (1, 7, 7) against subject (1, 42, 42), entity (1, 42, 9)
with group 0x2_ABCD); the order test pairs IDs whose lower fields run against their higher; the golden lists span two
types and two buckets.

### 4. Implementation

- **Keys.** Every keyspace naming a graph and a predicate names them first, so `to_bytes` writes the graph and the
  predicate from `graph()` and `predicate()` and then the variant's tail; the parser reads each keyspace's fields in a
  method of its own. A split key's start is what follows its fields, 16 bytes an ID and 8 a fingerprint; a value-index
  key's start is its last 16 bytes, read apart, since its token runs to the end. Errors carry the key's bytes.
- **Codec.** `utils/leb128` writes and reads canonical LEB128 of at most a width (19 bytes for 128 bits, 15 for a
  delta's 104), refusing a value past 128 bits, past its width or with a redundant last group. `utils/deltas::walk`
  reads a block's deltas, handing each checked ID on; `UidBlock::new` walks them in full, so a block exists only when
  every ID it holds is valid, and the decoder's unpack cannot fail. A decoder steps over empty blocks in every
  operation that moves it (`next_block`, the seeks, `linear_seek`). `UidPack::read_at` reads a pack at an offset, so the
  posting decoder names offsets in the whole value. Every allocation by a count read from bytes is capped by the bytes
  left.
- **Algo.** The lifted code over `SystemId`; packed operations add to the encoder through one helper that debug-checks
  the order the operations already guarantee. `intersect_compressed_with` returns nothing when no ID lies at or after
  its target (see the decisions).

Corrections of the suite in phase 4, each a test-data error, not a weakened assertion:

1. A decoder test expected `seek_to_block` of a block's last ID with `Current` to return the next block; the spec
   returns "the whole block holding the target", as Go does. The expectation is now the holding block, with a seek one
   past the last ID added for the next block.
2. `test_linear_seek` packed 1,000 IDs, filling its last block; Go's list has 1,001, so its last block holds one ID,
   which the test's second loop relies on. The list is now Go's shape.
3. The round-trip scenario's lists took every fourth ID of a pool sorted by type, sequence and bucket: always bucket 0.
   They take every third now, so they span both buckets, as the scenario says.
4. The golden test's index-of read a second list a single-list case does not have.

Bazel, final:

| Crate | Unit | BDD | Doc |
|---|---|---|---|
| `core_keys` | 35 | 13 scenarios, 26 steps | 1 |
| `core_codec` | 88 | 12 scenarios, 24 steps | — |
| `core_algo` | 30 | 7 scenarios, 14 steps | — |

The codec's unit target fell from 26.8 s to 0.05 s: the round trips of up to 10 million UIDs are now lists of at most
10,000 IDs and the golden files.

`scripts/coverage.sh`: `core_keys` 0 uncovered lines; `core_codec` 1, `core_algo` 3, each demonstrably unreachable:

- `core_codec/src/types/decoder/mod.rs`, the `ids.clear()` after a failed walk: a `UidBlock` exists only through
  `UidBlock::new`, which walks its deltas in full, or the encoder, whose block a debug check walks.
- `core_algo/src/packed.rs`, `merge_sorted_packed`'s `h.pop(); continue;` when a heap element names no decoder: each
  element is pushed together with its decoder and position.
- `core_algo/src/uidlist.rs`, `merge_sorted`'s `return None` for a share starting past the lists: with at least 100
  lists, `9 × ceil(n / 10) ≤ n`.

Two dead branches found by coverage were removed instead (`intersect_with_lin_packed`'s `!valid()` checks: a decoder
holding IDs is on a block; `bin_intersect`'s empty `d`: the longer slice is never empty when the shorter is not), and
one reachable branch gained a test (a linear intersection from a decoder past the end).

`cargo fmt -p` for the three crates and the fuzz packages changes nothing, `cargo clippy -p <crate> --all-targets --
-Wclippy::all` reports nothing for each, and no `unimplemented` warning is left.

### 5. Fuzz

- `fuzz/ultragraphdb_core_keys`, target `quad_key_parse`: any bytes parse or are refused with an error naming exactly
  those bytes; an accepted key encodes back to them, a split key with its start appended. Seeds:
  `core_keys/testdata/fuzz_seeds/quad_key_parse/`, 16 keys of every keyspace, split at an ID and at a fingerprint,
  and invalid ones (a zero sequence, a group past 18 bits, a cut ID), written from the layout.
- `fuzz/ultragraphdb_core_codec`, target `uid_pack_decode` (new): a pack decodes or is refused; an accepted pack
  encodes back to its bytes and decodes to strictly increasing IDs, as many as its blocks count, a decode from an ID
  starting at it. Seeds: the 15 golden packs, checked by `test_the_fuzz_seeds_are_the_golden_packs`.
- `posting_list_decode`, unchanged; its four seeds are now the golden encodings of version 2.

`cargo +nightly fuzz build --fuzz-dir ultragraphdb_core_keys` and `--fuzz-dir ultragraphdb_core_codec` (in `fuzz/`)
built every target; each replayed its seeds once (`-runs=0`: 16, 15 and 4 inputs, no failure). The time-boxed runs of
`scripts/fuzz.sh` belong to group 12.

## 2.3 Removed and retired

Removed with `git rm`, as approved (2026-11-01, confirmed 2026-11-02):

- `tools/golden/codec.go`: the file held the `codec_uidpack` and `codec_seek` generators and their helpers alone; no
  other generator uses them, and `go build` of `tools/golden` still succeeds.
- `ultragraphdb_core/ultragraphdb_core_codec/testdata/golden/codec_uidpack.json`
- `ultragraphdb_core/ultragraphdb_core_codec/testdata/golden/codec_seek.json`

The tests and steps reading them were rewritten in place against the new golden values: `encoder_tests.rs`'s
`encoding_matches_golden_packs` and `golden_packs_decode_to_their_lists`, `decoder_tests.rs`'s four
`*_matches_golden`, and the codec's "Encoding matches the golden vectors" scenario with its steps.

Retired, kept on disk: `algo_setops.json` stays valid for every result whose inputs a pack can hold, Go's number `n`
standing for the ID (type 1, bucket 0, sequence n + 1), an order-preserving map. Retired by the 128-bit format, which
refuses a list holding duplicates: the 130 plain-side results that pack a first list with duplicates
(`intersect_compressed_with*`), the 72 packed results of cases with a list holding duplicates, and with them 2 of the
3 ledger-3 deviations of the plain test and 14 of the 32 deviations of the packed test. Both tests count what they
retire. The HNSW graph vectors were not retired: they are `ultragraphdb_store_hnsw`'s, read by no file of these
crates, and whether 128-bit IDs invalidate them is decided by group 3's HNSW encoding.

## Downstream crates that no longer compile

Built with Bazel, `--keep_going`, each library alone:

- `ultragraphdb_core_catalog`: `QuadKey::Catalog`'s `uid: u64` is now `id: SystemId` (and `key_prefix` takes a
  `Graph` and a `SystemId`).
- `ultragraphdb_store_posting` (55 errors): `List::uids`/`uids_mut` are `ids`/`ids_mut` over `SystemId`;
  `Decoder::uids` is `ids`, its targets `u128`; `UidBlock::base`/`num_uids` are `first`/`count`; `Encoder::add` and
  `encode` return `Result` and refuse a list not strictly increasing; `Posting::uid` is `u128` and its statement IDs
  `SystemId`; `PostingList::splits` are `u128`; `intersect_compressed_with` takes a `u128` target; `QuadKey`'s fields
  are `SystemId` and `Graph`, `split_bytes` takes a `SplitStart` and `parse` returns one. Beyond the types: a pack
  holds valid system IDs alone, so the rollup, which today packs every posting's key, value fingerprints included, must
  pack references only.
- Not compiled, because a dependency above fails, and meeting the same changes in their own code: `ultragraphdb_store_hnsw`
  (`QuadKey::Forward`/`VectorIndex` fields, `key_prefix`, `Posting::with_uid`), `ultragraphdb_store_quads` and
  `ultragraphdb_store_apply` (`QuadKey` variants and `DEFAULT_GRAPH`, `key_prefix`, `Region::of`, statement IDs),
  `ultragraphdb_transactions_txn` (`Keyspace`, `PostingList`), `ultragraphdb_transactions_commit` (`QuadKey::Catalog`),
  `ultragraphdb_tool_bench` and `ultragraphdb_tool_test` (`QuadKey` variants, `DEFAULT_GRAPH`, `key_prefix`).
- Test targets only: `ultragraphdb_protocol_wire` (dev-depends on `core_catalog`, `store_posting`, `store_quads`) and
  `ultragraphdb_store_kv` (dev-depends on `tool_test`); and the fuzz package `fuzz/ultragraphdb_transactions_commit`.

## Decisions for the user's review

1. **A key's object stays an `EncodedTerm`.** An IRI object's form is `core_terms`' UID tag, still 8 bytes until group 3
   moves it to `SystemId`. The key parser no longer checks a UID object for 0 (`ZeroUid` is gone): an ID object's
   validity is left to `EncodedTerm`'s decoding. Until group 3 lands, "every ID in a key is a 128-bit system ID" holds
   for every field but an object's.
2. **Split starts of two widths.** `SplitStart::Id` (16 bytes) or `Fingerprint` (8), read by the length after the
   fields; a value-index key splits at an ID alone (`FingerprintStart`). The two widths make a forward split key with an
   ID start, cut by 8 bytes, a valid key with a fingerprint start: no parser can tell them apart.
3. **The mark's group is a `u32`** in `QuadKey::Mark`, not placement's `BucketGroup`, which a core crate cannot name;
   `to_bytes` writes its low 24 bits, debug-checking it is below 2^18, and the parser refuses 2^18 or more. Moving
   `BucketGroup` into `ultragraphdb_core_ids` would give both crates one type.
4. **Keyspace bytes and kinds.** Marks 0x19 and referrers 0x1A; neither holds lists (a referrer set never splits; a mark
   holds a generation). `Keyspace::has_graph` is new, and `key_prefix` leaves a graph out for the catalog, the marks and
   the referrers, where it used to append one for the catalog if given. The marks' and referrers' regions are their
   tenant's, implemented here as `QuadKey::region` must be total; group 3's task 3.3 names them too.
5. **A posting is keyed by 128 bits:** a reference's system ID, or a value's 64-bit fingerprint, which sorts below every
   ID; splits likewise. A reference and a literal can no longer share a key. The pack holds system IDs alone (see the
   downstream note on the rollup).
6. **The formats' layouts.** The pack: version 1, block size and block count as u32 little-endian, per block its first
   ID's 16 big-endian bytes, its count and its deltas' length as u32 little-endian and the deltas; canonical LEB128 only,
   so a pack has one encoding. The posting format, version 2: little-endian, but every 128-bit value as its 16
   big-endian bytes, a system ID's stored form; the pack embedded with its own version byte; a posting's fixed part 53
   bytes.
7. **Empty blocks are legal and stepped over.** `UidBlock::new` accepts a block of 0 IDs (the spec's empty-block
   scenario); every decoder move steps over it, so a decoder never presents one, unlike Go's `Next`.
   `intersect_with_lin_packed` reads past an empty block, where Dgraph's stops (its packed result now equals the plain
   one, as the spec asks).
8. **An intersection from past the pack's end returns nothing.** The port matched Dgraph on three golden results
   because Go's `SeekToBlock(0)` rewinds to the first block when the plain list begins with UID 0; no system ID is 0, so
   those three are now deviations under ledger 3, with the port's answer, nothing.
9. **Renames and signatures.** `UidBlock::base`/`num_uids` → `first`/`count`; `Decoder::uids`/`peek_next_base` →
   `ids`/`peek_next_first`; `List::uids` → `ids`; `Uint64Heap` → `IdHeap`; `QuadKey::Catalog { uid }` → `{ id }`;
   `Encoder::add`, `encode`, `UidBlock::new`, `UidPack::new` return `Result`. Kept: `UidPack`, `UidBlock`, `List`,
   `Posting::uid`, `commit_ts`, `encode_from_buffer`/`decode_to_buffer` and `BackupPostingList` (LEB128 of 128-bit
   values).
10. **Left for approval:** `core_codec/src/groupvarint.rs` and `core_codec/LICENSE-GROUPVARINT` are no longer used; the
    module is unreferenced and the files stay until the user approves their removal, with the crate's
    `license = "Apache-2.0 AND MIT"`. `tools/golden/main.go`'s comment still names `codec.go` (and `keys.go`, already
    gone) as examples of generator files.
11. **The lift's commit** (2.1) is not separate in the working tree; see 2.1.

## Note on the history

The user's commit `9546228` (group 5) included this group's staged index entries: the three removals above and three
renames made with `git mv` (`uint64_heap` → `id_heap` and its test, `uid_pack_tests.rs` into `tests/types/uid_pack/`),
all with their old contents. Until this group is committed, `HEAD` holds those renames and removals without the code
and tests that go with them, so `core_algo` and `core_codec`'s tests do not build at `HEAD`.

## Files

- `ultragraphdb_core/ultragraphdb_core_keys/`: `Cargo.toml`, `src/` (new `types/graph.rs`, `types/split_start.rs`),
  `tests/` (new `types/graph_tests.rs`), `features/key-encoding/`, `features/quad-storage/`, new
  `testdata/fuzz_seeds/quad_key_parse/`.
- `ultragraphdb_core/ultragraphdb_core_codec/`: `Cargo.toml`, `src/` (new `types/uid_pack/uid_pack_codec.rs`,
  `utils/leb128.rs`, `utils/deltas.rs`), `tests/` (new `types/uid_pack/`), `features/`, new
  `testdata/golden/{formats.py,id_lists.json,posting_lists.json}`, `testdata/fuzz_seeds/` (new `uid_pack_decode/`,
  `posting_list_decode/` regenerated).
- `ultragraphdb_core/ultragraphdb_core_algo/`: `Cargo.toml`, `src/` (`types/id_heap/`), `tests/`, `features/`.
- `fuzz/Cargo.toml`, `fuzz/Cargo.lock`, `fuzz/ultragraphdb_core_codec/` (target `uid_pack_decode`), new
  `fuzz/ultragraphdb_core_keys/`.
- `AGENTS.md` (the three crates' rows; "packed UIDs" out of the list of Dgraph-identical outputs).
- Removed: `tools/golden/codec.go`, `core_codec/testdata/golden/codec_uidpack.json` and `codec_seek.json`.
