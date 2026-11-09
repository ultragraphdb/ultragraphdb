# Group 8: `ultragraphdb_protocol_wire`, role commands 519–524

Challenge, Verify, Bind, Recall, ComputeRead and Scan (`PROTOCOL.md` §13.8–13.13), with their builders, views,
records and golden frames; a fork of the wire agent beside the forks of 512–518 and 525–530.

## Phase 1: the surface

Compiled with every body `unimplemented!()`; `SURFACE READY` posted in `wave-5-coordination.md`:

- `Challenge::request(name)`, `.with_salt(iterations: u32, salt: Vec<u8>)`; `name()`, `iterations()`, `salt()`;
  `ChallengeView` alike, borrowing.
- `Verify::request(name, auth_message, proof: [u8; 32])`, `.with_verified(principal: u64, signature: [u8; 32])`;
  `name()`, `auth_message()`, `proof()`, `verified()`, `principal()`, `signature() -> Option<&[u8; 32]>`; `VerifyView`.
- `Bind::new(partition: u32, kind: BindKind, tenant: u64, epoch: u64, principal: u64, transaction: [u8; 16])`,
  `.with_read_version(u64)`; getters; `BindView`. `BindKind { Transaction, Drop }` with `number()`, `from_number`.
- `Recall::new(partition: u32, session: [u8; 16], request: u64)`; getters; `RecallView`.
- `ComputeRead::new(tenant, read_only: bool, level: ReadLevel, terms: TermTable, pattern: QuadPattern, resume:
  Vec<u8>)` with `with_partition`, `with_transaction`, `with_read_version` (clears `named`), `with_named_version`
  (sets it), `with_epoch`, `with_principal`, `with_limit`, `with_deadline_ms`; getters; `ComputeReadView` with
  `terms() -> &TermTableView`.
- `Scan::new(at: ScanAt, read_version, epoch, principal, transaction: [u8; 16], limit: u32)`, `push_range(ScanRange)`,
  `ranges()`; `ScanView` with `range_count()`, `range(i) -> Option<ScanRange<'a>>`, `ranges()`. `ScanAt { ReadVersion,
  Latest, LatestClosed, Named }` with `number()`, `from_number`, `names_version()`. `ScanRange<'a>::new(partition,
  first: &[u8], past: &[u8])`, one borrowed type for builder and view, `encode(at) -> [u8; 24]`. `SCAN_RANGE_SIZE` 24.
- `ReadLevel { Snapshot, Latest, MonotonicFast }` (`src/types/read_level.rs`), created here as the parent specified
  since it did not exist yet; noted in the coordination file.

## Golden frames

