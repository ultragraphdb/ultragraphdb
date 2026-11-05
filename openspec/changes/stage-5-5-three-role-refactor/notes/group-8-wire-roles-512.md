# Group 8: `ultragraphdb_protocol_wire`, role commands 512–518

The fork of the wire agent building `PROTOCOL.md` §13.1–13.7: Join, Members, Subscribe, EpochClose, Lease, Placement,
Catalog, each an owned body (`BodyBuilder`) and a view (`BodyView`), with the records `Member`, `CatalogAnnouncement`,
`BucketRange`, `NamespaceRun`, `PreferredLeader` and the enum `RoleKind`. The crate-private helper
`utils/whole_records.rs` reads a section of fixed-size records as arrays (`whole_records`, `record_at`,
`record_count`); `utils/fields.rs` (the parent's) supplies the epoch, boolean and at-least-one rules.

## PROTOCOL.md first

§13.1–13.7 left these open, now stated:

- Join: `FieldValue` `"address"` carries the address's length; `"partition"` the first partition breaking the rule;
  `CountMismatch` `"partitions"` the partitions sent against 0 (compute) or 1 (storage).
- Members: a request and an empty reply share their bytes, so the view cannot apply "in a request, every section
  empty"; the rule moved to the frontend receiving a request (`CountMismatch` `"members"`). The order rule is spelled
  out: compute before storage, compute by address bytes, storage by partition, each ascending without repeat;
  `"member order"` names the member's index, `IndexOutOfRange` the member.
- Subscribe: `from` at most 2⁴² − 1 (`FieldValue` `"from"`).
- EpochClose: `"announcement"` and `"eviction"` carry the index in the frame; the orders across a close's frames are
  the subscriber's to check, as its epochs are.
- Lease: a body with `start` and `end` both 0 is a request; `"block"` carries `start`.
- Placement: a body with `from` 0 is a request; `epoch`, `from`, `until` each at most 2⁴² − 1 named by their own field;
  a request's `position` and `until` refused as `FieldValue` `"placement"` (the first nonzero), and its sections'
  emptiness checked after they are read (`CountMismatch` `"placement"`, their bytes); `"placement epoch"` carries
  `from` or `until`; `"range"` carries the range's index, and the ranges' count for a last range not ending at 2²⁴ (0
  with no range); `"run"` and `"leader"` the index; `CountMismatch` `"leaders"` (leaders, the ranges' distinct
  partitions), refused too when the counts agree and the partitions differ.
- Catalog: `position` is from 1 in a reply and a body with `position` 0 is a request; a request's `epoch` 0
  (`FieldValue` `"catalog"`); a reply's `version` at least 1 and `epoch` an epoch; a request's bytes empty
  (`CountMismatch` `"catalog"`, their length).

## Phase 1

The surface compiled with every logic body `unimplemented!()` (accessors that return a field as is excepted);
`SURFACE READY: ultragraphdb_protocol_wire (roles 512–518)` posted.

## Phase 2

Goldens laid out in `testdata/golden/roles/generate_roles.py` from the layout tables, independently of the crate:
`join_storage`, `join_compute`, `members`, `members_reply`, `subscribe`, `epoch_close`, `lease`, `lease_reply`,
`placement`, `placement_reply`, `catalog`, `catalog_reply`; their sizes and checksums are rows of
`tests/utils/golden_roles.rs`. Bazel, my modules (`--test_arg=types::<module>` for each): **51 tests, 51 failing**,
every one on `not implemented` (the suite's 51 panics are exactly these).

| Case | Test |
|---|---|
| Each kind's number; 0, 4, 255 naming none | `role_kind_tests` |
| Each record's bytes, every field at distinct bytes | `member_tests`, `catalog_announcement_tests`, `bucket_range_tests`, `namespace_run_tests`, `preferred_leader_tests` |
| Each golden built byte for byte by its builder | `join_tests` … `catalog_tests` |
| Join: unused bytes 1 and 127 (after a bad kind), kinds 0, 1, 4, 255; address not UTF-8, empty, `http://`, `tcp:/`, `TCP://`, `unix:/`; `tcp://`, `unix://` alone taken; partitions ragged, 0, repeated, descending, repeated last; a bad address before a ragged section; storage with none, compute with one; `u32::MAX` alone | `join_view_tests` |
| Members: header bytes 0 and 127; ends ragged before members ragged; kinds 0, 1, 4, 255 and a bad kind before padding; padding bytes 1–3 at their body offsets; compute with a partition, storage with 0; address equal to the count; compute after storage, compute addresses repeated and descending, storage partitions repeated and descending; storage alone, compute alone | `members_view_tests` |
| Subscribe: 0 and 2⁴² − 1 taken; 2⁴² and `u64::MAX` refused; unused byte 8; a section | `subscribe_view_tests` |
| EpochClose: unused 17, 127; epoch 0, 2⁴² before `more`; `more` 2, 255; any time; an empty close at the last epoch; both lengths before any record; version 0; an equal, a lower version, a lower tenant; a higher tenant with a lower version taken; an eviction of zeros, equal, lower at its last byte, higher at its last byte taken | `epoch_close_view_tests` |
| Lease: unused 2, 3, 32, 127; type, partition, size 0 in order; blocks (0, s), (1, s), (1, s + 2), (s, 0), (s, s), past `u64::MAX`; the last block a u64 holds; every field's extreme | `lease_view_tests` |
| Placement: unused 32, 127; epoch, from, until past 2⁴²; a request's position, until, sections, its position before its sections, epoch 0; a reply's from above epoch, until equal to from and to epoch, epoch 0's until; four valid epoch triples; each section ragged; range padding bytes 12–15, a first start past 0, a gap, an overlap, an empty range, partition 0, a neighbour's partition, a last end short and past 2²⁴, no range, one range of everything; ranges before runs, runs before leaders; run padding 9–15, a tenant equal and lower; replica 0, leader partitions equal and lower, too few, a wrong one, too many | `placement_view_tests` |
| Catalog: unused 32, 127; a request's epoch and bytes; a request naming a version and tenant `u64::MAX`; a reply's version 0 and epoch past 2⁴²; a pending reply with no bytes | `catalog_view_tests` |

## Phase 3

Inputs reviewed for coincidences: every golden field differs from its neighbours (Lease type 2, partition 5, size
4,096, start 81,921; Placement position 1,234, epoch 42, from 40, until 45, leaders' replicas 3 and 4 apart from
partitions 1 and 2, runs 0x40 and 0x81; Catalog version 12, epoch 44, position 1,240, 37 bytes, a length no multiple of
8). Members' compute addresses are string indices 0 and 1, equal to their member indices, so a view returning the
member's index for its address would pass on them; the storage members' addresses 2, 2 and 3 tell them apart
(`member(3)`, `address(3)`). The Members and Placement requests share their body, three empty sections, under
different commands. No change was needed.

## Phase 4

All 51 green under Bazel; `cargo clippy -p ultragraphdb_protocol_wire --all-targets -- -Wclippy::all` reports nothing in
these files.

## Phase 5: mutation testing

Per-file runs (`scripts/mutants.sh ultragraphdb_protocol_wire <file> --diff cabae5ec`, run 1 of each) once the crate's
suite was green, before `scripts/mutants.sh` became one run per crate:

| File | Mutants | Caught | Unviable | Missed / timeout |
|---|---|---|---|---|
| `types/role_kind.rs` | 7 | 6 | 1 | — |
| `types/member.rs` | 16 | 14 | 2 | — |
| `types/catalog_announcement.rs` | 7 | 6 | 1 | — |
| `types/bucket_range.rs` | 14 | 13 | 1 | — |
| `types/namespace_run.rs` | 12 | 11 | 1 | — |
| `types/preferred_leader.rs` | 7 | 6 | 1 | — |
| `utils/whole_records.rs` | 11 | 11 | 0 | — |
| `types/join/join_body_builder.rs` | 5 | 5 | 0 | — |
| `types/join_view/join_view_body_view.rs` | 12 | 10 | 2 | — |
| `types/join_view/mod.rs` | 8 | 7 | 1 | — |
| `types/members/members_body_builder.rs` | 4 | 4 | 0 | — |
| `types/members_view/members_view_body_view.rs` | 16 | 15 | 1 | — |
| `types/members_view/mod.rs` | 10 | 8 | 2 | — |

No survivor. The run of `types/subscribe/subscribe_body_builder.rs` was interrupted (its ledger counts 1, no result),
and the script then changed to one run per crate and base, which this crate shares with the parent and the sibling
parts; I did not start one.

### Open, for the crate-wide run and task 12.1

Not yet mutated: `types/subscribe/subscribe_body_builder.rs`, `types/subscribe_view/subscribe_view_body_view.rs`,
`types/epoch_close/epoch_close_body_builder.rs`, `types/epoch_close_view/{epoch_close_view_body_view.rs, mod.rs}`,
`types/lease/{mod.rs, lease_body_builder.rs}`, `types/lease_view/{lease_view_body_view.rs, mod.rs}`,
`types/placement/placement_body_builder.rs`, `types/placement_view/{placement_view_body_view.rs, mod.rs}`,
`types/catalog/catalog_body_builder.rs`, `types/catalog_view/{catalog_view_body_view.rs, mod.rs}`. The crate-wide
`--diff cabae5ec` run covers them.

## Choices the spec left open

- A record's crate-private `decode` checks its own zero bytes and kinds (`Member`, `BucketRange`, `NamespaceRun`); the
  view checks what needs the body (an address against the string count, order between records).
- `MembersView::member`, `PlacementView::range`/`run` re-decode a record already checked, with command 0; the error
  cannot arise after `from_frame`.
- `Lease::with_block` saturates `start + size`, so a builder never panics; the view refuses the saturated block.
- Placement keeps the ranges' distinct partitions in a `BTreeSet` to compare with the leaders; allocation is bounded by
  the section, which the frame bounds.
- `RoleKind` names the frontend (1) though no body of 512–518 carries it; Join and Members refuse it as a kind.
