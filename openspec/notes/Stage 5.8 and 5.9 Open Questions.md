# Stages 5.8 and 5.9: open questions before implementation

Nov 12, 2026 · every open question and unresolved issue of `openspec/changes/stage-5-8-system-group` and
`openspec/changes/stage-5-9-compute-role`, each with a recommendation decided by the principles of `AGENTS.md` in their
order: correctness, speed, scale, developer experience. **Decided 2026-11-12: the user accepted every recommendation,**
and each is written where its last line says. One recommendation changed on the way: B7.

The list was compiled by the author of both change sets, without an independent read. Four facts were checked against
the text or by running code, and are marked *verified*.

## A. Decisions that change what a client or an operator is promised

**A1. May two connections see snapshots that go back in time?** (review U2; both stages)
From stage 5.8 each endpoint core follows the closes on its own subscription, so two Begins without a token may answer
at epoch `e` and then `e − 1`. Today one owner answers every Begin.
*Options:* (a) state the weaker rule: monotonic per connection, a token carries the floor across connections; (b) ask
the system group at every Begin; (c) a read-only client carries the epoch of its last result as a floor, a new field.
*Recommendation:* (a). The stated guarantee, serializable at a closed epoch, holds. (b) puts a round trip on every
transaction's start. (c) can be added later without breaking (a).
*Lands in:* `PROTOCOL.md` 11.11 and 11.12a; client-endpoint, "An endpoint core keeps a view of what the closes
announce".

**A2. From which stage may a cluster run several compute nodes?** (review U3; stage 5.9)
`PROTOCOL.md` 11.14 says stage 5.9; the architecture said stage 8.
*Recommendation:* stage 5.9. The rules for several nodes, the mark, the node list, the Drop's check, are then tested
before stages 6.5 to 7 build on them, and the Rust clients are edited once.
*If stage 8 instead:* groups 3 and 7 of stage 5.9 and the Nodes and Roster parts of its group 1 move to stage 8;
design D8's claim that group 7 alone is separable is wrong as written and is corrected either way.
*Lands in:* stage 5.9 `design.md` D8 and `tasks.md`.

**A3. How long does a mark outlive its connection?** (stage 5.8; corrects review K2) *verified*
K2 keeps a closed connection's marks for `transaction_ms`, as "the longest a request no endpoint speaks for can still
reach a commit step". The text bounds two waits separately: a one-frame write may wait up to `transaction_ms` in the
endpoint's queue before it leaves (`PROTOCOL.md` 11.12, *Frames on their way*), and a parked frame at a partition is
released "by a home's reply or by its transaction's limit" (*Parked requests*). A request checked just before the
connection closed can therefore reach a commit step almost twice `transaction_ms` later. The partition's bound for a
parked one-frame write or Ingest share, which has no transaction, is implied and not stated.
*Recommendation:* keep a closed connection's marks for twice `transaction_ms`, and make the system group's quiet time
after it begins to listen the same. State in section 11.12 that a partition answers a parked one-frame write or Ingest
share with code 4 once `transaction_ms` has passed since it arrived. Cost: after an endpoint's connection is lost, its
sessions with a request in flight answer 14 for up to 20 s. Nothing is paid while connections stand.
*Lands in:* `PROTOCOL.md` 11.12 and 11.12a; system-group, "A mark outlives its connection by the transaction limit".

**A4. Is the quiet time after a restart of the system group acceptable?** (stage 5.8)
A restart loses the marks, so for the quiet time the group evicts nothing and answers every resend's check with 14.
New commits are not affected. With one replica a restart is rare.
*Recommendation:* accept it for stages 5.8 to 7. At stage 8 a change of the group's primary would trigger it, so
stage 8 decides whether marks join the replicated state, together with backlog item 13.
*Lands in:* no change; recorded in stage 5.8 `design.md`, *Risks*.

**A5. How does a compute node leave the node list?** (stage 5.9; corrects review K46) *verified*
K46 removes an entry once its connection has been closed for two closes. `epoch_ms_min` may be 1 ms
(sequencer, "The sequencer closes epochs at its own wall-clock time"), so two closes can pass in 2 ms and every
reconnect would remove and re-add a live node.
*Recommendation:* the system group proposes the removal once the entry's connection has been closed for twice
`epoch_ms_max` by its own clock, the bound it already uses for a silent subscriber. The removal is still a logged
command, so the list stays a function of the log. A dead node is listed for at most that long, and clients skip a
node they cannot reach meanwhile.
*Lands in:* `PROTOCOL.md` 11.14; node-discovery, "The node list is a function of the log".

