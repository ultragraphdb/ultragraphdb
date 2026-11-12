# UltraGraphDB Specification Update for the Two-Role Architecture

Nov 12, 2026 · the step before the change sets of stages 5.8 and 5.9 are derived and those of stages 6.5 and 6.6 are
rebased (`../../ROADMAP.md`, `../designs/UltraGraphDB Distribution Architecture.md`)

The target specifications under `specification/` are brought to the architecture once, so that all four change sets
are checked against one text. This note records the rules the update followed, the decisions it took, the decisions it
leaves to the user, what it wrote and what each change set still has to write.

## Rules of the update

1. **Every change names the stage it takes effect.** Text that the code implements today stays until that stage's
   change set removes it, since tests and golden frames cite it.
2. **No section is renumbered.** New text takes a new number (`5.4`, `11.14`, `13.23` to `13.28`, `12.5`, `17.7`) or the
   letter form the protocol already uses (`11.12a`, after `11.2a`).
3. **A trade-off is decided by the principles of `AGENTS.md`, in their order:** correctness, speed, scale, developer
   experience. Only a fundamental design decision goes to the user.

## Decisions taken

| # | Decision | Decided by |
|---|---|---|
| S1 | The session registry, the owner check and the mark of a commit in flight stay together, in the system group, as the frontend's owner holds them today. The client endpoint asks by `Check` and tells by `Ended`. One round trip per commit, batched per turn, until backlog item 13 is answered | Correctness: the registry evicts only a session with no commit in flight, so the three cannot be split without a proof. It also makes several compute nodes correct for sessions with no further rule |
| S2 | A principal's verifier never leaves the system group: `Challenge` and `Verify`, specified for stage 8, serve from stage 5.8 | Correctness, defense in depth: today the owner hands the verifier to an endpoint core |
| S3 | The counts of failed authentications, the throttle and the ban stay with the process that accepted the connection | Speed: a ban decided remotely would put a round trip before TLS. With several compute nodes a source gets one allowance per node, which is bounded |
| S4 | The role holding the client endpoint subscribes to the sequencer's closes as storage does, and sends `CloseRequest` with `partition` 0 for a snapshot read | Speed and scale: two existing commands serve, and a close reaches every process within one epoch |
| S5 | The node list reaches a client in a frame of its own, `Nodes`, after the connection authenticated, and not in the Welcome | Correctness, defense in depth: the Welcome precedes authentication, and a peer that has not authenticated should learn nothing of the cluster. **This departs from the instruction to carry the list in the Welcome** |
| S6 | The node list's version is the epoch of the close that announced its change, named by one flag in `EpochClose`; a compute node then reads the list with `Roster` | Correctness: the version is monotonic and durable with no state of its own. This is how the placement version already travels |
| S7 | A compute node is a member while its Subscribe stream to the system group is open, and announces draining by a Join | Scale: membership needs no heartbeat of its own |
| S8 | A node carries one to eight tags, and no request names a tag | Developer experience, after the three above were indifferent: one machine may serve two groups |
| S9 | A notice of a new list carries the list itself | As the user described it: the client compares versions and swaps |
| S10 | A wrong tag is refused with a code of its own, 32 `TagRequired`, from stage 7, when procedures exist | Correctness: a refusal a client can act on without parsing a message |
| S11 | An object's key holds the extent's number and the checksum of its placed blocks, the bytes from its first block to the last a tile was placed in | Correctness: an address is reused, and a content key makes publication idempotent and every fetch verifiable. Hashing the placed blocks alone keeps the key equal on every replica whatever lies in the unused tail |
| S12 | A node keeps no durable record of the extents it punched. A punched extent reads as zero, fails its tile's check and is repaired from the object store by the path that repairs a damaged tile. Replaces the presence record first written on this day | Correctness: a record of presence is a second source of truth that a crash can leave wrong, and the tile's own checksum already decides. It also asks nothing of the file system beyond what the direct binding asks |
| S13 | Only cold extents (stream 2) become absent | Speed: the write path, which reads hot tiles, fetches seldom |
| S14 | While the object store is unreachable, apply and compaction wait and the partition refuses with Busy or Unavailable | Correctness: it stalls and never answers from bytes it could not check, and no decision depends on I/O timing |
| S15 | Retention of published checkpoints follows the sequencer's logged close times | Correctness: retention is a function of the log, as the fold horizon is |
| S16 | A restore that gives up acknowledged commits runs only on an operator's command | Correctness: a loss is never silent |
| S17 | The binding is equal across a cluster's storage instances and fixed before stage 9.5 | Correctness: a node's recovery depends on what its peers published |
| S18 | Load control (credit to send, signals, probe) is named and left out of this protocol version | Scale: go slow. It is a direction to be proved and simulated before stage 8 |
| S19 | Every closed and released extent has a record, its placed blocks and their XXH3-64, held in extent leaves and an extent root (record kinds 21 and 22) that the superblock references and the checkpoint identity covers, now 58 bytes. Both bindings write them | Correctness: the key of an object is derived from checkpointed state, equal on every replica, and one format serves both bindings, so the direct binding's tests cover the object binding's records |
| S20 | The extent root holds, beside each leaf's reference, the record of the extent that leaf lies in | Correctness: a restore finds every extent leaf from the checkpoint record and the root alone, with no listing of the object store |
| S21 | A checkpoint record is the superblock's 4,096 bytes with the records of the extents the checkpoint's own step 2 closed, keyed by the checkpoint's log position and written last | Correctness: a record names only objects that exist, and the newest record is found by key order |
| S22 | The byte layouts of the object binding are specified now, and stage 6.5 implements them against vectors | Correctness: the grid's superblock and identity change with them, and a format changed after the grid is built is a second format |
| S23 | Stage 5.8 specifies two catalog commands, `Declare` (538) and `Defaults` (539), which carry what the owner answers in process today. The offer of a compiled version takes 540 and is specified by stage 6.6's change, with the fence in `EpochClose` | Correctness: stage 5.8 moves behaviour and adds none, so its suites compare before and after. A body for an Install that nothing yet sends would be specified untested |
| S24 | The system group links the catalog's format alone, its decoder, encoder and comparison, and decodes and re-encodes every offered version before it commits | Correctness, defense in depth: every partition stops on a version it cannot decode, so the process that commits a version checks it, and no parser of client text runs there |

