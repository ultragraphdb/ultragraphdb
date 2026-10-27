# Group 11.2: the Lean model of the round protocol

`lean/`, library `RoundModel`, Lean 4.14.0, core only (no Mathlib, Std or Batteries). `lake build` passes; no
`sorry`, `admit` or `axiom` in the sources, and `#print axioms` on every main theorem names only `propext`,
`Quot.sound` and `Classical.choice`.

## Structure

| Module | Holds |
|---|---|
| `RoundModel.Basic` | `Key`, `Value` (both `Nat`), `Write`, `State` (key → latest value), `applyWrites`, `lookupWrite`; applying writes is idempotent (`applyWrites_idem`) |
| `RoundModel.Txn` | `KeyRange`, `ReadSet` (points and ranges, `covers`), `Op`, `OpResult`, `TxnRequest`, `resolve`, `Commit` |
| `RoundModel.History` | `serial` (commits run in order), `snapshot` (commits at or below a version), `maxVersion`, `maxReq`, `lastCommitOf`, `Admissible`, `Valid` |
| `RoundModel.Round` | `conflicts`, `nextVersion`, `Outcome`, `decideTxn`, `runRound`; a round's commits are `Admissible` (`runRound_valid`) |
| `RoundModel.Log` | `Round`, `commitsOf`, `Store`, `applyAt`, `replay`, `replayAt`, `recover`, and the replay lemmas |
| `RoundModel.Node` | `Node`, `Step`, `Steps`, `Reachable`, `SyncDurable`, `Inv`; the invariant holds of every reachable state |
| `RoundModel.Theorems` | The four properties, each with a doc comment stating the simulator invariant that restates it |

**Histories.** The history is the log's commits in log order. `Admissible pre c` is what the commit step guarantees of
a commit decided after `pre`: its read version is at or below a committed version, its version is even and above
every earlier one, no earlier commit above its read version wrote a key its read set covers, its logged writes and
results are what it resolves to against `serial pre`, and its request number is one above its session's last.
`Valid H` states it of every commit against the commits before it. Every theorem rests on `Inv.valid`: the history
of every reachable state is `Valid`.

**The commit step.** `decideTxn H clock t`, in order: a retry of the session's last request returns its stored reply;
any other number than last + 1 is refused as out of order; a conflict aborts; a failed compare-and-set refuses;
otherwise the transaction commits at `max (maxVersion H + 2) (2 × clock)`. `runRound` decides a round's requests in
order, each against `H` and the commits admitted before it; an aborted request adds nothing (`runRound_aborted`).

**The node.** `Node` holds the log, the synced prefix's length, the store and a poison flag. `Step` is one of:

| Step | Effect | Premise |
|---|---|---|
| `round` | decide a round against the whole history, synced or not, and append it | not poisoned; every request's read version ≤ the watermark |
| `sync` | the next round's sync completes | not poisoned; an unsynced round exists |
| `syncFail` | poison the log | — |
| `apply` | apply the next synced round to the store | not poisoned; a synced, unapplied round exists |
| `crash fjallKept kept` | power loss, restart and recovery in one step: the log keeps its first `kept` rounds, the store its first `fjallKept`, recovery replays the kept log above the store, the poison clears | `fjallKept ≤ applied`; `SyncDurable` (`synced ≤ kept`); `kept ≤ log length` |

The acknowledged commits are those of the store's applied prefix (`Node.acked`), which `Inv` holds within the synced
prefix; the watermark is their highest version (`Node.watermark`).

## Theorems

Every one holds of every reachable state, so for every number of transactions, rounds, failed syncs and crashes.

### 1. Strict serializability

- **`serializable`** — in the history of every reachable state, each commit's reads at its read version equal, for
  every key its read set covers (points and ranges), the state the commits before it produce in commit order; its
  logged writes and results are what it resolves to against that state; its read version is below its commit version.
- **`reads_are_serial`** — what a transaction read from the store of the state it executed in, at a read version at
  or below that state's watermark, equals the state the commits before it in any later history produce.
- **`snapshot_stable`** — a read at or below the watermark gives what every later history gives at that version:
  every acknowledged commit is visible, and no commit becomes visible later.
- **`read_only_at_watermark`** — a read at the watermark returns the serial state of the acknowledged commits, a
  prefix of the commit order: a read-only transaction serializes there.
- **`commit_order_is_version_order`** — commit versions are even and strictly increase along the history.
- **`real_time_order`** — a commit acknowledged in a state precedes, in every later history, every commit whose
  transaction began after it (read version ≥ its commit version). **`began_after_ack`** — a transaction beginning at
  the watermark of any later state has such a read version (the watermark never decreases, `Node.watermark_mono`).

