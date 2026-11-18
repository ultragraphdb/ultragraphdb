## Context

The target is `openspec/designs/UltraGraphDB Distribution Architecture.md` (*One role serves clients and executes*;
*Clients: discovery, tags and load*; *The path*, item 2). The wire is `specification/binary_protocol/PROTOCOL.md`:
section 11.12a states what this stage changes, sections 5.4, 11.14, 13.1, 13.2, 13.4 and 13.26 the list of compute
nodes. The decisions behind that text are K46 to K53 and U2 and U3 of
`openspec/notes/UltraGraphDB Spec Consistency Review 2026-11-12.md`. Where this file and those disagree, those win
and this file is corrected.

The code after stage 5.8. `ultragraphdb_common_frontend` holds the client endpoint: `ClientSession`, a synchronous
state machine per connection, the tenant gate, the link to the system group and the core's view, and `RoleLinks`, by
which a core relays writes to partitions and reads to compute processes in turn. `ultragraphdb_common_compute` holds
the read executor: `ComputeCore`, one core's connections to the partitions, and `ComputeSession`, which runs each
ComputeRead as a task on the core it arrived on, merging the partitions' Entries into Result frames within the
client's `max_frame` and credit, which the frontend passes along. Both processes are stateless.

Stakeholders: the user, who commits after each group and approves each deletion; stages 6.5 and 6.6, rebased on this
change; stage 7, whose query core plugs into the executor this change puts beside the endpoint, in one crate.

## Goals / Non-Goals

**Goals:**

- One role binary, `ultragraphdb_compute`, that serves clients and executes their reads, and nothing named frontend.
- Every read scenario unchanged and green without the relay.
- Several compute nodes on one cluster, found by clients through seeds and the node list, spread by tag.
- The benchmark rerun as the tile store's baseline.

**Non-Goals:**

- Load control: credit to send, signals and the probe are stage 8's (`PROTOCOL.md` section 11.14, *Load*).
- Code 32 and procedures that need a tag (stage 7). Tags choose nodes from this stage; nothing refuses a wrong one
  yet.
- Replication, failover and a session check without a round trip (stage 8; backlog item 13).
- The query core. The executor keeps the quad-pattern reads it has.
- A worker pool for work that outlives a turn. Nothing of this stage does; D3 keeps the seam.

## Decisions

**D1. One crate under the merged role** (decided 2026-11-15). The code of `ultragraphdb_common_frontend` moves into
`ultragraphdb_common_compute` by `git mv` of its files, with its tests, features and test doubles: the connection's
session, the gate, the view, the link to the system group and the links to partitions, beside the executor the crate
holds: the routing of a read by its bound positions, the Scans and Frontier requests, the merge, resume keys. The
session calls the executor inside the crate, by the messages of D3, and the binary `ultragraphdb_compute` links the
one crate. `ultragraphdb_common_frontend` is then empty and is removed with the frontend binary, after approval. One
role has one crate, as storage has `ultragraphdb_common_storage`, and no dependency between an endpoint crate and an
executor crate is left to keep acyclic. *Alternative:* two crates, the frontend's renamed
`ultragraphdb_common_endpoint` beside the executor's. It keeps each crate's reason to change single, and D3's
messages keep that seam inside one crate, where stage 7's query core or an algorithm's worker can still be split
off when one of them needs a crate of its own.

**D2. One connection per core and partition carries writes and reads.** A core's `RoleLink` to a partition, which
carried Bind streams and one-frame writes, also carries the Scans, Frontier requests and Closed of the reads that
core executes. `ComputeCore`'s own connections go. The core that holds a transaction's Bind stream sends its Scans,
so the order between a transaction's writes and reads is kept by one task queue where it was kept by a rule between
two processes. D2 lands after the switch, in group 4: until then the executor keeps the connections `ComputeCore`
holds, so one executor serves the endpoint in process and, until the switch removes it, the compute binary's role
session, with no second kind of link beneath it.

