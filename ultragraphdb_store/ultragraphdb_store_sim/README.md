# ultragraphdb_store_sim

A deterministic, in-memory implementation of the `Store` trait (`ultragraphdb_traits_store`) that injects storage
faults and checks every result against a shadow model. Every storage crate runs its own workload on it from its own
tests; the fjall store (`ultragraphdb_store_kv`) is checked against it operation by operation.

This file lists what the simulator injects, what is detected when it does, and what is neither injected nor detected.
The last list is where detection has to grow, above all once replication (stage 6) and sharding (stage 7) arrive.

## A run

A run is a function of its seed. `Seed` derives one stream per consumer (workload, faults, process, scheduler, cache,
executor), each a `SimRng` (SplitMix64), so a change to one consumer does not move another's draws. Time is
`SimClock` and advances only when the simulation moves it; background work runs on `SteppedExecutor`, which the
simulation steps; the posting cache in simulation is `SimListCache`, with seeded eviction. A failing run reports its
seed, the step and the broken invariant, and the same seed replays it, log events included (`LogCapture` records
events without wall-clock time). A recovery whose own work a fault stops, as a drop or a rebuild finished at open can
be, is followed by another before the next step.

Seed sets run as hermetic Bazel tests in `make test`:

| Crate | Fixed seeds |
|---|---|
| `ultragraphdb_store_sim` | 48 with every fault class, 16 without faults |
| `ultragraphdb_store_posting` | 40 with faults, 20 without |
| `ultragraphdb_core_schema` | 48 of 300 steps with faults |
| `ultragraphdb_store_hnsw` | 30 of 300 steps with faults, 12 without; the same rules once on fjall |
| `ultragraphdb_store_apply` | 12 of 300 steps without faults, 24 with every fault class, 12 with power losses during drops, between steps and at every rebuild stage, and crashes at every rebuild stage; the same operations on fjall over three opens, and a rebuild on fjall killed at each of its stages |
| `ultragraphdb_store_kv` | the store workload replayed on fjall, temporary keyspaces and rebuild records included: 12 seeds without faults, 6 with crashes (61 reopens) |

Longer runs over fresh seeds: `store_sim_fuzz [first seed] [seeds] [steps]`, a binary run outside `make test`.

## Faults injected

`FaultConfig` sets each fault's chance per thousand; `FaultConfig::mixed()` turns every class on, rare enough that a
few hundred steps see most of them. `Fault` forces one at a chosen point in a test.

| Fault | Chance counted | What the simulated store does |
|---|---|---|
| Failed batch | per batch | The batch fails during its persist and poisons the store: every later write, sync and drop is refused until recovery, and after recovery the batch is present whole or absent whole, as fjall replays or drops it. A batch to a temporary keyspace is a batch: its failure poisons the store, and a crash may be drawn while it is written |
| Crash while a batch is written | per batch | The process stops mid-batch; the batch is absent after recovery |
| Crash between steps | per step | The process stops; what was handed to the operating system survives, what was buffered is lost |
| Power loss between steps | per step | A suffix of the batches written since the last sync is lost; the survivors are a prefix of batch order |
| Crash during a drop | per drop | The drop stops at one of its stages (`DropStage`: start, record written, record synced, data removed, entries synced); recovery finishes a drop whose record was synced |
| Power loss during a drop | per drop | As a crash, losing the unsynced stages |
| Crash during a rebuild | per rebuild record written | The rebuild stops at one of its stages (`RebuildStage`: start, record written, record synced, temporary keyspace written, record removed by its swap or failure batch); a stage the rebuild does not reach stops nothing |
| Power loss during a rebuild | per rebuild record written | As a crash, losing what was written since the last sync: an unsynced record, or the batch that removed it |
| Failed read | per read | A point read fails |
| Failed scan | per read | A scan fails after yielding some items |
| Corrupted value | per read | `Truncate` cuts the value below its version prefix; `FlipValueByte` inverts one byte past the prefix |
| Slow operation | per operation | The operation takes up to the configured simulated time |

## What detects a fault