### 2. Durability

- **`durability`** — a commit acknowledged in a state is, in every later state, still acknowledged and in the store,
  and the store read at its commit version returns each of its writes.
- **`crash_recovers_synced_log`** — after a crash the store equals the replay of the kept log, and the kept log holds
  the synced prefix.
- **`failed_sync_acknowledges_nothing`** — from a poisoned state, every step that does not restart the node leaves
  the history and the acknowledged commits unchanged.

### 3. Idempotent replay

- **`idempotent_replay`** — replaying a round at or below the store's position leaves the store unchanged; replaying
  the whole log, each round through its position, onto the store of any prefix gives the log applied once; recovery
  from the store of any prefix of a synced prefix equals applying that synced prefix once, and recovering again
  changes nothing; the store equals the replay of the log's applied prefix from an empty store. Built from
  `applyAt_applied`, `replay_whole_log`, `recover_prefix` and `recover_idem` in `RoundModel.Log`.
- **`round_apply_idem`** — applying a round's commits twice in a row to a state is applying them once, because the log
  holds resolved writes, assignments, and not operations: a counter add is logged as the value it produced.

### 4. Sessions

- **`session_commits_at_most_once`** — in the history of every reachable state, at most one commit carries a given
  session and request number, and within a session request numbers strictly increase in commit order.
- **`acknowledged_request_commits_once`** — once a commit is acknowledged, every later history holds exactly that one
  commit under its session and request number.
- **`decideTxn_retry`** — a retry of a session's last committed request is answered with that commit's stored version
  and results, and commits nothing.

### The commit-time operations' own properties

`upsert_result_held` (after an upsert the unique key holds the identity returned, created exactly when absent),
`upsert_twice_one_node` (two upserts of one key resolve to one node; the second does not create),
`counterAdd_comm` (two adds commute), `compareAndSet_refuses` (a failed compare-and-set refuses the whole transaction,
reporting the value found).

## Assumptions

- **`SyncDurable`** — a crash keeps at least the synced prefix of the log: a completed sync is durable. It is the
  `crash` step's premise. A round torn while its sync reported success breaks it and lies outside the model; the spec
  has recovery report such a round by name.
- **Transactions begin at the watermark** — the `round` step's premise that every request's read version is at or
  below the current watermark. A transaction reads at the watermark when it begins, and `Node.watermark_mono` proves
  the watermark never decreases, across crashes too, so a read version taken at begin satisfies it at commit.

## Model choices the spec leaves open

- **Commit-time operations see every earlier commit, in flight or not.** An operation is decided against
  `serial` of every commit before it, including rounds appended but not yet synced or applied. design.md's commit step
  says operations run "against the store at W plus the round's admitted writes". With round `k + 1` decided while
  round `k` syncs, W excludes round `k`, and two counter adds in rounds `k` and `k + 1` would both read the value
  before `k`: a lost update, and `serializable`'s second clause fails. The model takes the reading under which the
  theorem holds; the implementation's state for operations has to include the rounds in flight.
- **An operation sees its transaction's own writes,** applied before the operations, and each operation the writes
  of the operations before it.
- **An aborted transaction takes no version:** "previous" is the highest commit version.
- **Upsert-and-link's existing-node case writes only its links,** not the unique key again, so a reader of the key
  does not conflict with an upsert that created nothing.

## Simplifications

- Keys and values are unbounded `Nat`s; a deletion is not a separate operation.
- A link is a write of the node's identity under the link's key, not an append to a posting list; index maintenance,
  reverse lists and statistics a changeset derives are not modelled.
- The store's multiversion record is the list of the commits it applied, read through `snapshot`.
- Crash, restart and recovery are one step. The clock is one number per round.
- The model allows any number of rounds appended ahead of the sync; the spec's rule that a round closes when the
  previous sync completes is one schedule among them.

## Not modelled

- Transaction execution: reads of a transaction's own writes, the read-write and read-only lifetime limits.
- Pruning the conflict history and the too-old abort; busy, too-large and catalog refusals.
- Catalog changes, drops and index builds (D7); rollups and their odd versions; the vector indexer (D8).
- Session registration, the bounded table and eviction; one commit in flight per session is not a premise, and the
  at-most-once theorems hold without it.
- The WAL's bytes: framing, checksums, segments, the scan, torn tails, interior corruption, the superblock,
  checkpoints, `trim_through` and `seal`; the store-ahead-of-log and gap refusals, which no reachable state reaches.
- Reordering within a round, replication and VSR.