**D3. A read is a task of the endpoint core, and a step is one frame.** The architecture asks for the bound on read
work per turn and for where longer work runs. A read's task gives the core back after it has consumed one Entries or
Neighbours frame and after it has built one Result frame; a frame is at most the role connection's `max_frame`, one
mebibyte, so no read holds a core longer than one frame's merge, and a Commit is relayed between any two frames. The
session hands the executor a read as a message and takes its frames as messages, so an executor on another runtime
can serve the same session later, which is what an algorithm that runs for minutes will need. *Alternative:* a pool
of worker cores for reads now. It adds a hop across cores to every read, the hop this stage removes between
processes, for work nothing yet produces.

**D4. The client's frame and credit apply directly.** A Result frame is built within the `max_frame` of the client's
Hello and sent under the stream's credit, which starts at the Welcome's `initial_credit`; a Credit and a Cancel act
on the read in the turn they are read. The fields that carried both in ComputeRead go with it.

**D5. The node list is the metadata state machine's.** A Join appends a command that adds or changes an entry, and
the group proposes a removal once an entry's connection has been closed for twice `epoch_ms_max` by its own clock
(K46, as amended: the shortest epoch is one millisecond, so a count of closes would drop a live node at every
reconnect). The close record gains
one flag, and the list's version is that close's epoch. A compute node joins once per process, on its first core's
connection, joins again at once on another open connection when that one closes, and every core holds the list its
process read. Compute's Join is built in group 3 (D12); between the switch and group 3 a compute process joins nothing.
*Why not membership by open stream:* a restart of the
group would empty the list, and one version would name two lists.

**D6. Compute is told its address and its tags.** `[compute] advertised_address` and `[compute] tags` are settings of
each compute instance, checked by the config service. A node carries exactly the tags its settings name, and `query`
alone when they name none. One pod is one instance: several compute nodes are several instances, each with its own
file and its own StatefulSet of one replica, as storage has, and a node checks at start that its certificate names
the host it advertises. `compute.role_address`
goes, since nothing dials compute.

**D7. The clients.** `ultragraphdb_client_core` holds the list, its version and the choice of a node: seeds in order,
the list of the highest version swapped whole, a tag per client, serving nodes of that tag taken in turn when a
transaction begins and for a one-frame request, a draining or unlisted node given nothing new, backoff with jitter,
and the return to the seeds. The shells dial. The core is synchronous and already tested over a simulated network,
which gains several servers.

**D8. Several compute nodes from this stage** (decided 2026-11-12): the config service accepts one or more compute
instances, and the acceptance battery runs two on one host. The node list is built by groups 1 and 3 and used by
group 7, so the three stand together; nothing of them waits for stage 8.

**D9. A partition's check of a Drop's catalog version is stage 5.8's** (K26; its design D14). Compute's check stays
as the early refusal, and group 8 sends a Drop through a node one close behind.

**D10. Names and removals.** The role, its settings table, its SCRAM username and its instances are `compute`. The
frontend's binary, image, test crate, chart entries, `FrontendTable` and profiles are removed once their contents
live under compute's names, each at a task that waits for the user's approval. The scenarios of
`ultragraphdb_frontend_tests` move into `ultragraphdb_compute_tests` first, green with the same count.

**D11. Verification.**

- **Before and after.** The read features, fan-out, credit, resume keys, levels and a transaction's own writes, run
  unchanged. A feature is edited only where a step names the relay.
- **Laws, as Hegel property tests** (`tdd-protocol.md`): the node list as a state machine against a model, Join and
  removal and closes in any order, with one version naming one list and a version never falling; Nodes and Roster
  round-tripping and refusing by rule, every list within its bounds fitting a frame; the client's list never going
  back and its choice never naming a draining node; the executor's merge returning the rows of any drawn partitions
  partition by partition, each partition's in its own order (`PROTOCOL.md` section 9.1), whatever the frame boundaries.
- **Two nodes.** Sessions through both, a resend on the other node while the original is on its way, a node drained
  and a node killed, a Drop sent through a node one close behind.
- **The benchmark.** Rerun exactly as `openspec/benches/benches-2026-11-15.md` ran it, the last record of reads and
  writes, since stage 5.8 took none (decided 2026-11-18). The difference from that record holds stage 5.8's Check round
  trip and this stage's removed hop together. It is recorded as the baseline stage 6.5 compares against, so that
  comparison measures the tile store and not the hop.