**A6. One compute pod is one instance.** (stage 5.9) *verified*
The chart runs one StatefulSet of one replica per instance, because an instance takes its name and its settings file
from its pod's environment (`deploy/helm/ultragraphdb/templates/storage.yaml`). The stage 5.9 delta says compute "MAY
run several replicas", which that model cannot give each pod its own advertised address.
*Recommendation:* several compute nodes are several compute instances, each with its own file and its own StatefulSet
of one replica, as storage has. Scaling out is adding an instance and a settings revision. The config service then
checks every advertised address before any node starts.
*Lands in:* kubernetes-deployment and config-service deltas of stage 5.9.

**A7. What must a compute node's certificate name?** (stage 5.9)
A client dials a seed and then advertised addresses, and verifies each node's certificate against the host it
dialled. Nothing says a node's certificate must name its advertised host.
*Recommendation:* a compute node checks at start that its certificate names the host of its
`compute.advertised_address`, and exits with code 78 otherwise. A list entry no client can verify is then found at
start and not at a client's first connect.
*Lands in:* node-discovery; client-endpoint, "The client endpoint speaks TLS 1.3 when configured".

**A8. How long does a draining node wait?** (stage 5.9)
The delta says an open transaction runs to its end and a read-only one ends when the node stops. No bound is stated.
*Recommendation:* a draining node waits until its read-write transactions have ended, at most `transaction_ms`, which
bounds them already, and then stops. A read-only transaction is cut then and its client reads again on another node. A
commit cut by the stop is in doubt and is resolved by its resend on another node, which the mark answers correctly.
No new setting.
*Lands in:* node-discovery, "A draining node is announced before it stops".

**A9. Does a node tagged `gpu` or `high_memory` also serve `query` clients?** (stage 5.9 design, open question)
*Recommendation:* a node serves exactly the tags its settings name, and carries `query` only when it names none. The
operator sees in one place what a node serves, and a GPU node is not filled with ordinary queries by default.
*Lands in:* node-discovery, "A compute node joins with what clients need of it".

## B. Gaps with a clear answer

**B1. A property test cannot return a result.** (both stages) *verified by a build*
`#[hegel::test]` refuses a return type at compile time: "the test body's assertions are the property". The
repository's rule has tests return `TestResult`.
*Recommendation:* allow the exception for `#[hegel::test]` functions alone. They assert, use no `unwrap`, `expect` or
`panic!`, and fold fallible calls into `Option` or `Result` values compared in an assertion, as the `Version`
properties do. This needs the user's consent, as a rule exception does.
*Lands in:* `tdd-protocol.md`; `AGENTS.md`.

**B2. May a storage instance host the system group and no partition?** (stage 5.8 design, open question)
*Recommendation:* refused by the config service until stage 8. One shape is built and tested; stage 8 decides
whether dedicated hosts are wanted, with replication.
*Lands in:* config-service, the System rule.

**B3. May `system.data_dir` and `storage.data_dir` share a directory?** (stage 5.8)
*Recommendation:* the config service refuses a `system.data_dir` equal to, inside or containing the `data_dir` of an
instance on the same host, by its Data rule, so no log is ever found under another's path.
*Lands in:* config-service, the Data rule.

**B4. What does a storage process that does not host the group do while the group is unreachable?** (stage 5.8)
The delta states its exit after `join_deadline_ms` and nothing before it.
*Recommendation:* state what the text already gives a partition without closes: it goes on committing in its open
epoch, refuses with code 5 once the epoch is full and its close request has gone unanswered for `close_request_ms`,
and its snapshot reads wait within their deadlines.
*Lands in:* system-group, "A lost system group is waited for until the deadline".

**B5. Describe reads two things at two log positions.** (stage 5.8)
The frontend asks for the version to report and for the default-entry records in two requests. A record added
between them can appear in one Describe and not in the version's position.
*Recommendation:* allow it and say so. A default-entry record is only ever added, creates no version and stales no
statement, so such a Describe equals one sent a moment later. A single command would add a body for no gain.
*Lands in:* `PROTOCOL.md` 11.12a.

**B6. Which connection does a node's entry belong to?** (stage 5.9)
A compute process joins once but holds one connection per core. If the joining connection closes while the others
stay open, the entry would be removed though the node serves.
*Recommendation:* when its joining connection closes, the node joins again at once on another open connection, which
moves the entry there. With A5's grace no client sees a change.
*Lands in:* node-discovery, "A compute node joins with what clients need of it".

