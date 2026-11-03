# Group 6: the metadata group

One new crate, `ultragraphdb_cluster/ultragraphdb_cluster_metadata`, task 6.4's second half in
`ultragraphdb_cluster_sequencer`, and new variants in `ultragraphdb_error_cluster`. The catalog side of group 6
(`core_catalog`, the partition's loading, default entries on the partition, index builds) is the catalog agent's.

- `ultragraphdb_cluster_metadata`: `MetadataGroup<L: OrderedLog>`, `MetadataState`, `MetadataCommand`,
  `MetadataSnapshot`, `MetadataConfig`, `CatalogRecord`, `Principal`, `LeasedBlocks`.
- `ultragraphdb_cluster_sequencer`: `SequencerSnapshot` (encode/decode), `Sequencer::snapshot`, `Sequencer::trim`,
  `Sequencer::recover_from`; `recover` is now `recover_from(.., None)`.
- `ultragraphdb_error_cluster`: `StaleCatalog`, `NotPending`, `CatalogReleased`, `BlockSize`, `SequencesSpent`,
  `NoPlacement`, `Contradicts`, `PrincipalName`, `PrincipalExists`, `UnknownPrincipal`, `LastAdministrator`,
  `Bootstrap`, `SessionExists`, `SessionEvicted`, `SnapshotBehind`, each with its constructor, text and `source`
  `None`. `Display` is split into six helpers (placement, settings, sequencer, damage, catalog, access) so each stays
  under 70 lines with an exhaustive match.
- Root `Cargo.toml` (one workspace dependency; the member glob `ultragraphdb_cluster/*` already covers the crate),
  `AGENTS.md` (the crate's row; the `error_cluster` and `sequencer` rows), `fuzz/Cargo.toml`, the fuzz package
  `fuzz/ultragraphdb_cluster_metadata` (`command_decode`, `snapshot_decode`) and the sequencer package's new
  `snapshot_decode` target.

## What the group is

A sans-I/O state machine over `OrderedLog`, as the sequencer is. Every command is one round whose body is a
`MetadataCommand`; the group decides it against the **head** state (every command appended, durable or not), appends
it, and applies it to the **durable** state reads answer from only once `sync` reports it durable, so a read never
sees what a crash could lose and every read names its position (`applied`). On a round the log refuses, the head is
rebuilt from the durable state and the staged commands, so a refused command changes nothing.

Every command records its decision whole (a lease its block, a principal its identity, a registration the sessions it
evicts, a close the versions it announced), so `MetadataState::apply` needs nothing but the state before it and replay
never reads the settings: `sessions_max` or `block_size_max` changed between starts changes only later decisions.

- **Catalog versions.** `offer_catalog(ns, predecessor, bytes)` numbers the version `predecessor + 1` and refuses a
  predecessor other than the namespace's latest (pending or not) with `StaleCatalog { namespace, predecessor, current }`.
  The bytes are opaque (`CatalogContents::encode`). `pending_versions()` is what the sequencer's next `tick` announces;
  `learn_close(epoch, versions, evictions)` records the durable close and assigns each announced version
  `epoch + 1` (the close of `c + 1` gives `c + 2`). `catalog_at(ns, e)` answers every epoch up to one past the last
  learned close (a pending version cannot take effect earlier) and refuses further with `EpochAhead`; the namespace's
  initial catalog is `None`. `MetadataState` implements the catalog agent's `CatalogSource` (`Error = ClusterError`).
- **Default entries**, by (namespace, predicate) beside the versions: `record_default_entry(&DefaultEntry)` answers
  `None` when a record exists (found in the head, so a second ask before the sync appends nothing), creates no version,
  and the predecessor check ignores it.
- **Retention.** `report_untrimmed(partition, epoch)` records where a partition's untrimmed log begins; once every
  partition of the placement map's latest layout has reported, versions superseded at or before the oldest such epoch
  are released, and `catalog_at` an epoch before the oldest retained version refuses with `CatalogReleased`.
- **Sequence blocks** from per-type high-water marks: a block of 1 to `block_size_max` (`BlockSize { size, max }`),
  past 2^64 − 1 `SequencesSpent`.
- **Placement**: `init_placement` (even, once; twice is `Full("placement maps", 1)`), `change_placement` (up to the
  map's 2^18 ranges, the map's own checks naming the first bucket), `set_preferred_leader`; before the map,
  `NoPlacement`.
- **Large commands** (decided by the user): a command whose encoding passes `CHUNK_BYTES` (128 KiB, so a round fits
  the 1 MiB frame and a test log's 256 KiB segment) is appended as `Chunk { offset, bytes }` rounds and a
  `Commit { length, checksum }` naming the whole by its length and XXH3-64. The command is decided and staged at the
  commit's position, so it stays pending, unread and with no effective epoch, until the commit is durable. Replay
  reassembles the chunks and applies the command at the commit; a run no commit follows (before another command, a new
  run from offset 0, or the log's end) is discarded, a chunk not at the run's end or a commit not naming the run diverges.
  A snapshot is the state at `applied`, the last applied command's position, so it holds no half-written command.
- **Principals** with SCRAM verifiers, numbered from 1 and never reused; `bootstrap(name, secret_file, salt,
  iterations)`; the last administrator kept; `MetadataState` implements `Credentials`.
- **Sessions**: `register_session(session, principal_name)` evicts the oldest past `sessions_max` (at most 4,096 at
  once) into the pending evictions the next close carries (`pending_evictions()`, at most 4,096); `learn_close` removes
  the evictions it carried. `session_principal` is the frontend's owner check.
- **Snapshots and trimming**: `snapshot()` is the durable state at `applied`; `snapshot_due()` after
  `snapshot_rounds` durable rounds; `trim(&snapshot)` trims the log through it; `recover(log, config, snapshot)`
  replays the rounds after the snapshot and reports `replayed()`.
- **`LeasedBlocks`** implements group 3's `BlockSource` over blocks the partition's host leased and saw durable.

## Decisions

1. **Snapshots are bytes the host stores, not rounds of the log.** A snapshot can pass a round's 1 MiB frame (the
   placement map alone holds up to 256 layouts of 2^18 ranges; the registry up to 2^20 sessions), so the crates hand the
   host a `MetadataSnapshot` / `SequencerSnapshot` to store durably (`/data/meta`, `/data/seq`) before `trim`;
   recovery takes the last one. Recovery refuses a snapshot below the log's checkpoint (`SnapshotBehind`, also for no
   snapshot over a trimmed log) and one past the committed position (`ReplayDiverged`). The host's file handling is
   group 11's.
2. **The sequencer's snapshot is its durable closes retained and every transaction it holds.** The sequencer has no
   notion of a multi-partition transaction finishing, so "open" is "held"; trimming the log frees disk, not the held
   transactions, which stay bounded by `transactions_retained` as before.
3. **The host learns every durable close before the next tick**, in order: the first close learned may be any epoch,
   later ones the one after the last (`EpochAhead` otherwise), and a close at or before the last learned changes
   nothing, so after a restart the host hands over every close the sequencer holds. A version announced twice (a host
   ticking before learning) is refused at the second learn as `NotPending`.
4. **A decision a command records that contradicts the state** (a block not at the high-water mark, a principal not the
   next identity, evictions not the oldest sessions) is `Contradicts`, not an invariant: replay reads it from disk,
   which is input, and `invariant` panics in a debug build. Recovery reports any refusal as `ReplayDiverged` at the
   round's position.
5. **`LeasedBlocks` rather than `BlockSource` on the group itself.** `BlockSource::lease(&self)` is synchronous and
   must not hand out a block before its lease is durable; the group's lease is durable only after `sync`. The partition's
   host leases ahead and supplies durable blocks (at most 16 per type); the store's `lease` pops them. Its clones share
   one queue (`Arc<Mutex>`, as `TestBlockSource`), since the store requires `Send + Sync`.
6. **The bootstrap secret file** is UTF-8 whose whole content, one trailing `\n` or `\r\n` left out, is the password;
   the host passes the principal's name, the salt and the iteration count (the frontend's settings). The group keeps the
   verifier only. A group holding a principal, durable or not, refuses a bootstrap.
7. **An unknown session is `SessionEvicted`**: the registry keeps no evicted sessions, and identities are 128 random
   bits, so a session it does not hold was evicted or never registered.
8. **Limits per command**: a catalog version holds at most 16 MiB and a placement change the map's 2^18 ranges, a
   command 32 MiB in chunks (the user removed the frame-bound caps, decision 10); a predicate's name 4,096 bytes, a
   default entry 65,536, a name and a salt 255.
9. **Index builds** need nothing from the group: their start and completion are records in each partition's log
   (design, "The sequencer and the metadata group").
10. **Large commands are split across rounds** (the user's decision): chunks and a commit, as above, in place of the
    256 KiB cap on a version and the 16,384-range cap on a change.
11. **The bootstrap principal's name is a frontend setting** (accepted by the user, as decisions 1 to 8).

## Phases

### 1. Surface

Every type, method, constant and error variant, logic bodies bare `unimplemented!()`, trivial getters and constructors
written; built under Bazel. `SURFACE READY` posted with the API; the catalog agent's `CatalogSource` and `DefaultEntry`
adopted (`record_default_entry` takes a `DefaultEntry`).

### 2. Suite

- Scenarios, verbatim (checked line by line against the specs by script): `features/metadata_group/` (17: every
  requirement of `specs/metadata-group/spec.md`), `features/system_identity/` (4: "Sequences are unique per type and
  leased in blocks per partition"), `features/schema_changes/` (2: "A schema change takes effect at an epoch
  boundary"). The epoch scenarios run beside the real sequencer (`tests/utils/cluster.rs`); the partition sides are
  models (`tests/utils/partition_model.rs`: a partition stamping the epoch after its last delivered close and
  validating against the group's `CatalogSource`, and an allocator taking sequences as the store's `Sequences` does),
  as each feature's header says. The placement map's four scenarios run through the group's commands.
- Unit tests mirroring `src/`; the recovery workload (`metadata_group_crash_tests.rs`) checks the recovered state
  against a reference model applying the same commands, killed after every append before and after its sync and with a
  fault at every storage operation in each crash mode.
- Expected values hand-written: every command kind's encoding and a small snapshot's, byte by byte; offsets of
  refusals; the even split's sizes; block ranges.

Run against the surface: metadata unit 122 tests, 117 failed, every one `not implemented` at a `src/` stub (the 5
passing assert constants and getters); bdd 23 scenarios, 23 failed (`Step panicked. Captured output: not
implemented`); sequencer unit 110, the 15 new ones failed on their stubs; `error_cluster` 13 passed (the constructors
and texts are values).

### 3. Input variety

Reviewed for coinciding quantities: leases of different sizes per lease, so an end is not a start plus a constant;
versions whose number differs from their position (version 2 at position 4) and from their effective epoch; distinct
partitions, nodes, salts and sessions per test; principals whose name order differs from their identity order in the
snapshot (`zed` 1, `amy` 2) and sessions registered out of numeric order (30, 10, 20); a close's effective epoch checked
both where a version is in effect and the epoch before.

### 4. Green

`bazel test //ultragraphdb_cluster/... //ultragraphdb_error/ultragraphdb_error_cluster/...`: metadata unit 132 passed
(122 at first green, 10 added by the mutants' fix pass), bdd 23 scenarios / 46 steps passed, doc test passed; sequencer
unit 111 (110 + 1 from the fix pass), bdd 19; placement 55 and 4; `error_cluster` 13.
`cargo fmt` and `cargo clippy --all-targets` clean for the three crates. One test was wrong, not the code: a secret file
ending in two line endings leaves a `\n` in the password, which SASLprep refuses; the case moved to the refusals.

`bazel build --keep_going //...` builds everything but `ultragraphdb_error_protocol`'s tests and
`ultragraphdb_client_core`, both broken by the wire agent's in-flight role commands (their NOTEs), neither touched here.

### 5. Hardening

Fuzz: `cargo +nightly fuzz build` of both packages; seeds replayed with `-runs=0`: `command_decode` 14 seeds,
`snapshot_decode` 4, the sequencer's `record_decode` 10 and `snapshot_decode` 3, no failure. Seeds are written by
`testdata/seeds.py` from the formats' documentation and tied to the encoders by `test_the_fuzz_seeds_are_the_encoders`.

Mutants (`scripts/mutants.sh <crate> <file> --diff cabae5ec`), 26 files:

| File | Run 1 | Fix pass | Re-run |
|---|---|---|---|
| `metadata_state_apply.rs` | 57: 53 caught, 3 missed (the evictions-awaiting-a-close bound, `>`/`+`), 1 unviable | `test_at_most_65536_evictions_await_a_close`; the registry gained an index by session (`owners`), since the existence and owner checks scanned the registry linearly, which 2^20 sessions make quadratic | `--re register_session`: 8 caught |
| `metadata_command_codec.rs` | 27: 18 caught, 9 unviable | | |
| `metadata_snapshot_codec.rs` | baseline failed (the tree was copied mid-edit of the index above) | run 2: 54, 17 missed (the order checks, the placement map's offset) → `metadata_snapshot_order_tests.rs`, hand-written bytes with one value out of place each; the check `released < current` removed, as implied by the first retained version lying above the released one | run 3 `--re` over the five functions: 39: 37 caught, 2 unviable |
| `metadata_state_catalog.rs` | 31: 24 caught, 7 unviable | | |
| `metadata_group/mod.rs` | 12: 11 caught, 1 missed (`config()`) | `test_a_group_runs_with_its_own_settings` | run 3 `--re config`: 1 caught (run 2's `--re` printed nothing to the filter) |
| `metadata_group_recover.rs` | 18: 16 caught, 1 unviable, **1 missed: `READ_BYTES` `1 << 20` → `1 >> 20`** | open: a read still returns at least one round and the loop is bounded by the rounds left, so the mutant changes how many reads replay takes, not what it replays; equivalent in behaviour, left for task 12.1 | |
| `metadata_group_access.rs` | 18: 15 caught, 3 missed (the bootstrap's own name check, implied by `create_principal`'s) | `test_the_bootstrap_checks_the_name_before_the_file` (a bad name over an unreadable file) | `--re bootstrap`: 10 caught |
| `metadata_group_blocks.rs` | 9: 8 caught, 1 unviable | | |
| `metadata_group_catalog.rs` | 10 caught | | |
| `metadata_group_snapshot.rs` | 5: 4 caught, 1 unviable | | |
| `metadata_state_access.rs` | 14: 12 caught, 2 unviable | | |
| `metadata_state_catalog_source.rs` | 9: 6 caught, 3 unviable | | |
| `metadata_state_credentials.rs` | 3: 1 caught, 2 unviable | | |
| `metadata_state/mod.rs` | 4: 3 caught, 1 unviable | | |
| `metadata_config/mod.rs` | 22 caught | | |
| `leased_blocks/mod.rs` | 20: 4 caught, 16 unviable | | |
| `leased_blocks_block_source.rs` | 2: 1 caught, 1 unviable | | |
| `reader.rs` (metadata) | 32 caught | | |
| `sequencer_snapshot/mod.rs` | 29: 19 caught, 10 unviable | | |
| `sequencer_snapshot_codec.rs` | 10: 6 caught, 3 unviable, 1 missed (a record's refusal offset `+` → `-`, every test's inner offset 0) | `test_a_records_refusal_is_named_at_its_offset_in_the_snapshot` | `--re read_framed`: 2 caught, 2 unviable |
| `sequencer/sequencer_snapshot.rs` | 4: 3 caught, 1 unviable | | |
| `sequencer_recover.rs` | 11: 9 caught, 2 unviable | | |
| `reader.rs` (sequencer) | 2 caught | | |
| `cluster_error_display.rs` | 7 caught | | |
| `cluster_error/mod.rs` | 15 unviable | | |
| `cluster_error_error.rs` | 2: 1 caught, 1 unviable | | |

Survivors left open for task 12.1: `metadata_group_recover.rs` `READ_BYTES`, equivalent as argued above, and the two
chunk-bound mutants named under "Chunked commands".

## Chunked commands (after the user's decision)

Tests: `test_a_version_larger_than_one_round_commits_across_rounds_and_reads_back_whole` (300 KiB, three chunks and a
commit, pending until synced, recovered whole), `test_a_command_one_byte_past_a_chunk_takes_two_chunks` (the boundary
at 131,072 bytes), `test_a_crash_between_chunks_leaves_the_version_absent` (the run's sync failed at every storage
operation in turn: the version whole or absent, the snapshot the state before the run, a retry committing it; at least
one case leaves chunks without their commit), `test_chunks_with_no_commit_are_discarded_and_a_retry_commits`,
`test_chunks_with_their_commit_replay_as_one_command`, `test_chunks_out_of_order_or_a_commit_not_naming_them_diverge`,
`test_a_placement_change_of_more_than_16384_ranges_commits_across_rounds` (20,001 ranges, two chunks),
`test_a_chunk_or_a_commit_is_no_command_of_the_state`; the codec's kinds 12 and 13 by hand; the log-refusal test now
fills a two-segment log. Fuzz seeds `chunk_at_16` and `commit_of_19_bytes` added (16 seeds; `malformed_kind_12` became
`malformed_kind_14`, regenerated by `seeds.py`), replayed clean. Metadata unit 142 passed (with `test_chunks_of_the_most_bytes_of_a_command_are_taken`, 32 MiB of chunks at the
bound), bdd 23 scenarios; `bazel test //ultragraphdb_cluster/...` green.

Mutants on the files whose logic changed, before the user stopped mutation testing:

- `metadata_command_codec.rs` run 2 (diff): 27 mutants, 18 caught, 9 unviable.
- `metadata_group_recover.rs` run 2 (diff): 31 mutants, 27 caught, 1 unviable, 3 missed: `READ_BYTES` (equivalent, as
  before) and the chunks' bound `chunks.len() + bytes.len() > COMMAND_BYTES_MAX` (`>` → `>=`, `>` → `==`).
  `test_chunks_of_the_most_bytes_of_a_command_are_taken` was written to kill the two; its `--re` re-run was cancelled
  at the user's instruction before it started.
- `metadata_state_apply.rs` run 3 (`--re MetadataState::apply`): no mutant in the new arm.

**Mutants deferred to task 12.1** (changed by the chunked commands and not run since): `src/types/metadata_group/mod.rs`
(the chunked append; its three runs were used before the change), `src/types/metadata_group/metadata_group_recover.rs`
(the re-run of `replay_round` killing the chunk-bound survivors), `src/types/metadata_command/mod.rs` (constants only).
