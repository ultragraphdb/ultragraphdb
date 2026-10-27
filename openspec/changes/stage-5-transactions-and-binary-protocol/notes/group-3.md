# Group 3: the write-ahead log

## 3.1 Scenarios

`ultragraphdb_transactions_log/features/write-ahead-log/write_ahead_log.feature` holds 13 of the spec's 14 scenarios,
verbatim, run by `tests/bdd.rs` on the simulated storage. "A store ahead of the log refuses to start" is not there: it is
decided by the store's applied version and the version range a round records, which the log does not interpret; it
belongs to task 7.9, with the rest of the replay into the store. Where a scenario names the store ("A round never reaches
the store before the log", "Rounds lost from the store are replayed"), the steps use a model store that applies what the
log hands it at its committed position and nothing else; group 7 binds them again against the real store. "Interior
corruption refuses to start" names the round, the chunk and the chunk's byte range in the body; naming the transactions
whose write-set records overlap that range is the Round body's (group 9), and the step says so.

## 3.2 Phases 1 to 3

### Surface

- `ultragraphdb_error_log`: `LogError(pub LogErrorEnum)`, grouped by what the caller does. Retry or redirect:
  `NotPrimary`, `Busy { segments }`, `Sealed { position }`. Outcome unknown: `Truncated { position }`. Programming
  errors: `PositionMismatch { expected, found }`, `ParentMismatch { position, expected, found }`,
  `TooLarge { size, limit }`. Fatal: `Corrupt { position, segment, offset }`,
  `CorruptChunk { position, segment, offset, chunk, start, end }`, `SuperblockUnreadable { copy }`,
  `Diverged { what, expected, found }`, `Io { operation, kind, message }`, `Poisoned`. One constructor per variant,
  `Display`, `Error`.
- `ultragraphdb_traits_log`: `Position` (gapless from 1, `ZERO` before the first round, `next` checked), `RoundRef<'a>`
  (what `append` takes: position, parent, first version, transaction count, body), `Round` (what `read` returns, with
  its `checksum`), `LogEvent` (`ViewChanged`, `SuffixReplaced`), `OrderedLog`.
- `ultragraphdb_protocol_wire`: `RoundHeader`, the Round command's 128-byte command header: position at 0, parent at 8,
  incarnation at 16, first version at 24, transaction count (u32) at 32, the rest zero. The cluster is the frame
  header's own field. The Round body stays opaque bytes here.
- `ultragraphdb_transactions_log`: the storage seam `DurableStorage` (open, open aligned, create directory, sync
  directory, rename, remove, list) and `DurableFile` (aligned read and write, preallocate, length, truncate, sync), both
  returning futures as Iggy's do; `OpenMode`; `AlignedBuf`; `SimStorage` and `SimFile` with `FaultMode` and
  `StorageOperation`; `CompioStorage` and `CompioFile`; `WalConfig`; `Superblock` and `SuperblockRead`; `encode_round`;
  `scan_segment`, `find_complete_round` and their types `ScanStart`, `ScannedRound`, `SegmentScan`, `Damage`;
  `Truncation`, `Recovery`; `Wal`.

### The async boundary (decision)

`OrderedLog` splits by whether a method waits. `append`, `committed`, `tail`, `seal` and `next_event` are synchronous
and do no I/O: `append` checks a round (position, parent, size, segment, capacity), encodes its frame and stages it, so
every refusal is known before anything is written, which the note's "refuse before writing" asks. `sync`, `read` and
`trim_through` return `impl Future`, as Iggy's journal does: on compio they complete on the runtime's completions; over
the simulated storage every future completes when first polled, so the simulator drives the log with a trivial poll
(`tests/utils/helpers.rs::block_on`), without a runtime. Durability is reported by `sync`, never awaited inside
`append`. The commit step pipelines because the resolver is separate state: it resolves round k + 1 while the future of
round k's `sync` is pending, and appends k + 1 after it completes, which D6 (a round closes when the previous round's
sync completes) asks anyway. D2's signature `append(&mut self, round: Round)` becomes `append(&mut self, RoundRef<'_>)`,
and two methods are added: `sync` (D2 left durability's mechanism to this design) and `tail`, because the next round's
parent is the checksum of a frame only the log builds. Under VSR, `sync` resolves when a quorum has journaled the staged
prepares.

### Decisions taken here, for review

- **Segments**: files `{index:020}.segment`, 16,384 blocks (64 MiB) by default, preallocated by writing zeros, not with
  `fallocate(2)`: a write into an unwritten extent changes metadata an `fdatasync` must flush; zeros written once do
  not. A new segment is created, preallocated, synced and its directory synced before the round's write. Capacity 64
  segments; an append needing a segment past it is `Busy` until a checkpoint trims.
- **First round after a checkpoint**: the superblock holds the checkpoint's position and checksum and the first segment
  kept, as the spec lists, and no more. The first kept segment may start below the checkpoint, so its first round is
  taken as the chain's start when it lies at or below the round after the checkpoint; that round must name the
  checkpoint's checksum, and a round at the checkpoint must have its checksum, else `Diverged`.
- **Truncation**: a torn tail is cut off by leaving the cursor at it. The blocks after the damaged block are zeroed and
  synced (so a shorter round written there later leaves nothing stale); the damaged block itself is left for the next
  append to overwrite. Until then every open finds the same torn tail and reports it again (see phase 4 for why).
- **A tail below the checkpoint** (the round the checkpoint covers was torn while reported synced): the log continues
  from the checkpoint in a fresh segment, the superblock records it as the first kept, and the segments below it are
  removed.
- **Incarnation**: 1 for a log created on an empty superblock. Segments present beside an empty superblock are refused
  (`Diverged`, "segments without a superblock"), never overwritten. No API of this group creates a log afresh over an
  existing superblock, so no incarnation above 1 is produced yet; the scan's rule is tested on segments written with
  `encode_round` under another incarnation, and at the WAL level by a superblock and segments put in place.
- **Read** verifies each round again from the disk: the frame through its chunks, and its checksum equal to the
  indexed one. That checksum covers every header field, so position, parent and incarnation need no separate check
  (the audit's one equivalent mutant below).
- **Direct I/O**: `O_DIRECT` on Linux x86-64 (0o40000) and arm64 (0o200000), written as constants since the crate has
  no `libc`; buffers aligned to 4,096 by `AlignedBuf` (an over-allocated vector at its aligned offset, safe code only).
  On macOS there is no `O_DIRECT`, and `fcntl(F_NOCACHE)` needs an unsafe call: an aligned file is buffered there, and
  compio's `sync_data` is `fsync`, which does not flush the drive's cache. Alignment is checked in software on every
  platform, so a misaligned call fails the same way everywhere.
- **Simulated storage** follows Iggy's model (buffered and stable contents per file and per directory, file and
  directory syncs, power loss) with shared state behind `Rc<RefCell<_>>`, borrowed only inside one call, so a test keeps
  a clone to inject faults while the log owns another. Faults by trace index (`fail_at`) or drawn from the seed
  (`fail_randomly`, SplitMix64 in-crate, no dependency on `ultragraphdb_store_sim`). A torn write keeps whole 512-byte
  sectors, at least one and never all, drawn from the seed: a tear that keeps nothing would be indistinguishable from
  an unwritten block and make a silent tear undetectable, which no checksum can help. `injected()` lists the faults that
  fired, the only record of a silent tear.
- **`DurableFile: Debug`**, added in phase 2: the suite's `expect_err` needs it.

### LogError: each variant and the test that constructs it

| Variant | Test |
|---|---|
| `NotPrimary` | `log_error_tests::test_each_constructor_builds_its_variant_with_its_values` only: unreachable on one node, which is always primary; VSR raises it |
| `Busy` | `wal_ordered_log_tests::test_an_append_past_the_capacity_is_busy_until_a_checkpoint_trims` |
| `Sealed` | `wal_ordered_log_tests::test_a_sealed_log_refuses_appends_and_reports_its_tail`, scenario "A sealed log refuses appends" |
| `Truncated` | constructor test only: unreachable on one node, which never replaces a round it accepted; a round torn by a crash is reported by the recovery after the process that appended it is gone |
| `PositionMismatch` | `test_an_append_out_of_order_is_refused_naming_the_next_position`, `test_read_takes_positions_from_the_first_kept_to_the_one_after_committed`, `test_trim_through_records_the_checkpoint_and_removes_whole_segments` |
| `ParentMismatch` | `test_an_append_with_another_parent_is_refused` |
| `TooLarge` | `test_a_round_too_large_for_a_frame_or_a_segment_is_refused`, `frame_tests::test_the_largest_body_fits_and_one_byte_more_is_refused` |
| `Corrupt` | `wal_tests::test_interior_damage_refuses_the_start_naming_the_round`, `test_a_missing_segment_with_rounds_after_it_refuses_the_start`, `wal_ordered_log_tests::test_read_refuses_a_round_that_no_longer_verifies`, `test_read_refuses_a_valid_frame_that_is_not_the_round_logged_there` |
| `CorruptChunk` | `test_interior_damage_refuses_the_start_naming_the_round`, `test_a_chunk_damaged_in_a_later_segment_is_named_with_its_segment`, scenario "Interior corruption refuses to start" |
| `SuperblockUnreadable` | `wal_tests::test_a_corrupt_superblock_copy_refuses_the_start_and_writes_nothing` (both copies), scenario "A corrupt copy is not a fresh deployment" |
| `Diverged` | `scan_tests::test_rounds_contradicting_the_checkpoint_diverge`, `test_a_fresh_logs_first_round_with_another_parent_diverges`, `wal_tests::test_a_superblock_of_another_cluster_diverges`, `test_segments_without_a_superblock_are_refused_not_overwritten`, `test_the_checkpoint_must_match_the_rounds` |
| `Io` | `test_a_failed_write_or_sync_poisons_the_log`, `test_io_errors_name_the_operation`, `superblock_io_tests::test_a_failed_read_is_an_io_error`, `test_a_failed_write_is_an_io_error`, `wal_tests::test_a_failed_directory_creation_is_an_io_error` |
| `Poisoned` | `test_a_failed_write_or_sync_poisons_the_log` (each of a first round's six storage steps), `test_a_failed_trim_poisons_the_log` (each of a checkpoint's seven steps) |

### Independent sources

- Frame layout: header fields and the Round command header asserted byte by byte against the tables
  (`frame_tests`), the checksum recomputed with `xxhash_rust` directly over header bytes 8 to 256
  (`helpers::frame_checksum`); frames validated by `ultragraphdb_protocol_wire::FrameView`, which group 2 pinned to the
  xxHash reference implementation's outputs.
- Superblock layout: records written by hand in the test from the table, checksummed with `xxhash_rust`.
- `RoundHeader`: command headers laid out by hand.
- Offsets, sizes, chunk ranges and positions in the scan and WAL tests: worked by hand from the frame sizes (a
  1,000-byte body is a 1,256-byte frame, one block; 5,000 bytes take two chunks and a 16-byte table, 5,272 bytes, two
  blocks; 3,840 and 7,920 fill one and two blocks exactly; 1,046,272 bytes is the largest body of a 1 MiB frame,
  computed separately in Python).
- The durability property: a seeded workload of appends, syncs, checkpoints, restarts and power losses under random
  faults of all four modes, with every acknowledged round's checksum kept by the test and checked after each recovery
  (`wal_tests::test_no_acknowledged_round_is_lost_under_faults_and_power_losses`, 512 seeds in the suite; 20,000 seeds
  run once in release, all passing).
- The storage contract is one set of tests over `DurableStorage` and `DurableFile`, run on both implementations
  (`tests/traits/`), so the simulator is held to compio's observable behaviour.

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| Empty segment; zeros; bytes short of a block; a trailing partial block | `scan_tests::test_a_segment_of_zeros_holds_no_round`, `test_bytes_short_of_a_block_are_not_read` |
| Rounds filling a segment exactly; a round that does not fit the rest of one | `test_rounds_filling_the_segment_end_at_its_length`, `wal_ordered_log_tests::test_a_round_never_spans_two_segments` |
| A frame of exactly one and two blocks; an empty body; the largest body and one byte more; a round larger than a segment, and one exactly a segment | `frame_tests`, `test_a_round_too_large_for_a_frame_or_a_segment_is_refused` |
| `size` 0, 255, 16,385 (past the segment), 1 MiB + 1, `u32::MAX`; a valid frame running past the segment | `test_a_size_out_of_bounds_is_header_damage_before_its_checksum_is_trusted` |
| A flipped byte in each header field; the table; chunk 0 and 1 at their first and last byte; a one-chunk body's last byte | `scan_tests` |
| Another cluster; another command; another incarnation after and before the current rounds | `scan_tests` |
| Chain: wrong parent, a skipped position, a repeated position, the first round's parent | `test_a_broken_chain_is_chain_damage_naming_the_round`, `test_a_fresh_logs_first_round_with_another_parent_diverges` |
| Checkpoint: first round below it, at the round after it, past it; the checkpoint round's checksum; position `u64::MAX` | `test_the_first_kept_segment_may_start_below_the_checkpoint`, `test_rounds_contradicting_the_checkpoint_diverge`, `test_the_round_at_the_last_position_ends_the_scan` |
| Append at the tail, past it, at 0, at `u64::MAX` | `test_an_append_out_of_order_is_refused_naming_the_next_position` |
| Read from 0, 1, the first kept, committed + 1, committed + 2; `max_bytes` 0, 1, one frame − 1, exactly two frames | `test_read_*` |
| Trim past committed, at, below, at 0, at the checkpoint; the segment the next round goes to | `test_trim_through_*` |
| A fault at every step of a round's sync and of a checkpoint; each mode on a write, a sync, every other operation | `test_a_failed_write_or_sync_poisons_the_log`, `test_a_failed_trim_poisons_the_log`, `sim_storage_tests` |
| A torn tail with damage in its header, its chunks, at a segment's start, at its last block, below the checkpoint (alone in a segment, after rounds below it) | `wal_tests` |
| Two silent tears in a row | the property test (seed 5,892 in the 20,000-seed run) |
| A missing middle segment; a short tail segment; junk where a round did not fit; other names in the directory | `wal_tests` |
| Superblock: absent, empty file, one copy, two, each corrupt, a wrong length, every bit flip, version and reserved bytes resealed, sequence 0 and `u64::MAX` | `superblock_tests`, `superblock_io_tests` |
| A tear of 1 byte, of under a sector, of many sectors, over old bytes | `sim_storage_tests` |
| Coincidence: test bodies equal across rounds | every round's body depends on its position; incarnation-1 and incarnation-2 bodies differ |

### Phase 2: the failing run

`cargo test --no-fail-fast` against the surface:

- `ultragraphdb_error_log` unit: 7 tests, 7 failed.
- `ultragraphdb_traits_log` unit: 9 tests, 9 failed.
- `ultragraphdb_protocol_wire`: the 4 new `round_header_tests`, 4 failed (the 62 existing tests and 9 scenarios as before).
- `ultragraphdb_transactions_log` unit: 132 tests, 131 failed, 1 passed; `bdd`: 13 scenarios, 13 failed.
- Every failure is an `unimplemented!()` panic in `src/` (counted by file: 73 in `sim_storage`, 35 in `Position`, the
  rest across the surface); no compile error and no panic elsewhere. The one that passed,
  `aligned_buf_tests::test_alignment_is_four_kibibytes`, pins a constant of the declared surface.

### Phase 3: the audit

A throwaway implementation, written in a scratch workspace with path dependencies on the repository, then discarded:
its sources were moved aside and the repository's phase-4 code was written separately afterwards.

**The throwaway found defects in the suite** before any deliberate defect was introduced, each corrected as a wrong
expectation, not a weakened one:

- The BDD steps ran the simulated storage's futures with `futures::executor::block_on` inside cucumber's own executor
  (`EnterError`). `helpers::block_on` now polls with a no-op waker; the simulator's futures complete on the first poll.
- Two simulator tests expected stable contents of a file whose directory entry was never synced; the model was right
  (the entry would not survive a power loss). The tests sync the directory.
- The body helper overflowed at position `u64::MAX`.
- Torn-write tests used 1,000-byte bodies: a tear in the frame's zero padding loses nothing, and the scan rightly reads
  the round as whole. Those tests use bodies that fill their blocks (3,840 and 7,920 bytes).
- The tracing-event test was flaky under parallel tests: tracing caches a callsite's interest process-wide. It now keeps
  a second dispatcher alive and rebuilds the cache, as `ultragraphdb_store_sim`'s `LogCapture` does.

**The property test found three defects in the design** (all in phase 4's code):

1. A segment created before a failed directory sync was reopened from the buffered view and reused; nothing synced its
   entry, so a power loss dropped acknowledged rounds. The open now syncs the log's directory unconditionally.
2. A round a crashed process left in the page cache was read back as committed but never synced; a later round, synced
   in another segment, survived a power loss and the earlier one did not, leaving a hole the next start refused. The
   recovery now syncs every segment it recovered rounds from before calling them committed.
3. A failure while zeroing a truncated tail could destroy the damage and lose its report (found by the 4,000-seed run in
   phase 4, below).

**Deliberate defects**: 84, scripted (the script and its list stayed in the session's scratch space, not committed),
introduced one at a time, each run against the full suite of the crate it touches: guard removed 44, plausible
neighbour 15, off-by-one 8, flipped comparison 7, constant changed 6, early return 2, order 2. 78 were rejected on the
first run. Of the rest:

- One snippet named the wrong file (tears keep everything); rerun, rejected (12 tests).
- "Read does not verify" was a defect written wrongly (it still validated); replaced by "read accepts a body that fails
  its chunks" and "read names a chunk failure as the round's", both rejected.
- Three were gaps, each closed by a new test and rejected on the rerun: a failed trim that does not poison
  (`test_a_failed_trim_poisons_the_log`); normalisation below the checkpoint when the segment also holds rounds below it
  (`test_a_torn_round_at_the_checkpoint_after_rounds_below_it_leaves_the_log_at_the_checkpoint`); the tail segment's
  re-preallocation at open (`test_a_tail_segment_cut_short_is_preallocated_again` now checks the length at the open).
- Two are equivalent: the size lower bound changed from 256 to 255 (a 255-byte frame fails `HeaderView::new`'s length
  rule, the same `Header` damage), and "read trusts the index for the parent" (the checksum comparison covers the
  parent). Phase 4 drops the redundant checks, and `test_read_refuses_a_valid_frame_that_is_not_the_round_logged_there`
  pins the checksum comparison.

Input variety: the round bodies, positions, versions and counts differ in every test; the two-chunk and one-chunk cases
are both present wherever chunk ranges are asserted.

## 3.3 to 3.7 Phase 4

Written fresh in the repository after the audit. The suite passed on the first run (135 unit tests and 13 scenarios at
that point).

A wider run of the property test (4,000 seeds, release) then failed at seed 674: a fault that took effect and then failed
on the write zeroing the damaged block destroyed the evidence; the open failed, and the next open found zeros and no
torn tail, so the acknowledged round was lost without a report. Zeroing the damaged block last is not enough, since the
last write can be the one that fails. **Decision**: the recovery never zeroes the damaged block; it zeroes the blocks
after it, and the next append overwrites the damaged block. One assertion of the torn-tail test changed with it (it had
required the damaged block zeroed); it now requires the same truncation reported again on a second open, the next
round written over the block, and nothing of the torn round left behind it
(`test_a_torn_tail_is_truncated_reported_until_overwritten`).

The 20,000-seed run then failed at seed 5,892: two consecutive silent tears, both acknowledged, both lost, the report
naming the first. That is within the spec (each was a write torn while reported successful) and was my test's
assumption of one tear; the test now asserts that every lost round is at or past the named one and that there are no
more lost rounds than silent tears, and `Truncation`'s documentation says every round from its position on is cut off.
With that, 20,000 seeds pass.

Coverage (`cargo llvm-cov`): 100 % of lines in every added file except these, each unreachable:

- `SimStorage::with_io` and `SimFile::perform`'s busy error: the shared state is borrowed only inside one call, never
  across one.
- `SimState::entries_mut` on a non-directory: called only after `entries` or `parent` checked the inode.
- `Wal::open`'s directory without a parent: the log opened at the filesystem root.
- `Wal::read_entry` with no open file, `checkpoint_at` with no indexed round: every indexed round's segment is open, and
  every committed round above the checkpoint is indexed.
- `encode_round`'s refusal other than `FrameTooLarge`: `Header::encode_frame` refuses for size alone.

`O_DIRECT` is compiled for Linux (`cargo check --target x86_64-unknown-linux-gnu` passes) but never executed here: the
machine is macOS, where the compio tests run buffered.

Final counts, Cargo and Bazel equal:

| Target | Cargo | Bazel |
|---|---|---|
| `ultragraphdb_error_log` unit | 7 passed | 7 passed |
| `ultragraphdb_traits_log` unit | 9 passed | 9 passed |
| `ultragraphdb_protocol_wire` unit, `bdd` | 66 passed, 9 scenarios | 66 passed, 9 scenarios |
| `ultragraphdb_transactions_log` unit, `bdd` | 141 passed, 13 scenarios (26 steps) | 141 passed, 13 scenarios |

Doc tests: none written; each crate's Bazel `doc_test` passes. `cargo fmt --all -- --check` and
`cargo clippy --all-targets` of the four crates are clean.

## 3.8 Phase 5

Deferred: by the user's decision of 2026-10-27, mutation testing and fuzzing run once, at the stage's final verification
(group 11), not per group. `scripts/mutants.sh` and `scripts/fuzz.sh` were not run for this group.

Prepared for that run:

- `ultragraphdb_transactions_log/fuzz/`, its own `[workspace]`, no `BUILD.bazel`, `arbitrary` `=1.5.0` and
  `libfuzzer-sys` `=0.4.13`. Targets: `scan` (any bytes as a segment, scanned as a first segment kept at checkpoint 0,
  at a checkpoint the first byte chooses, and as a continuing segment; never panics; every accepted round lies at a
  block boundary end to end, validates on its own through its chunks, is of this incarnation, chained and gapless, and
  is found complete by `find_complete_round`) and `superblock` (any bytes decoded or refused; an accepted record
  re-encodes to the same bytes).
- Seeds in `testdata/fuzz_seeds/scan/` (three rounds then zeros; mixed one- and two-block rounds; a torn tail; rounds over
  an older incarnation's; a segment after a checkpoint at 3) and `testdata/fuzz_seeds/superblock/` (a fresh log's, one
  after a checkpoint, the largest values), generated with the crate's own `encode_round` and `Superblock::encode`.
- `cargo +nightly fuzz build --fuzz-dir ultragraphdb_transactions/ultragraphdb_transactions_log/fuzz` passes, and each
  target executed its seeds once (`-runs=0`: 6 and 4 inputs) without a failure. That is a check of the seeds, not a
  fuzzing run.
- No regression case exists, so no replay test was added, and the unit target declares no `data`: a test that reads
  `testdata/` will declare it then.

## 3.9 Segment allocation

Two halves. The first reserves a fresh segment's blocks with `fallocate(2)` and `FALLOC_FL_KEEP_SIZE` in place of
writing zeros. The second recycles trimmed segments: it was held back because it needs a guard the design does not
give ("Recycling needs a guard", below), and the user decided it on 2026-10-27, with the file system without
`fallocate` and a second tear in the simulator ("The second half", below).

### The first half

#### Decisions

- **The reservation**: `DurableFile::preallocate(len)` now reserves the blocks of the file's first `len` bytes and
  leaves its length and bytes unchanged; on an aligned file `len` is whole blocks, the file's length unchecked. This
  supersedes 3.2's "preallocated by writing zeros". `CompioFile` calls `rustix::fs::fallocate` with `KEEP_SIZE` on
  Linux (`cfg(target_os = "linux")`) on compio's blocking pool, on a duplicate of the descriptor, as compio runs its
  own blocking file calls; on macOS it reserves nothing. `SimFile` reserves nothing the model can observe. A fresh
  segment is still created, reserved, synced and its directory synced before its first round, and every sync stays
  where it was.
- **What a short segment changes**: a segment is as long as its rounds, so the scan reads only what was written (the
  scan's contract fix for a final round whose padding lies past the bytes, since committed as 385d0c3, is kept). Two
  places relied on full-length segments:
  `clear_after_damage` zeroed up to the segment size, which would have written zeros over the reserved rest of a short
  segment; it now zeroes only up to the segment's length, since past it a segment holds no bytes. `sync_recovered`
  keeps reserving the cursor's segment again at the open when it is short, which is now nearly always: `fallocate` is
  idempotent, and a crash between a segment's creation and its reservation leaves one unreserved.
- **The simulator's tear**: a torn write used to grow the file only to the bytes kept, so a tear could leave a length
  short of a block. With full-length segments that never mattered; in a short segment the scan, which reads whole
  blocks, dropped the torn block and lost a silently torn, acknowledged round without a report (the audit's D1 below:
  12 unit tests and 2 scenarios fail, the property test among them). A real file system records an `O_DIRECT` write's
  whole end or none of it, and the drive keeps a prefix of sectors, so a torn write now grows the file to the write's
  end with zeros where the drive kept nothing. Five simulator tests changed their expectation with it, each to that
  rule.
- **`rustix`**: `=1.1.6`, no default features, `fs`, a Linux-only dependency of the crate. The release compio already
  resolves, so the lock adds no package. Pure Rust: on Linux its `linux_raw` backend makes raw system calls with
  `linux-raw-sys`'s generated constants (`build = false`, no C), and its build script only probes `rustc`; no `cc` in
  the crate's tree on either platform (`cargo tree -e normal,build`), and on macOS the crate does not depend on it.
- **A file system without `fallocate`** (`EOPNOTSUPP`) failed the segment's creation as `Io`; the second half replaces
  that with a fallback.

#### Phase 2: the failing run

No new public surface; the surface's contract changed (`preallocate`, the simulator's tear). The suite was changed and
extended first and run against the code that wrote zeros: 147 unit tests, **15 failed**, 132 passed; 13 scenarios
passed. Every failure an assertion of the new contract, none a compile error or a panic elsewhere:

- `durable_file_tests`: `test_preallocate_and_truncate_on_{the_simulated_storage,compio}` (the length is left),
  `test_aligned_files_on_{the_simulated_storage,compio}` (a short file reserves whole blocks; the length is the write's
  end), `test_preallocate_reserves_nothing_on_macos` (`st_blocks` 0). `test_preallocate_reserves_the_blocks_on_linux`
  (`st_blocks` at least 2,048 for 1 MiB, length 0) is compiled for Linux only.
- `sim_storage_tests`: the five tear tests, and `test_a_torn_write_past_the_end_keeps_the_old_bytes_and_zeros_after_them`.
- `wal_ordered_log_tests`: `test_rounds_lie_at_block_boundaries_padded_with_zeros` (8,192 bytes, not 16,384),
  `test_the_log_on_compio_survives_a_reopen` (segment 0 is 12,288 bytes).
- `wal_tests`: `test_a_fresh_segment_ends_at_its_last_round`, `test_a_tail_segment_cut_short_is_preallocated_again`
  (reserved and synced at the open, the length left), `test_a_torn_tail_in_a_short_segment_is_cleared_within_its_length`.

Two tests and one scenario that relied on bytes after the rounds (zeros, or junk) now put them there (`test_preallocated_zeros_are_not_read_as_rounds`,
`test_junk_where_a_round_did_not_fit_is_passed_over`, the step of "Preallocated zeros are not read as rounds"), so
each still checks what its name says.

#### Phase 3: the audit

16 deliberate defects, one at a time, against the full suite in an isolated copy of the tree (scratch space, not
committed), each rejected by a test whose subject it is:

| Defect | Rejected by |
|---|---|
| D1 a tear grows the file only to the bytes kept | 5 tear tests, 6 WAL torn-tail tests and the property test; 2 scenarios |
| D2 the simulator's reservation grows the file with zeros | 6: the two contract tests on the simulator, the two WAL length tests, the cut-short and clearing tests |
| D3 clearing unbounded by the length; D4 bound one block short; D13 bound flipped (`max`) | `test_a_torn_tail_in_a_short_segment_is_cleared_within_its_length` |
| D17 clearing starts at the damaged block | the clearing test, `test_a_torn_tail_is_truncated_reported_until_overwritten`, the property test |
| D5 no reservation again at the open; D6 no sync after it | `test_a_tail_segment_cut_short_is_preallocated_again` |
| D7 the simulator's alignment check dropped; D8 it checks the file's length | `test_aligned_files_on_the_simulated_storage` |
| D9 compio's alignment check dropped | `test_aligned_files_on_compio` |
| D11 a tear keeps everything; D12 the zeros after the kept prefix replaced by the write | 14 each, the superblock's fault test among them |
| D14 a fresh segment not reserved | the trace test, the poisoning test, two torn-tail tests counting steps; 1 scenario |

Not audited, because they run on Linux only: `fallocate` called without `KEEP_SIZE`, or with offset and length
swapped. `test_preallocate_reserves_the_blocks_on_linux` and the compio contract tests reject both on Linux; task 11.9
runs them.

#### Phase 4

The implementation passed the audited suite on its first run. The property test passed at 20,000 seeds in release once
(4.6 s), as at 3.7. Coverage (`cargo llvm-cov`): every changed line covered on macOS except the Linux block of
`CompioFile::preallocate`, which is not compiled there; its `JoinError` arm is reached only if the blocking closure
panics, which `fallocate`'s `Result` does not.

**Exercised on Linux: nothing.** `cargo check` and `cargo clippy` of the crate and its tests pass for
`x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. The `fallocate` path, the
Linux reservation test and the compio suite against a real Linux file system are task 11.9's; the extra sync cost of
a fresh segment's extent conversion and length update is task 12.4's.