**B7. Who times a partition that stops answering a Scan?** (stage 5.9) *verified after the list was written* The removed
requirement timed a hung compute process with `scan_stall_ms`. After the merge the same silence can come from a
partition, and I have not confirmed that another requirement times it. *Resolved:* `PROTOCOL.md` section 11.12,
*Errors*, already holds the rule: a partition that leaves a request of compute's unanswered past its deadline and twice
`scan_stall_ms` beyond is hung, and compute closes the connection. No capability stated it, so stage 5.9 adds it to
quad-reads as it stands, in place of the rule first recommended here. *Lands in:* quad-reads, "Storage's read kernel
answers what compute scans".

**B8. The step bound names reads alone.** (stage 5.9)
*Recommendation:* "one frame per step" holds for every entry of the executor: a merge over several partitions, a
reverse read, and the query core's two lookups, by value index and by reverse count.
*Lands in:* quad-reads, "A read's step is bounded by one frame".

**B9. What a client keeps when it moves between nodes.** (stage 5.9)
*Recommendation:* state three facts. A session is the cluster's and works through any node. A transaction belongs to
the connection it began on. A prepared statement belongs to its connection, so a client prepares again on a new node.
*Lands in:* node-discovery, "A client is started with seeds and picks nodes by tag".

**B10. What decides that the Check's round trip is acceptable?** (stage 5.8)
Success criterion 5 says the round trip is measured and names no bound.
*Recommendation:* two bounds, both from the stage's own measurements. Throughput at saturation stays within the
spread of the baseline, since checks are batched per turn. A commit's latency rises by at most one hop between the
frontend and the system group as measured in the same run. A larger gap is a software defect and is root-caused.
*Lands in:* stage 5.8 `proposal.md` and task 9.3.

**B11. What decides that a read sharing a core with commits is acceptable?** (stage 5.9)
*Recommendation:* a commit relayed beside a merge waits at most one frame's merge, and a commit's latency under a
mixed load stays within the spread of its latency without reads. Outside it, the executor moves to a runtime of its
own behind the seam design D3 keeps.
*Lands in:* stage 5.9 task 8.3.

**B12. The renamed requirements have not been read in full.** (stage 5.9)
Sixty-four requirements were renamed from "the frontend" to "compute" by a script, with hand edits where the two
roles met.
*Recommendation:* task 0.1 becomes a gate: every delta is read in full against the capabilities as stage 5.8's change
is archived, before group 1 starts.
*Lands in:* stage 5.9 `tasks.md`.

**B13. The rewritten protocol sections are not reviewed.** (stage 5.8)
Task 0.2 rewrites sections 3 to 13 of `PROTOCOL.md` for the system group.
*Recommendation:* the rewritten sections are checked against section 11.12a before group 1 starts, by the method of
the review of this day, and the golden frames are regenerated only after it.
*Lands in:* stage 5.8 `tasks.md`.

## C. Accepted as written, to confirm

- **C1. The frontend's read of Members** (stage 5.8, D9): at start, when a role connection closes, and before it
  refuses a read. A compute process that joins later is found at the next such event. The rule lives one stage.
- **C2. The authentication throttle** (stage 5.8, D8) stays in the frontend, in its first core, asked by message as
  the owner was. With several compute nodes a source gets one allowance per node (S3).
- **C3. The hosting storage instance** is named by a `[system]` table, exactly one per target until stage 8 (D2).
- **C4. A partition of the hosting process dials the group's address** like any other (D4). One path is tested.
- **C5. `sessions_max` is the operator's to size** to the compute nodes' connections. No check compares them, since a
  reading connection needs no session.
- **C6. The benchmark of stage 5.9 runs one compute node,** so it compares with stage 5.8. A run with two nodes is
  recorded apart and gates nothing.
- **C7. Compute nodes are told their address and tags by settings** (stage 5.9, D6), not discovered.

## D. Later, by limitation

- **D1. A tenant created at stage 7** must be announced with a close, as a catalog version is, or a compute node's
  view does not learn of it. Stage 7's change.
- **D2. A principal's revocation** reaches no endpoint before stage 7, whose commands carry it with the closes.
- **D3. The session check without a round trip** is backlog item 13, answered before stage 8.
- **D4. Load control,** credit to send and the signals, is stage 8's.
- **D5. Whether the config service folds into the system group,** and whether the two state machines share one log,
  are stage 8's.