## Decisions for the user

1. **Decided 2026-11-12: compute compiles.** Where an Install compiles, and who executes the catalog's requests. The schema specification has the compiler
   run "beside the metadata group", decided 2026-11-10 when that group lived in the frontend. From stage 5.8 that would
   put a parser of client text into the storage process that hosts the system group. The alternative: compute compiles
   and offers the compiled version with the version it was built on, and the system group checks the base and commits.
   *Recommendation:* compute compiles. No parser runs in the process that keeps the cluster's books, and the check of
   the base version keeps allocation exact. The requests are `Declare` (538) and `Defaults` (539) from stage 5.8, and
   the offer of a compiled version at 540 from stage 6.6 (S23).
2. **Decided 2026-11-12: the fence travels with the epoch closes and the partitions enforce it.** Rules one frontend
   enforced for the whole cluster. The fence of a first schema (stage 6.6) is held by the one
   frontend. With several compute nodes every node must refuse the fenced tenant's writes before the Install reads the
   tenant. *Recommendation:* carry the fence with the epoch closes and enforce it at the partitions, as evictions
   travel today. The alternative is one compute node per cluster until stage 8.
3. **Decided 2026-11-12: slots leave their replica only inside tiles streamed for a projection.** An analytical read
   that returned slots in its answer would contradict `STORAGE.md` section 14.6 and decision T3. Tiles, slot maps and
   identity columns streamed to compute for a projection at a closed epoch or a published checkpoint carry slots; the
   projection is immutable, held in memory, named by its checkpoint identity and discarded with it. No answer, resume
   key or durable state outside the replica names a slot.

## What was written