| Fault | Detected by | How |
|---|---|---|
| Failed batch, crash, power loss | the shadow model (`StoreModel`) | After recovery the store's contents equal the model's (`ContentsMatchModel`); every synced batch survives (`SyncedBatchSurvives`); a batch is whole or absent (`BatchWhole`); the survivors are a prefix (`SurvivorsArePrefix`); nothing but recovery follows a failed batch (`RecoveryAfterFailure`) |
| A drop stopped part-way | the shadow model | A dropped prefix stays dropped once the drop's record was synced (`DroppedPrefixAbsent`); the entries a prefix drop writes are fenced and survive garbage collection |
| Failed read or scan | the caller | The store returns `StoreError::ReadFailed` naming the key; every crate propagates it |
| `Truncate` | the store | The read is refused with `StoreError::Corrupted` naming the key |
| `FlipValueByte`, structural | the decoder above the store | The posting decoder refuses a value whose protobuf, operation or type enum, per-block uid count, uid order, posting order or split order no longer holds, with `PostingError::Corrupted` naming the key; the schema decoder refuses an undecodable entry with `SchemaError::Decode` |
| Slow operation | nothing needs to | Results are unchanged; the run checks that nothing depends on timing |
| Reads, garbage collection and timestamps | the shadow model | `ReadsAtOrBelowTimestamp`, `DiscardBelowOpenReads`, `NoWriteBelowDeletion`, `AppliedTsIsHighest`; a temporary keyspace's reads are checked against the versions written to it, and it holds no discard timestamp back |
| A rebuild stopped part-way, at the store | the shadow model | The rebuild records are part of the contents (`ContentsMatchModel`): a synced record survives any crash, a record's removal survives with its batch or not at all, a drop of everything removes every record; every temporary keyspace is gone after a recovery |
| Posting lists | `ultragraphdb_store_posting`'s workload | "a list holds what was written to it" (cached and uncached reads both), "a rollup writes nothing after a drop" |
| Schema loading | `ultragraphdb_core_schema`'s workload | Between failures a load equals the newest written state; after a recovery the loaded state is the state after some batch at or after the last sync |
| Vector index | `ultragraphdb_store_hnsw`'s workload | "a search returns only vectors live at its timestamp, each once, at most k, best first", "a search reports the empty index exactly when no vector is live", "every uid a stored neighbour list or the entry point names has stored neighbour lists, one per level", "the neighbour lists an insert writes name no node dead at its read timestamp"; the decoders refuse a neighbour matrix that does not fill its bytes, an entry point of other than 8 bytes, a vector whose length is not a multiple of its float width and an unparsable dead list, each as the store's `Corrupted` naming the key |
| Index maintenance, rebuilds and drops | `ultragraphdb_store_apply`'s workload | "every index the schema declares holds exactly the entries the data produces", "a drop leaves no key under what it dropped", and after every recovery "no key exists under an index prefix the loaded schema does not declare"; the same rules on fjall after a kill at each rebuild stage |
| fjall diverging from the simulator | `ultragraphdb_store_kv`'s workload test | Every answer, the contents version for version, the applied and discard timestamps and the tombstone horizon, compared after every step |

Each detector was checked against planted bugs: seven in the simulated store (lost synced batches, a partial batch,
over-eager collection, a read above its timestamp, drop entries not written, a crash losing buffered batches, bans
ignored), each reported under the right invariant; two in its temporary keyspaces and rebuild records (a record not
synced, a temporary version written one above its own), caught under `AppliedTsIsHighest` and
`ReadsAtOrBelowTimestamp`; a disabled cache update in the posting commit, caught at once; and three in the vector index
(dead nodes kept, a deleted value read, an extra level), each caught by its rule. The index-maintenance rules found four
real defects while `ultragraphdb_store_apply` was written, among them writes during a background rebuild reading
reverse lists the build had not yet written; the undeclared-index rule found the reverse counts that writes keep while
a rebuild maintains a reversed and a counted schema together, which neither schema declares.

## Injected but not detected

- **A flipped byte that leaves a valid encoding.** `FlipValueByte` can turn a list of uids 1..=10, or a delta of one to
  five postings, into a different list that decodes cleanly, and a protobuf name byte into another valid byte; no
  decoder can tell; in the vector index, a changed uid in a neighbour matrix or a changed float in a vector decodes
  cleanly too. Stored values carry no checksum of their own. The posting, schema and vector-index workloads run with
  `corruption = 0`, and the corruption scenarios use values whose flip is structural. On fjall, a corrupted block fails
  lsm-tree's block checksum and surfaces as a failed read; a corruption that happens before the value reaches fjall
  (memory, a bug above the store) is not caught anywhere.

## Neither injected nor detected

- **Faults below fjall.** fjall does its I/O through `std::fs` with no file-system seam, so the simulator injects at the
  `Store` boundary and fjall's own recovery is exercised only by the kill tests (`ultragraphdb_store_kv`'s crash
  harness: kills during a batch and during a drop). Not modelled: an fsync that reports success and loses the data, a
  write that lands at the wrong offset, lost directory entries, disk full, read-only remounts, a torn write inside a
  journal page.
- **Power loss on fjall.** The fjall replay runs crashes as close-and-reopen; a power loss, a failed batch, failed reads
  and slow operations cannot be injected into fjall and are checked on the simulated store only.
- **Real thread interleavings.** The simulation steps tasks one at a time. fjall's compaction and flush workers, moka's
  internal threads and the thread-backed `Executor` run concurrently in production and are not explored by the seeds.
- **Wall-clock effects.** Simulated time does not model clock jumps or skew; moka's time-based eviction does not run in
  simulation.
- **Corruption of the store's own metadata** (the discard timestamp, the tombstone horizon, the banned set, drop
  records, rebuild records): these are synced when set and are not corrupted by the fault stream.
- **Corruption in a temporary keyspace.** Its reads draw read failures but no corrupted value; a rebuild's temporary
  entries are decoded by the same posting decoder the corrupted-value fault already tests.
- **Compaction failures.** A failed compaction poisons fjall as a failed batch does; the simulator does not inject one.
- **Anything across processes.** One store, one process: no replicas, no network, no Raft log.

## Where detection grows next

- **Checksummed values.** A checksum in the stored value's framing would make every flipped byte detectable, at the
  cost of bytes per version; the corruption workloads could then run with `corruption > 0`.
- **Stage 6, replication.** The Raft log has its own file-level simulated storage (torn writes reported and silent,
  process crash, power loss) and CRC32C-framed records, and a deterministic simulator runs a group of three and its Zero
  over a simulated network. Replicas apply the same entries, so comparing their stores' contents after each step turns a
  silent divergence on one replica into a detected one.
- **Stage 7, sharding and backups.** Backups carry a CRC32C per frame and a SHA-256 per part, and `ultragraph backup
  verify` reads a chain without restoring it; predicate moves and rebalancing add cross-group states the shadow model
  will need to follow.
