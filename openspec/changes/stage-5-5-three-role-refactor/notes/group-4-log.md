# Group 4: the ordered log, the WAL and the store (agent 2)

Crates: `ultragraphdb_traits_log`, `ultragraphdb_transactions_log`, `ultragraphdb_store_kv`, `ultragraphdb_store_sim`,
and the fuzz package `fuzz/ultragraphdb_transactions_log`. Logs and scripts are in the session scratchpad (`g4log/`).

## 4.1 The Ultra Style lift, each crate its own step

Each lift was made and its suite run under Bazel before any behaviour changed. `transactions_log` does not compile at
`HEAD` (`protocol_wire` names `EncodedTerm::uid`, which group 2 removed, and agent 1 is moving it), so it was lifted in
a scratch worktree at `95462282`, where its dependencies compile and the crate is byte-identical to `HEAD`, and the
diff (`g4log/lift_wal.diff`, 7 files, +28 −7) was applied to the main tree unchanged. The other three were lifted in
place. `cargo fmt -p` and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` clean after each.

Bazel before and after, the same counts:

| Crate | Unit | BDD | Other |
|---|---|---|---|
| `traits_log` | 9 | — | — |
| `transactions_log` | 181 | 14 scenarios, 28 steps | — |
| `store_kv` | 107 | 33 scenarios, 66 steps | `batch_gib`, `crash` 2, `crash_drop`, `crash_rebuild`, `crash_temporary`, `fjall_workload_checks`, `fjall_workload_crashes` 1 each; doc test passes |
| `store_sim` | 251 | 6 scenarios, 12 steps | doc test passes |

Review of the rules no lint checks:

- **Rule 1.** A self-call scan of every function (`g4log/recursion.py`, group 3's) found no recursion in the four
  crates; its one hit, `Syncer::drop` calling `std::mem::drop`, is not a self-call.
- **Rule 2.** `transactions_log`: the segment scan's `loop` became a `while` naming its end (every round moves the
  offset on by at least a block); the recovery's chain walk is a `for` over at most every segment opened, with an
  error past it (`Diverged`, "the segments the chain crosses"; unreachable, since each turn moves into an existing
  segment one index up); `sync`'s drain and `write_zeros` carry comments saying why they end. `store_kv`: the scan
  iterator's two loops carry their end (each turn consumes a read of fjall's finite iterator), the reverse key read
  became a `let ... else` loop; the stored key's decoder and the sync thread's event loop say why they end; the crash
  writer's wait is the one endless loop, where the test kills it. `store_sim`: the simulation's recoveries in a row are
  bounded by `RECOVERIES_MAX` (1,000), a store still down past it breaking the run as a workload violation; the stepped
  executor's two loops say why they end.
- **Rule 7.** Indexing and slicing replaced by `get`, `get_mut`, `split_at_checked`, `split_first` and `last_chunk`:
  the simulated file's write (`transactions_log`); the stored key's prefix range, end code and version, the GC filter's
  and snapshot's version reads, the rebuild-record and banned-tenant keys (`store_kv`); the corruption of a read
  value, the reader's reread, the model's power-loss prefixes and the workload's key draws, through one `pick` taking
  the same draw as indexing did (`store_sim`).
- **Rule 9.** Every `Rc<RefCell>`, `Arc`, `Mutex` and atomic of the four crates carries a comment saying why it is
  shared and how long a lock is held.
- **Rule 8.** The `cfg`s are `target_os` for `O_DIRECT`, `fallocate` and the sync calls; none added.

## Phases 1–4

Phase 1 surfaces were bare `unimplemented!()` bodies; phase 2 ran the suite against them under Bazel; phase 4's counts
are the final ones.

| Crate (change) | Phase 2 | Final |
|---|---|---|
| `traits_log` (`DurabilityTicket`, the derivation in `RoundRef` and `Round`) | 11 tests, 6 failed, each `not implemented` | 11 |
| `store_kv` (`Durability::Log`, manual journal persistence, the crash writer's `log` mode) | unit 109, 2 failed; `crash` 3, 1 failed (the writer panicked `not implemented` at open, so it printed nothing); BDD 34 scenarios, 1 failed for the same reason; each failure the stub | unit 109; `crash` 3; BDD 34 scenarios, 68 steps; `batch_gib`, `crash_drop`, `crash_rebuild`, `crash_temporary`, both workload targets 1 each; doc test |
| `store_sim` (statistics keys for catalog keys, commits unsynced with checkpoint syncs) | 253 tests, 2 failed: the two new workload tests, on assertions against the old workload (it wrote catalog key `[1, 0…5, 1, 0…7]` at seed 0 step 1, and synced commits); no new surface, so no stub | 253; BDD 6 scenarios, 12 steps; doc test |
| `transactions_log` (tickets, `trim_through` and `checkpoint` by position, superblock format 3, the Round header's derivation in the frame, the scan and `ScannedRound`) | unit 188, 123 failed, every one `not implemented` (the superblock 86, `encode_round` 33, `ScannedRound` 4); BDD 15 scenarios, 14 failed (the fuzz-regression scenario passed) | unit 189; BDD 15 scenarios, 30 steps |

Phase 3 (input variety): every field of the round, ticket and superblock tests distinct (the derivation of round p is
`p % 7 + 1`, so neighbouring rounds differ and only rounds 7 and 14 coincide with the transaction count of 1); the
ticket tests cover a lone round, several rounds under one sync, a lost round, a ticket past the tail before and after
a sync; the kill tests run four delays (the crash test) and three (the scenario), each kill after the writer's third or
second acknowledgement.

Assertions changed in phase 4 alongside the specification, none weakened: `frame_tests`' layout test now reads the
derivation at command-header offset 36 where it read zeros; `store_sim`'s catalog-drop test reads the statistics layout
(`[0x18][tenant][graph 0][predicate]`, a 25-byte entry) where it read the catalog's; two pinned trajectory counts of
`store_sim` moved with the workload (seed 3's reads 201 and rereads 56 became 198 and 55, by the key change alone,
checked by reverting the checkpoint sync; the mixed seeds' failed and retried reads 28 and 20 became 43 and 26).

### The trait (`ultragraphdb_traits_log`)

`append(RoundRef) -> Result<DurabilityTicket, LogError>`; `poll_ticket(&self, DurabilityTicket) -> Poll<Result<(),
LogError>>` — `Ready(Ok)` once `committed()` reaches the ticket's position, `Ready(Err(poisoned))` when a failed write
or sync lost the round, `Ready(Err(position_mismatch(tail, position)))` for a ticket past the tail, `Pending`
otherwise; `sync` unchanged; `trim_through(position)` without a version, the caller having synced the store;
`checkpoint() -> Position` in place of `checkpoint_version()`; `RoundRef` and `Round` carry `derivation: NonZeroU32`,
as `protocol_wire`'s `RoundHeader` takes it. A ticket is a value, not a future, so holding one borrows nothing.

### The WAL

The superblock is format 3, 64 bytes, without the checkpoint's commit version: recovery compares the store's applied
log position with the checkpoint's position (write-ahead-log, "Recovery replays the log above the store"). The scan
refuses as header damage a Round frame whose command header has derivation 0 or an unused byte of 40–127 set
(`RoundHeader::decode`); `ScannedRound::new` takes the decoded `RoundHeader` (8 arguments were one past clippy's
limit). The inherent `Wal::checkpoint` is gone; the trait's answers.

### The store

`Durability::Log` opens fjall with `manual_journal_persist(true)` (set for every mode; the process and filesystem modes
name each batch's durability, so nothing changes for them) and no sync thread: a `PersistMode::Buffer` batch stays in
fjall's 8 KiB journal buffer, handed to the operating system only by a sync, which the test reads off the journal file's
bytes (fjall preallocates its journal to 64 MiB, so its length says nothing). The `Store` trait's durability paragraph
(`ultragraphdb_traits_store`) now states the weaker contract: a batch survives a crash once a sync covered it, the
survivors a prefix, recovery replaying the log above the last one's applied position.

The scenario "A crash loses no acknowledged round" runs on fjall in the kv store's BDD target: the crash writer's `log`
mode syncs each round's line to `<dir>.rounds` (the stand-in log) before writing its batch unsynced, the test kills it,
reopens the store, replays the logged rounds above its applied position and finds every acknowledged round whole.

`store_sim`'s workload writes a predicate's statistics key (`0x18`, default graph) where it wrote a catalog key, drawing
nothing more, and writes its commits unsynced, the 1-in-250 draw that made a commit synced now a checkpoint
`Store::sync` after it. The simulator's crash model is unchanged: a crash between steps keeps what was handed over, a
power loss keeps a prefix since the last sync, which is the outcome a log-mode store leaves after a crash.

## Final Bazel counts

`traits_log` 11; `transactions_log` unit 189, BDD 15 scenarios / 30 steps; `store_kv` unit 110, BDD 34 / 68, `crash` 3,
`batch_gib`, `crash_drop`, `crash_rebuild`, `crash_temporary`, `fjall_workload_checks`, `fjall_workload_crashes` 1 each,
doc test; `store_sim` 254, BDD 6 / 12, doc test.

## Phase 5

**Fuzzing (4.5, the WAL's half).** The `scan` target of `fuzz/ultragraphdb_transactions_log` now checks the new Round
header on every accepted round: it decodes (`RoundHeader::decode`), its unused bytes 40–127 are zero, and the scanned
round carries its header's derivation, first version and decision count. The `superblock` target takes format 3's six
fields. The seeds are regenerated by `testdata/fuzz_seeds/generate_seeds.py`, which lays out the suite's rounds from
`PROTOCOL.md`'s tables with python-xxhash 3.6.0. The unit test `test_the_scan_seeds_are_records_of_the_round_header`
checks that its `three_rounds.segment` matches the log's own encoder byte for byte, and that every seed scans to its
rounds and derivations. `cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_transactions_log` succeeds, and both
targets replayed their seeds (`-runs=0`, the scan's regression case included) without a failure. `scripts/fuzz.sh` was
not run; group 12 runs it.

**Mutation testing** (logs in `g4log/mutants_*`):

| Crate, files | Result |
|---|---|
| `traits_log`, the whole crate | 30: 29 caught, 1 unviable |
| `transactions_log`: `wal_ordered_log` 58 (51 caught, 7 unviable), `wal_write` 17 (16, 1), `wal_recovery` 58 (47 caught, 9 unviable, 2 missed), `wal_open` 12 (11, 1), `wal/mod` 18 (17, 1), `scan` 47 (44, 3), `frame` 2 (both unviable), `scanned_round` 16 caught, `superblock` 24 (23, 1), `sim_file_durable_file` 13 (12, 1) | The 2 missed are the unreachable error after the lift's bounded chain walk (`wal_recovery.rs`, `segments as u64 + 1`), recorded in `.cargo/mutants.toml` and checked with `comm` |
| `store_sim`: `store_workload_keys` 18 caught; `simulation` 16 (13 caught, 2 unviable, 1 timeout); `corrupted` 31 (30, 1 missed); `model_recovery` 38 (33, 4 unviable, 1 missed); `store_workload/mod` 69 (60 caught, 4 unviable, 1 timeout, 4 missed) | The 4 workload survivors are equivalent. They are recorded in `.cargo/mutants.toml` with their arguments and checked with `comm`, and the same four survive at `95462282`. The other two are open (below) |
| `store_kv`, `--in-diff` over the lines this group changed (50 mutants), the `unit` and `fjall_workload_checks` targets | 37 caught, 5 unviable, 1 timeout, 7 missed. The new test `test_a_stored_key_without_its_zero_before_the_end_code_is_corruption` killed `encoded_part`'s `&&`→`||` on a rerun (6 of 6 caught). The 6 crash-writer survivors are open (below) |

`store_kv` ran through `cargo mutants --in-diff … -- --test unit --test fjall_workload_checks`, not `scripts/mutants.sh`
over whole files. Mutating whole files is 243 mutants, and every one runs a suite that takes minutes. On top of that,
the unmutated tree fails under Cargo: the crash and BDD targets find `kv_crash_writer` only through Bazel's
`KV_CRASH_WRITER`, because the `option_env!` sits in `tool_test`, not in `store_kv`.

`tool_test`'s new helpers (`acknowledged_rounds`, `replay_round`) had no mutants run. `tool_test`'s own tests do not reach
them; the kv store's `crash` and `bdd` targets do.

### Open survivors (for 12.1)

- `ultragraphdb_store_kv/src/bin/kv_crash_writer.rs:61` `round_batch` → `Default::default()`; `:69` `batches` → `Ok(())`;
  `:94` `log_rounds` → `Ok(())`; `:175` `main` → `Default::default()`; `:182` `mode == "log"` → `!=`; `:199` delete the arm
  `"log"`. Only the crash and BDD targets run the writer, and under Cargo they cannot find it, so cargo-mutants never
  ran them. Under Bazel, `crash` and `bdd` would catch each of these (they read the writer's lines and its store).
- `ultragraphdb_store_sim/src/types/sim_store/corrupted.rs:49` `Corrupted::remove_all` → `()`. This predates group 4
  (it also survives at `95462282`). It is probably equivalent: after a drop of everything no key is left, and every
  write that brings a key back heals its version itself (`Inner::write` calls `corrupted.remove`), the drop's own
  entries included. The test added for it, `test_a_drop_of_everything_heals_every_corruption`, does not kill it.
- `ultragraphdb_store_sim/src/types/store_model/model_recovery.rs:218` `w.key() == key` → `!=` in `partial`. This
  predates group 4 (it survives at `95462282` on line 217). It is a model gap: no run gives a later surviving batch a
  versionless key other than the one being judged.

## Choices to review

1. **The superblock drops the checkpoint's commit version** (format 3, 64 bytes), and so do the trait's `trim_through`
   and `checkpoint_version`. Recovery now compares the store's applied position with `OrderedLog::checkpoint()`.
2. **`DurabilityTicket` is a value, polled through `OrderedLog::poll_ticket`**, not a future. A ticket past the tail is
   refused as `PositionMismatch(tail, position)`; one whose round a failed sync lost fails as `Poisoned`.
3. **`RoundRef`'s and `Round`'s derivation is `NonZeroU32`**, matching the wire's `RoundHeader`. I adapted the
   sequencer's three call sites myself (`DERIVATION = NonZeroU32::MIN`; its records hold no quads).
4. **`Durability::Log` is added beside Dgraph's `Process` and `Filesystem`** rather than replacing them. About 25
   files across other crates name those two modes. A partition should open with `Log`, and `wal_bench` should measure
   with it. `manual_journal_persist(true)` is set for every mode.
5. **The `Store` trait's durability paragraph is weakened** (`ultragraphdb_traits_store`, doc only). The method stays
   named `applied_ts`, though it now holds the log position; renaming it is a cross-crate change.
6. **The simulator's crash model is unchanged.** A log-mode store loses a suffix after a crash; the power-loss fault
   already exercises that, and the fjall replay can only model a crash by close and reopen.
7. **The simulator's catalog keys became statistics keys** (`0x18`, default graph, the same draws). Commits are now
   unsynced, with a `Store::sync` checkpoint where the 1-in-250 draw used to sync. The fixed and fuzzer-found seeds still
   pass, but I did not re-check that seeds 302, 5495, 2900, 584, 1001 and 1853 still reach the defects their comments
   name. Their comments mention a "schema key" and the step numbers measured before.
8. **The scenarios "A store ahead of the log refuses to start" and "A store behind its checkpoint refuses to start"**
   are decided by the commit step's `Replay` from the store's applied position, so I left them to agent 3. "A crash
   loses no acknowledged round" runs on fjall, with the crash writer's synced `<dir>.rounds` file standing in for the log.