**D12. The relay goes first, the switch second, the node list third** (decided 2026-11-18; `notes/feasibility.md`).
From stage 5.9 a compute node's Join carries no role address, Members lists storage alone and the role name `frontend`
is refused (`PROTOCOL.md` sections 13.1 and 13.2; config-service). Each of those ends the relay, so none can land while
a frontend binary still relays reads. The groups therefore run 0, 5, 6, 4, 1 and 2, 3, 7, 8, 9:

- **Group 5** moves the endpoint's code beside the executor and executes reads in process. Nothing sends a ComputeRead
  after it.
- **Group 6** is the switch, one commit as stage 5.8's task 7.4 was: the compute binary takes the endpoint under the
  role's one name, and every retired name is refused. From the switch until group 3 a compute process sends no Join,
  since nothing dials it.
- **Group 4** puts a core's reads on its `RoleLink` (D2), once the executor has one caller.
- **Groups 1, 2, 3 and 7** build the node list on two roles: the wire, a node's settings, the list in the system group
  with compute's Join, and the clients.

Every group passes the full `make test`, and the processes of a cluster speak one protocol at every commit.
*Alternative:* the order 1 to 9 under two gates, as stage 5.8 ran: most of the stage would run without a test over the
built binaries, and stage 5.8's first full gate after such a window found the defects of four groups at once.

**D13. What stage 5.8 left is taken where its code changes** (decided 2026-11-18). The mutation survivors of the
endpoint's code are resolved in the change's one mutation run (task 9.1), after group 5's removal of the relay has taken
those of the relay with it. Mutation testing runs once, in group 9, so no group waits on it (decided 2026-11-18).
The findings on compute's `Link` are corner cases of group 4, which removes `Link` and holds `RoleLink` to the same
rules. The reader rule written three times becomes one before group 3 writes a fourth (task 0.7). The two mutants only
real time tells apart take a clock (tasks 3.7 and 6.6). Storage's two findings land in group 3 (task 3.6).

## Risks / Trade-offs

- **A read's CPU now shares a core with commits' relays** → D3's bound; measured in group 9 as the relay delay of a
  commit beside a merge; a worker runtime behind the same seam if the measurement asks for it.
- **A rename across many requirements invites drift** → the deltas were generated from the capabilities as stage 5.8
  leaves them and reviewed for every sentence in which the two roles met; group 0 repeats the check after stage 5.8
  is archived.
- **Clients may see a snapshot go back across nodes** → decided 2026-11-12: a connection's snapshots never go back,
  and a token carries the floor across connections and nodes; group 8 tests both.
- **Compute pods need stable names to advertise** → one StatefulSet of one replica per compute instance, without a
  volume; a Deployment would advertise pod addresses that change with every restart.
- **A draining node cuts what outlives the transaction limit** → it waits at most `transaction_ms`; a read-only
  transaction is cut and read again elsewhere, and a commit in doubt is resolved by its resend.

## Migration Plan

No data migrates. The groups land in the order of D12; the tree builds and `make test` passes after each. The
frontend's files are removed last, after their tests have moved. This change is archived, and stages 6.5 and 6.6 are
rebased, before stage 6.5 starts.

## Resolved before implementation

Decided on 2026-11-12 (`openspec/notes/Stage 5.8 and 5.9 Open Questions.md`):

- Several compute nodes run from this stage (A2), each a compute instance with its own settings file and StatefulSet
  (A6), whose certificate names the host it advertises (A7).
- An entry leaves the node list once its connection has been closed for twice `epoch_ms_max` (A5), and a node whose
  joining connection closes joins again on another (B6).
- A draining node waits at most `transaction_ms` (A8). A node serves exactly the tags its settings name (A9).
- A session serves through any node; a transaction and a prepared statement belong to their connection (B9).
- One frame per step holds for every entry of the executor (B8). A partition silent past a request's deadline and
  twice `scan_stall_ms` is hung, by the rule `PROTOCOL.md` section 11.12 already holds (B7).
- A read sharing a core with commits is acceptable when a commit relayed beside a merge waits at most one frame's
  merge and its latency under a mixed load stays within the spread of its latency without reads; outside it the
  executor moves to a runtime of its own behind D3's seam (B11).
- Every delta is read in full against the capabilities as stage 5.8's change is archived, before group 1 starts
  (B12).