Final counts, Cargo and Bazel equal: `ultragraphdb_transactions_log` unit 147 passed, `bdd` 13 scenarios (26 steps).
`cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_transactions_log` passes.

### The second half (decided 2026-10-27)

#### Decisions

- **Recycling, option (a), the user's decision of 2026-10-27**: a checkpoint zeroes each segment it trims whole, to the
  segment size, through the zeroing loop the log already had (`utils::zeros::write_zeros`, 1 MiB aligned pieces, now
  shared with the torn-tail clearing and the fallback below), syncs it, and renames it a spare, `{index:020}.spare`,
  named by the segment it was; the directory is synced once after the last. All of it runs in `trim_through`, the
  checkpoint path, never in a round's `sync`. A new segment takes the lowest spare: renamed to the segment's name, the
  directory synced, opened, then the round's one write and one sync, so the steady state creates and reserves nothing
  and the round writes into blocks already written, inside the file's length. With no spare, a segment is created and
  reserved as in the first half. Spares do not count toward the capacity, and their number is bounded by it: each was
  a segment. Nothing caps them below that (a log that once grew to its capacity keeps that much disk), which the design
  does not ask for.
- **The sequence and why its order**: superblock (temporary, sync, rename, directory sync), then per segment zeros,
  file sync, rename, then one directory sync. The superblock goes first: zeroing a segment it still keeps would destroy
  rounds above its checkpoint (the audit's R5 loses acknowledged rounds in the property test). The zeros are durable
  before the spare's name, so a spare never holds an old round (R3, R4). A crash at any point leaves either the old
  superblock and every segment intact, or the new superblock with some segments below its first kept, under their own
  name or as spares; the open recycles those below the first kept the same way (zeros, sync, rename), where it removed
  them before, and finds the spares by name. `restart_at_checkpoint` recycles too.
- **What the scan sees**: a recycled segment is zeros at full length, so the block after the tail is a clean end and the
  scan's rules stand unchanged; the two stale cases found in the first half cannot arise.
- **No `fallocate` (`EOPNOTSUPP`), the user's decision of 2026-10-27, scoped to that one error**: `CompioFile` maps
  `EOPNOTSUPP` to `io::ErrorKind::Unsupported`; the log's `reserve` then writes zeros from the segment's written end to
  its size through the same loop, which reserves the space as `fallocate` does, so a full disk shows at the segment's
  creation, and logs one warning per log, naming the error, the first time. Every other error still fails the
  creation as `Io`. The simulator refuses preallocation as unsupported after `SimStorage::refuse_preallocation`.
- **The second tear**: `FaultMode::ShortTornWrite`, a write torn to a prefix whose file grows only as far as that
  prefix, reported as an error: a crash during an unsynced write extending the file. The first half's tear stays as
  `TornWrite` and `SilentTornWrite` (the whole length recorded). A random fault that draws a reported tear takes each
  at equal odds; a silent tear always keeps the whole length, because a write the drive tore while reporting success
  and whose length was lost too leaves no bytes for any checksum to find, which the spec's requirement that such a
  round is detected cannot cover.
- **What the short tear exercises, and what it does not**: a short tear leaves a length inside a block. The log reads a
  segment in whole blocks, so the partial block is never scanned, and the round is absent, as it should be, being
  unacknowledged. That reaches two places that had to change: the torn-tail clearing now zeroes up to the length
  rounded up to a block (an unaligned end would fail the aligned write), and the open's re-reservation starts its
  fallback zeros at the length rounded down. It does **not** reach the scan's "a round whose padding lies past the
  bytes" contract: whole-block bytes always hold a fitting frame's padding, so that contract is reached only by direct
  callers of `scan_segment` and the fuzz target, as before. The property test asserts both mechanisms run: over its
  512 seeds, 141 short tears injected into writes and 1,665 spares taken as new segments.

#### Phase 1 and 2

Surface: `FaultMode::ShortTornWrite`, `SimStorage::refuse_preallocation` (`unimplemented!()`), `DurableFile::preallocate`
documented to refuse as `Unsupported`. The suite, written first: 156 unit tests, **12 failed**, 144 passed; 13 scenarios
passed. Each failure an assertion of the new behaviour or the `unimplemented!()` panic of `refuse_preallocation`:

- `sim_storage_tests`: `test_a_short_torn_write_grows_the_file_only_as_far_as_its_prefix`,
  `test_a_random_tear_reported_with_an_error_is_whole_or_short`, `test_a_refused_preallocation_is_unsupported`,
  `test_torn_modes_on_other_operations` (a short tear on another operation).
- `wal_ordered_log_tests`: `test_a_checkpoint_zeroes_its_trimmed_segments_and_renames_them_spares` (the trace and the
  spares' zeros), `test_a_new_segment_takes_a_spare_and_allocates_nothing` (rename, directory sync, open, write, sync;
  no create, no reservation), `test_a_failed_trim_poisons_the_log` (now nine steps).
- `wal_tests`: `test_a_crash_after_any_step_of_a_recycling_checkpoint_recovers_without_a_report` (each of the twelve
  steps faulted after its effect, with and without a power loss: no report, every acknowledged round above the
  checkpoint read back, three later rounds surviving the next restart without a report, segment 1's spare zeroed),
  `test_a_crash_after_any_step_of_taking_a_spare_loses_no_acknowledged_round` (each of the five steps, both ways),
  `test_a_clean_restart_with_the_tail_in_a_recycled_segment_reports_no_truncation` (the spare's old life held two-block
  rounds, so its second block was the middle of one, the first half's case 2; the open finds the other spare and the
  next segment takes it), `test_without_fallocate_a_new_segment_is_filled_with_zeros_and_warned_once`, and the
  property test (its tallies).

#### Phase 3: the audit

18 deliberate defects, one at a time, in the isolated copy:

| Defect | Rejected by |
|---|---|
| R1 recycling does not zero; R2 it zeroes one block | the checkpoint trace test, the spare test, the clean-restart test, both crash tests, and a torn-checkpoint test |
| R3 no file sync before the rename | the trace test, the poisoning test, both crash tests |
| R4 renamed before zeroed | the trace test, the checkpoint crash test |
| R5 recycled before the superblock is written | the trace test, the checkpoint crash test, the property test |
| R6 spares never taken | the spare test, the clean-restart test, the property test's tally |
| R7 no directory sync after taking a spare | the spare test, the clean-restart test, the spare crash test, the property test |
| R8 spares not found at the open | the clean-restart test, the checkpoint crash test |
| R9 the open leaves segments below the first kept; R10 it removes them | the checkpoint crash test (R9 also `test_open_finishes_an_interrupted_trim_and_sweeps_temporaries`) |
| R11 the fallback writes nothing; R12 it refuses; R13 it warns every time | the fallback test |
| R14 a short tear records the whole length | the short-tear test |
| R15 the draw never tears short | the random-tear test, the property test's tally |
| R16 clearing to an unpadded length; R17 the open's fallback from an unrounded length; R18 the restart at the checkpoint does not recycle | **none at first**: three gaps |

The gaps were closed by `test_a_torn_tail_before_a_short_torn_write_is_cleared_to_a_whole_block`,
`test_without_fallocate_a_segment_whose_zeros_were_torn_short_is_filled_at_the_open` (both pick, among seeds 0 to 63,
the first whose tear ends inside a block) and `test_a_torn_checkpoint_round_after_rounds_below_it_recycles_their_segment_at_the_open`;
each rejects its defect on the rerun. Not audited, Linux only: the `EOPNOTSUPP` mapping in `CompioFile`.

#### Phase 4

The implementation passed the suite after one change made against a test, not to it: the fallback first read the
file's length, an extra traced step the fallback test did not expect; `reserve` now takes the written end from its
caller (0 for a new segment), which the open already knows. The property test passed at 20,000 seeds in release
(5.6 s). Coverage: every changed line covered on macOS except the Linux block of `CompioFile::preallocate`.

Exercised on Linux: nothing, as in the first half; `cargo clippy --all-targets` passes for `x86_64-unknown-linux-gnu`
and `aarch64-unknown-linux-musl`. Final counts, Cargo and Bazel equal: unit 159 passed, `bdd` 13 scenarios.
`cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_transactions_log` passes.

### Recycling needs a guard the design does not give (found in the first half)

The design relies on self-identifying rounds: "a scan stops at the first block that is not the next round of this
incarnation, so a preallocated segment's zeros or a reused segment's old rounds are never read as rounds." The chain
does keep them from being read as rounds: a stale round of this incarnation lies below the checkpoint, so its position
is never the one expected, and `find_complete_round` looks only at the expected position and later, so stale rounds
never refuse a start either. What it does not give is a clean end. Only a round's first block carries a header, so the
block after the current tail of a recycled segment is, depending on how the old life's rounds fell:

1. an old round's header, of this incarnation and a lower position: `scan_segment` returns `Damage::Chain`;
2. a block from the middle of an old round's body: arbitrary bytes, `Damage::Header`.

Both were checked on `scan_segment` (a segment whose old life held two two-block rounds, then one-block round 10
written over its start: rounds `[10]`, end 4,096, `Some(Header)`; with one-block old rounds, `Some(Chain { position: 2 })`).
The recovery turns either into a reported torn-tail truncation naming a round that never existed, at every clean
restart while the tail sits in a recycled segment, which in steady state is always; it then writes zeros over the rest
of the segment. Nothing acknowledged is lost, but the report stops meaning anything. Case 1 alone could be made a clean
end (a complete round below the expected position), though the existing rule that a repeated position is chain damage
stands against it; case 2 cannot, since a block of old body bytes is indistinguishable from a last round whose header
was damaged on the medium, which must stay a reported truncation.

## Open, for later groups

- **An availability risk in the probe**: interior damage is told from a torn tail by searching for a complete round of
  this cluster and incarnation after it. A round body carries client bytes, and a client knows the cluster; a value
  placed at a block boundary inside a round that is later torn could hold a well-formed frame and make that start a
  refusal. It never makes the log accept damage or drop a round: the error is on the safe side. Group 9's body layout
  or stage 10's repair from a peer can close it.
- **Incarnation above 1**: the first API that creates a log afresh over a kept superblock (stage 7's restore) raises it.
- The round-bound and checkpoint-interval defaults are provisional until task 12.4 measures both kinds of drive.