| File | Section | Stage | What |
|---|---|---|---|
| `PROTOCOL.md` | preamble | 5.8, 5.9 | Roles by stage, and how the other sections are read from each stage |
| | 2.5, 3 | 5.8, 5.9 | `Nodes` (27); `Session`, `Check`, `Ended`, `Roster` (534 to 537); `Declare`, `Defaults` (538, 539); 540 kept for stage 6.6; `ComputeRead` retired at 5.9 |
| | 5.4 | 5.9 | `Nodes`: the list, its version, its rules |
| | 9.4 | 7 | Code 32, `TagRequired` |
| | 11.12a | 5.8, 5.9 | The system group's endpoint; epochs, authentication, sessions and the catalog from the client endpoint's role; compute holding the client endpoint; the endpoint tables of both stages |
| | 11.14 | 5.9 | Seeds, the node list, tags, draining, a tag a node lacks, the place kept for load control |
| | 13, 13.1, 13.4, 13.16 | 5.8, 5.9 | Role kind 1 retired; Join's state, advertised address and tags; `EpochClose`'s `nodes`; `CloseRequest` with partition 0 |
| | 13.23 to 13.26 | 5.8, 5.9 | Bodies of `Session`, `Check`, `Ended`, `Roster` |
| | 13.27, 13.28 | 5.8 | Bodies of `Declare` and `Defaults` |
| `STORAGE.md` | 2, 3.1 | 6.5 | The engine's publish and fetch requests; the settings `binding`, `object_retention_ms` and `cache_extents` |
| | 9.3, 11.3, 12.3 | 6.5 | Record kinds 21 and 22; the checkpoint identity over 58 bytes; the superblock's `extents` reference at offset 176 |
| | 12.5 | 6.5 | Extent records and bindings: the records, the two bindings, the objects' keys and bytes, publication, absent extents, waiting on the object store, retention, the promise with one replica |
| | 14.6 | 6.5 | The one place a slot leaves its replica |
| | 17.1, 17.2, 17.4 | 6.5 | Checkpoint and recovery with the extent leaves and root; repair from the object store |
| | 17.7 | 6.5, 8 | Restore from the object store |
| | 20, 22 | 6.5 | Three decisions; the conformance vectors this text now asks for |
| | 18.1, 19.1, 19.5, 21 | 5.9, 6.5 | The lane's chooser; the object store's faults; the simulator's check; what the binding asks of others |
| `SCHEMA.md` | 10.2, decisions | 5.8 | The compiler runs in the role holding the client endpoint and offers the version to the system group |
| `SCHEMA.md`, `QUERY-GQL-CORE.md`, `QUERY-SPARQL-CORE.md` | one sentence each | 5.9 | The role holding the client endpoint |

## What each change set still writes

- **Stage 5.8.** The golden frames of `Session`, `Check`, `Ended`, `Declare` and `Defaults`. The settings: the system
  group's address in place of the frontend's, the partition count, the bootstrap secret, the epoch bounds and the
  session limits moving to storage. Sections 11.12, 11.13 and 13 brought to the text of 11.12a.
- **Stage 5.9.** Sections 1 to 13 brought to "compute"; `ComputeRead` and the relay rules removed; the golden frames of
  Join, EpochClose and the new commands. The Rust clients: seeds, the list, the swap, tags.
- **Stage 6.5.** The conformance generator and vectors, which lag `STORAGE.md` since this update: kinds 21 and 22,
  the superblock's `extents`, the 58-byte identity, checkpoint step 2, and the objects of section 12.5 (tasks 0.5 to
  0.7 of its change). The object-store interface and its simulated store. Its deltas for `quad-reads`,
  `wire-protocol`, `role-binaries`, `ingest`, `transactions` and `storage-lanes` name the frontend and compute and are
  rebased on stage 5.9.
- **Stage 6.6.** The offer of a compiled version (540) and the fence carried in `EpochClose`, by decisions 1 and 2.
  Its deltas for `client-endpoint`, `metadata-group`, `ugs-catalog`, `wire-protocol`,
  `system-identity`, `term-dictionary` and `schema-changes` name the frontend as the metadata group's host and are
  rebased on stage 5.8.

## Left as it is

- `PROTOCOL.md` says `SECURITY.md` lies beside it; the file is `openspec/notes/SECURITY.md`. Its tiers name three roles.
- The conformance vectors under `specification/conformance/storage/` are those of 2026-11-10 and lag `STORAGE.md`
  until task 0.5 of the tile store's change.
- `QUERY-ULTRA-QL.md` asks, as an open question, what a `subscribe` feed reads from the frontend.
- The consistency review across the specifications and the two open change sets has not been run on this text.