`testdata/golden/roles/generate_roles.py`, block `519 to 524`, laid out from §13's tables: `challenge` (request),
`challenge_reply` (RFC 7677's salt, 4,096 iterations), `verify` (RFC 7677's AuthMessage and ClientProof),
`verify_reply` (principal 7, RFC 7677's ServerSignature), `bind` and `bind_reply` (partition 3, tenant 5, token
epoch 30, principal 9, the watermark `(29 << 22) | 500`), `bind_drop` (kind 2, partition 4, tenant 6, principal
11), `recall` (partition 3, request 8), `compute_read` (read-only, level 2, floor epoch 31, limit 100, 50 ms, a resume
key), `compute_read_rw` (read-write, level 1, partition 3, the Bind's version, the transaction, 9,000 ms), `scan` (two
ranges of partitions 3 and 4, the second open-ended), `scan_transaction` (a transaction's one-key range).

## Phase 2: the suite, observed failing

Bazel, `--test_arg=types::<module>::` over the 15 test modules: **37 tests, 36 failed, 1 passed**; every failure the
`not implemented` panic (36 of 36 in the log). The one passing test checks `ComputeRead`'s `with_read_version` /
`with_named_version` setters, plain data the stubs already held.

| Case | Test |
|---|---|
| Each enum's bytes, and 0, 3/5, 255 naming none | `bind_kind_tests`, `scan_at_tests` |
| A range's record: partition, zero bytes, offsets; an open-ended range | `scan_range_tests` |
| Challenge: golden request and reply both ways; unused 4 and 127 before the count; wrong command; iterations 1, 4,095, 10,000,001, `u32::MAX` refused, 10,000,000 taken; a request with a salt, a reply without; the name's UTF-8 before the salt | `challenge_view_tests` |
| Verify: goldens; unused 1, 7, 16, 127 before `verified` 2; a principal without verification; names UTF-8 (sections 0 and 1) before the proof; proof 31 and 33; signature 31 and 0 when verified, 32 when not | `verify_view_tests` |
| Bind: goldens (request, reply, Drop); unused 5, 7, 56, 127; partition 0 before kind; kinds 0, 3, 255 before the epoch; a Drop reading at a version; epoch 2⁴² refused, 2⁴² − 1 taken; a transaction of zeros; a body after no section | `bind_view_tests` |
| Recall: golden; unused 4, 7, 32, 127; partition 0, then a zero session, then request 0; a session of one nonzero byte; request `u64::MAX` | `recall_view_tests` |
| ComputeRead: goldens; unused 7, 64, 127 before mode; mode 2 before level; level 3 before named; named 2; named at level 2 and for a read-write read; named at a read-only snapshot taken; epoch 2⁴² before the transaction, 2⁴² − 1 taken; a read-only read with a transaction byte, a read-write one with partition 0 or no transaction; the term table before the pattern; a parameter slot refused | `compute_read_view_tests` |
| Scan: goldens; unused 1, 7, 52, 127; `at` 0, 5, 255; a version at `at` 2 and 3, and none there taken; `at` 4 with an epoch, and without; epoch 2⁴²; ranges of 47 bytes; no range; padding bytes 4 and 7 before the partition; partition 0; two partitions in a transaction's Scan; a first key off its place, a past key off its place, an empty first key, range 1 off its place; a past key past section 1; keys longer than the ranges; a past key equal to and below its first; a longer past key sharing the first as a prefix | `scan_view_tests` |

## Phase 3: inputs reviewed

Every golden field is distinct from its neighbours: Bind's partition 3, tenant 5, epoch 30, principal 9, a
16-byte transaction of distinct bytes, the watermark a version of epoch 29; ComputeRead's limit 100 and deadline 50,
floor 31, tenant 5, principal 9; Scan's epoch 30, principal 9, limit 1,000 and a version of epoch 30's bound; the
Drop binding names partition 4, tenant 6, principal 11 and the session's bytes as its transaction. The read-write
ComputeRead leaves epoch and limit 0, which the read-only one covers nonzero; `ScanRange::encode`'s test places the
first key at offset 7 with lengths 4 and 3, so no offset equals a length.

## Phase 4

Bazel: the 37 tests pass. `cargo clippy -p ultragraphdb_protocol_wire --all-targets -- -Wclippy::all` and
`cargo fmt --check` report nothing in these files.

## Choices the spec left open

- A request of a "both" command is its body with the reply's fields zero: Challenge's `iterations` 0, Verify's
  `verified` 0 (the request and an unverified reply are one body), Bind's `read_version` 0 (a reply's watermark is
  then any version, and a Drop's is 0).
- A verified reply's principal may be any value, 0 included; §13.9 only requires 0 when not verified.
- `FieldValue` values: the field's value, except `"transaction"` of ComputeRead, which gives the mode (0 for a
  read-write read missing its partition or identity, 1 for a read-only read naming one), `"range"`, which gives the
  range's index, and `"partition"` of Scan, which gives the offending partition.
- Challenge's `CountMismatch` `"salt"`: the salt's length against 0 in a request, 0 against 1 in a reply.
- ComputeRead's mode is `read_only: bool`, as Begin's.

## PROTOCOL.md changes

- §13.8 Challenge: the `"salt"` counts stated, and a request defined as the body with `iterations` 0.
- §13.9 Verify: a request and an unverified reply stated to be one body.
- §13.12 ComputeRead: the values of `FieldValue` `"named"` (the byte) and `"transaction"` (the mode).
- §13.13 Scan: the Order line strengthened. It was silent on a key running past section 1, which a receiver must
  refuse before comparing keys: both keys within section 1 (`IndexOutOfRange`, section 0, the range, the past key's
  end, section 1's length). It now also states `read_version` 0 for `at` 2 and 3 and `epoch` 0 for `at` 4 as checks
  of the Order (the field table said so), the ranges' `SectionLength` before `CountMismatch` `"ranges"`, and the
  values each refusal names.
