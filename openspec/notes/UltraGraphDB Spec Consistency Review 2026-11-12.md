# UltraGraphDB Specification Consistency Review, 2026-11-12

Nov 12, 2026 · review of the text the specification update of this day wrote
(`UltraGraphDB Specification Update.md`, decisions S1 to S24) against the rest of `specification/`, the architecture
(`../designs/UltraGraphDB Distribution Architecture.md`) and the two open change sets, before the change sets of stages
5.8 and 5.9 are derived and those of stages 6.5 and 6.6 are rebased. It follows the review of 2026-11-10
(`UltraGraphDB Spec Consistency Review.md`, C1 to C48) in method and form.

## Method

Five reviewers each read through one concern: sessions, authentication and epochs across the roles (R1); the catalog's
path (R2); client discovery, the node list and tags (R3); the extent records and the object binding (R4); references,
stage labels, terminology and the slot rule (R5). Each concern was checked with the elements of thought against the
intellectual standards, as on 2026-11-10, and each finding quotes both sides. The reviewers reported 115 findings,
which collapse to the 62 decisions below and a set of mechanical corrections. Six findings were re-read against the
text and the code before this note was written: the Recall's check (K1), the stateless subscriber (K3), the records of
the extents a checkpoint's own step closes (K30), recovery's sources (K31), the token check (K4) and the frontend's
Join (mechanical). All six hold. Two findings were reported independently by three reviewers (K1, K3).

A finding is **mechanical** when the documents already decide the matter and some text says otherwise. Every other
finding needs a rule, decided by the principles of `AGENTS.md` in their order: correctness, speed, scale, developer
experience. Severity: **B** an implementation built from the documents is wrong or two implementations diverge; **A**
two readings exist; **W** wording.

Status: *applied* means the specification holds the rule as of this note; *for 5.9*, *for 6.5* and *for 6.6* mean the
rule is decided here and written by that stage's change set in its specification step, which this note lists at its
end.

## Decisions for the user

Four findings change what a client or an operator is promised. Each has a default written into the specification so
that the text is consistent, and each is the user's to confirm or overturn.

**U1 (R4-10). Does the grid keep its whole size on the local drive under the object binding?** `STORAGE.md` section
12.1 creates the grid whole with `fallocate` and never changes its size, and section 12.5 says everything else holds
for both bindings. The architecture says the object store bounds the data and the local drive bounds the cache. Both
cannot hold. With the grid allocated whole, a partition's data is bounded by its local drive and punching only frees
blocks the grid may claim back. With a sparse grid a node's local use has no bound unless one is added: the cache
setting counts published cold extents alone.
*Recommendation:* a sparse grid under `object`, with one per-node bound on all extents present, a fetch that waits for
an eviction at the bound, and apply stalling with Busy when the unpublished backlog reaches it. That is what makes the
object binding worth having, and every wait is a stall that changes no decision. *Default written:* none; section 12.1
stands, and the architecture's row is marked open.

**U2 (R1-10). May two connections see snapshots that go back in time?** Today one owner answers every Begin, so the
snapshot epoch never falls from one Begin to the next, on any connection. From stage 5.8 each endpoint core follows
the closes on its own subscription, and from stage 5.9 each compute node does. Two Begins without a token may then
answer at epoch `e` and then `e − 1`.
*Recommendation:* state the weaker rule: a connection's snapshots never go back, and across connections only a token
carries the floor. It costs nothing, and the guarantee of the architecture ("serializable at a closed epoch") holds.
The alternatives are a round trip to the system group on every Begin, or a read-only client carrying the epoch of its
last result as a floor, which needs a field. *Default written:* the weaker rule, in section 11.12a.

**U3 (R5-3). From which stage may a cluster run several compute nodes?** `PROTOCOL.md` section 11.14 says from stage
5.9. The architecture's status table says stage 8, then stage 10. The answer decides whether stage 5.9 builds and tests
the node list, the Rust clients' swap, the rules for one session at two nodes and the Drop's catalog check, or whether
those wait for stage 8.
*Recommendation:* from stage 5.9. The rules for several nodes are then exercised before stages 6.5 to 7 build on them,
and the client is edited once, with the merge. *Default written:* stage 5.9, and the architecture's row corrected.

**U4 (R4-6, R4-15). Retention of published checkpoints counts checkpoints, not time.** S15 retained a published
checkpoint by the sequencer's logged close times. The review found no source for those times: a round carries none, a
superblock holds no epoch, and a restarted replica holds no close older than its log. A retention position stamped by
the primary would be a new field in every round.
*Recommendation:* retain the latest `object_checkpoints` published checkpoint records, a setting equal across the
cluster. It is a function of the published positions alone and needs nothing the replica lacks. A reader that must
hold a source for minutes, which exists from stage 10, is then counted by the stage that adds it. This replaces S15.
*Default written:* the count, in section 12.5.

## Decisions

### Sessions, the owner check and the mark

**K1 (B; R1-1, R5-1, R2-18). A resend's check could never answer "in flight".** Section 13.24 allowed outcome 14 for
kind 2 alone. From stage 5.8 the mark lives in the system group, so only the Check can tell an endpoint that the
original is still on its way. A resend on another core or node passed, its Recall answered 18 before the original
reached its partition, the client ran the transaction again, and the original then committed: one transaction twice.
The code answers 14 today. *Decision:* a check of kind 1 naming a marked session answers 14 and marks nothing.
*Applied.*

**K2 (B; R1-2). A mark ended with its connection, which section 11.5 does not list as an end of "in flight".** The
connection to the system group and the one carrying the request fail apart. *Decision:* a mark ends by an Ended. When
the connection that began it closes, the mark is kept for `transaction_ms`, the longest a request no endpoint speaks
for can still reach a commit step, and then ends. An Ended ends a mark begun on its own connection or on one that has
closed. For `transaction_ms` after the system group begins to listen, when it holds no marks, it evicts no session and
answers every check of kind 1 with 14. An endpoint whose connection to the group closed refuses what needs a Check
with 17, and cancels the Bind stream of a session commit whose Check went unanswered. No path costs anything while
connections stand. *Applied.*

**K3 (B; R1-3, R2-14, R3-13). A subscriber that holds nothing durable has no `from`.** The sequencer keeps only its
last closes, so `from` 0 is refused on any cluster past that count. *Decision:* Subscribe gains `latest`, which the
client endpoint's role alone sends: the stream begins with the latest durable close. A subscription ended as trimmed
is opened again the same way. *Applied.*

**K4 (B; R1-4). The token check of section 4.8 refuses valid tokens once the checker follows closes by
subscription.** A partition may have received a close the endpoint has not. *Decision:* a token above the endpoint's
open epoch is not refused. A Begin naming it is held until the close before the token's epoch arrives, within its
deadline, then code 24. One-frame writes go through, since the partition already holds a commit until that close.
*Applied.*

**K5 (B; R1-5). Nothing stated what the client endpoint's role answers while the system group cannot be reached.**
*Decision:* an Authenticate is answered Error 17 on stream 0 and the connection closes, counted as no failure, so an
outage bans no source. A Register and a request through a session answer 17. A Begin, a read and a commit through no
session go on from the view. The role exits with code 69 once the group is gone for `join_deadline_ms`. *Applied.*

**K6 (B; R1-6). The registry question after the Recalls had no command.** *Decision:* once every partition answered
18, the endpoint sends a second Check of kind 1 and answers 12 on outcome 12, else 18. *Applied.*

**K7 (A; R1-7).** A Check reads the registry as every registration decided before it left it, durable or not, and a
mark exempts its session from evictions decided after it. *Applied.*

**K8 (A; R1-8).** One check per session per frame; a further request through the session waits for the next frame.
Within a turn the Ended frame is sent before the Check frame. *Applied.*

**K9 (A; R1-13, R1-15, R1-20).** A check's outcomes are decided in the order 12, 21, 14. Ingest is named among kind 2,
and its Ended follows its last share's answer. A Session naming a principal the state machine does not hold is refused
with code 7. *Applied.*

**K10 (A; R1-16). Session's repeat could register again an identity whose eviction had not reached the partitions.**
*Decision:* no repeat. A held identity is refused whoever holds it. A Session whose answer is lost answers the
client's Register with 17, and the client registers again under a new identity, as section 11.5 has it. *Applied.*

**K11 (A; R1-12, R1-11).** An Ended's stream ends with it, and a refused Ended is dropped unanswered, as a
CloseRequest is. A role connection's role is its SCRAM username. A CloseRequest with `partition` 0 from `storage` is
dropped unanswered. *Applied.*

**K12 (A; R1-17).** The final Authenticate's `expires` is 0 for SCRAM. A principal's change reaches no endpoint before
stage 7, whose administration commands carry revocations with the closes, as evictions travel. *Applied as a
statement; the mechanism is stage 7's.*

**K13 (W; R1-19).** `sessions_max` bounds the cluster's sessions and is sized to the compute nodes' connections.
*Applied.*

### The system group's endpoint

**K14 (A; own finding in deriving stage 5.8). A partition of the hosting process dials the group as any other
does.** The update wrote an in-process message, after the rule for partitions of one process. No partition asks the
group anything per commit, so one path serves and is tested once. *Applied.*

**K15 (A; own finding). Each endpoint core holds its own connection to the group.** The update's stage 5.9 text gave
the process one. A funnel across cores on every commit is a hop the thread-per-core design avoids. *Applied.*

**K16 (A; own finding, noted by R1). When the frontend reads Members again.** At its start until every partition of
the placement is listed, when one of its role connections closes, and before it refuses a read for want of a compute
process. The rule lives one stage. *Applied.*

### The catalog

**K17 (B; R2-1). No command told the client endpoint which version Describe reports.** Whether a version is being
checked or was withdrawn is the metadata state machine's state, and Catalog's reply cannot tell such a version from
one that stands. *Decision:* a Catalog request with `version` 0 and `epoch` `e` asks for the version a client may read
at closed epoch `e`: the latest in effect at `e`, leaving out one being checked or withdrawn. With `epoch` 0 it stays
the tenant's current version, pending or not. One round trip per Describe, off every hot path. *Applied.*

**K18 (A; R2-4). The tenant's current version at the client endpoint.** It is the highest version the process has
learned: one Catalog read when a tenant is first used, each close's announcements, and each Declare reply. It is never
asked per request, and nothing is decided from it: the system group checks `built_on`. A tenant with no version
answers Catalog with code 7, `detail` 0. *Applied.*

**K19 (A; R2-6). "The format alone" contradicted what the system group is told to build.** *Decision:* the state
machine and its log treat a version's bytes as opaque. The host, before it appends, links the catalog's model with no
parser of UGS or query text: the decoder, the encoder, the comparison of two versions, and the application of
declaration records and drops with their refusals. This restates S24. *Applied.*

**K20 (A; R2-7).** A Declare is one frame. A Write that takes a transaction's declarations past what one Declare frame
holds is refused with code 6, `detail` the bound, ending the transaction; larger schemas come by Install. *Applied.*

**K21 (A; R2-8).** A Declare for a Drop of kind 4 removes the tenant's default-entry records in the same command as
the version, or alone when no version is built. *Applied.*

**K22 (A; R2-9).** A Declare of kind 1 is never sent again: its closed connection answers the client's Commit 17, in
doubt, and Describe decides. Kind 2 may be sent again. A Cancel on a Declare's stream changes nothing. *Applied.*

**K23 (A; R2-15, R2-16, R2-11, R2-10, R5-14).** Declare's unnamed cases are named: the term table empty for kind 1, a
drop term of another kind, an unknown tenant, a kind 2 reply of `version` 0 equal to its request; the requester sends
Declare for Drops of kinds 1 and 4 alone. Each Defaults frame holds a whole term table, the order runs across frames,
and a changed header is refused. Defaults spends credit in section 11.3, and its row answers "or Error". *Applied.*

**K24 (B; R2-2). The fence, as written, could not follow section 8.4.** It must be in force before the Install's
check, which precedes the offer, and one compute node knows only its own commits in flight. *Decision, within the
user's decision of this day:* the fence is a record of the system group, asked for by a role command naming the tenant
and the base version, refused when the base is stale or a fence is held. It is durable, named in the next close `k`,
and logged by each partition in its first round after `k`, as evictions are. The Install reads each partition at the
closed epoch `k + 1`. The fence is lifted at the version's effective epoch, or by a close naming its release, when the
Install is refused or the asking connection closes with no offer committed. A partition refuses a fenced tenant's
Commit, one-frame write, Ingest share and Drop at its commit step. *Applied as the rule in section 11.12a; the bodies
are for 6.6.*

**K25 (A; R2-5). What the system group checks of an offered version.** *Decision:* the four counters are fields of
every version, a Drop's and a write-path version's included, and a tenant with none starts from the initial values, so
compiling is a function of the sources and the base version. Before it commits, the system group compares the offered
version with the base: the tenant, `version` one above the base's, counters at or above the base's, retained numbers
unchanged, new ones taken from the base's counters in entry order. *Applied in section 11.12a and `SCHEMA.md`; the
catalog's text is for 6.6.*

**K26 (B; R2-3). A Drop's `catalog` was checked at the client endpoint alone.** With several nodes a node one close
behind passes a Drop its partition commits under a later version. *Decision:* the partition refuses a Drop of kind 3
or 4 whose `catalog` is not the version in effect at its round's epoch, as it refuses an Ingest share. *For 5.9; the
rule is stated in section 11.12a.*

**K27 (A; R2-13). No client-facing reply carries the catalog version's number**, which Drop, Ingest and Install must
name. *Decision:* Describe's header gains `version`, the version K17 selects. *For 6.6, whose group 1 built these
bodies.*

**K28 (A; R2-12, R5-10, R5-11).** `SCHEMA.md` named stage 5.8 for where an Install compiles; it is stage 6.6, in
section 10.1, and the paragraph placing the compiler beside the metadata group goes. *Applied.*

### The extent records

**K30 (B; R4-1, R5-18). The records of the extents a checkpoint's own step 2 closes were lost by a crash.** Replay
never places a checkpoint's own tiles again. *Decision:* recovery recomputes them. The extents the free set marks that
hold live blocks and have no record in the leaves are the ones step 2 closed; every tile in them is referenced by the
checkpoint, so the record is rebuilt from tiles each verified by its reference, the padding between them checked
zero. *Applied.*

**K31 (B; R4-2).** Recovery's step 4 takes the extent root and leaves as a source of live blocks and of the record
table, and drops the records of extents it finds free. Without it an extent holding only a kept extent leaf was reused.
*Applied.*

**K32 (A; R4-7).** The leaves hold the record table as it stands when step 2 begins, closed and released extents
alike. A leaf may therefore hold the record of an extent no object exists for. *Applied.*

**K33 (A; R4-18). No durable state held an extent's stream**, which decides whether a node may punch it.
*Decision:* the record carries the stream in the two high bits of its placed blocks. *Applied.*

**K34 (A; R4-20).** The reserve's term for the extent leaves is computed from the records held and the extents the
drain can close, not from the previous checkpoint's size. *Applied.*

### The object binding

**K35 (B; R4-4). A checkpoint trimmed the log before it was published.** A grid lost with the log kept then lost the
rounds between the published and the local checkpoint, against section 17.7's promise. *Decision:* under `object` the
log is trimmed through the latest published checkpoint's position at most. During an outage the log grows, and at
its bound admission stops with code 5. *Applied.*

**K36 (B; R4-5, R4-8). Publication checked nothing it uploaded, and its set was sampled at no stated moment.**
*Decision:* closed extents are published as they close and after each checkpoint; a checkpoint's own extents and its
record follow. Publication hashes what it uploads and writes no object that fails its key. An extent freed or failing
before its upload completes abandons that checkpoint's publication, and the next checkpoint's takes its place. At open
a node takes what is published from the latest record under the partition's prefix and offers the rest again, which
the conditional write makes idempotent. *Applied.*

**K37 (B; R4-3). A lost grid and log could not be told from a new partition.** A node reopening with an empty
directory would have published a second history under the same prefix. *Decision:* under `object` a new grid first
lists the partition's checkpoint records. If one exists, the partition serves nothing until the operator restores or
discards the published history. *Applied.*

**K38 (B; R4-6). Retention had no executor, no key source and no rule that survives an extent number's reuse.**
*Decision:* an object `(extent, checksum)` is deleted when no retained checkpoint's leaves or record hold that pair.
It is executed when a record expires (U4): the node fetches the expiring record's extent root and leaves, subtracts
the oldest retained record's set, deletes the extents and then the record, last. The expiring record is the redo list.
Objects no record names, left by an abandoned publication or a restore, are removed by a paced listing. *Applied.*

**K39 (A; R4-9). A scrub would have fetched every absent extent each cycle.** *Decision:* a scrub does not fetch. A
tile whose blocks read all zero in a published extent is absent and skipped; any other failure in a published extent
is repaired by fetch. *Applied.*

**K40 (A; R4-13, R5-13).** The fault model's object-store faults get their responses: no answer, wait; a node punches
only after a read of the object has confirmed it; a missing or differing object is replaced by a node holding
verified bytes, else the replica stops naming the extent. A refused punch of a published extent leaves it present.
*Applied.*

**K41 (A; R4-14).** A checkpoint record is little-endian, ends with an XXH3-64 over the whole object, and has named
refusals. *Applied.*

**K42 (A; R4-16, R4-17).** The operator's command is required exactly when the log does not begin at the checkpoint's
position plus one; a restore continues from the latest record. The open repairs a failing checkpoint tile from the
record at its own position before it refuses. The object store's interface lists a prefix, fetches a record and
deletes, beside the three requests it had. *Applied.*

**K43 (A; R4-12, R5-12).** Code 5 gains `detail` 3, the object store unreachable or the log at its bound for want of a
publication; code 17 covers a read of an absent extent that cannot be fetched; `object_fetch_ms` defines "cannot be
reached"; a parked read answers whichever of its deadline and code 17 comes first. *Applied.*

**K44 (A; R4-11, R5-22).** The binding's settings get a table: `binding`, the store's endpoint and bucket and
`object_checkpoints` equal across the cluster and fixed; `cache_extents` and `object_fetch_ms` per node. None enters
the derivation version. *Applied.*

### Slots

**K45 (A; R5-4, R5-5, R5-15). The slot rule forbade what the store does.** Read strictly it ruled out publication,
repair, state sync and a bucket group's move, all of which carry tiles. *Decision:* the rule names what may hold a
slot. A slot is written only inside the tiles of its bucket group. Those tiles travel as bytes to peer replicas, the
object store, a move's destination and, for a projection, compute. No round, log record, answer, resume key or state
that compute writes names one. The stream to compute is specified with stage 8's tile fetch, its source a checkpoint,
and nothing streams before it. A slot pair appears in a fence and never in a resume key. *Applied.*

### Finding compute nodes

**K46 (B; R3-2). Membership by open stream emptied the list whenever the system group restarted.** *Decision,
replacing S7:* the node list is logged state of the metadata state machine. A Join adds or changes an entry. An entry
leaves by a logged removal, proposed once its connection has been closed for two closes, counted from the later of the
connection's close and the group's start. One version names one list, and no heartbeat is added. *Applied.*

**K47 (B; R3-3).** A compute node sends Join, then Subscribe, then Roster. An entry belongs to the connection of its
latest Join and enters the list at the first close after it. A Join for an address another connection holds replaces
it. A draining node stops no earlier than after it has sent its clients a Nodes naming it draining. *Applied.*

**K48 (B; R3-1, R3-5, R5-8).** A Roster reply may hold no node. A node sends Nodes once it holds a list with a node. A
client never waits for Nodes: until a list arrives it uses the connection it has. *Applied.*

**K49 (B; R3-4, R5-6).** From stage 5.9 Members holds storage members alone; Roster is the one list of compute nodes.
*Applied.*

**K50 (A; R3-6).** Nodes names the sender's own entry by its index, or `ABSENT`, so a client knows its connection's
tags and state without a second connection. *Applied.*

**K51 (A; R3-8, R3-14, R5-17).** An advertised address is `tcp://host:port` with a host of at most 253 bytes, an IPv6
literal in brackets, checked at Join. A list holds at most 1,024 nodes and a cluster at most 256 distinct tags, a Join
past either refused with code 15, so every list the group can hold fits one frame. *Applied.*

**K52 (A; R3-7, R3-11, R3-16, R3-17, R3-24).** The client's rules are stated: no new transaction and no one-frame
request on a node named draining or named no longer, an open transaction running to its end; a listed node that cannot
be reached is skipped and tried again with backoff; a client with no reachable node returns to its seeds; new nodes are
dialled after a random delay; Roster is read only for a close above the version held. The system group ends a
subscription whose frames go unacknowledged for twice `epoch_ms_max`. *Applied.*

**K53 (A; R3-15).** Code 32 names the tag first in its message, covers Prepare and Execute, leaves the transaction
open, and binds every driver to hand the refusal to its caller. *Applied.*

## Mechanical corrections

Applied unless marked.

- `PROTOCOL.md` section 3: rows 519 and 520 read "from stage 5.8"; the refusal before a stage is `UnknownCommand` for
  27 and 534 to 539 alone, and `CommandNotServed` for Challenge and Verify; the frontend named as a sender of Members,
  Placement, Catalog and Subscribe (R1-9, R5-2, R1-21).
- Section 11.12a: the frontend out of Join's row (R1-11, R2-17, R3-18, R5-7); "the system group" for the group and
  "its host" for the process (R5-20); `SCHEMA.md` section 10.1 cited (R5-11).
- Sections 13.1 and 13.4: the stage 5.9 order lines restated whole (R1-14, R3-9, R3-10).
- Section 5.4: `flags` 0; the addresses' count equals the nodes' (R3-19, R3-20). The preamble's reading rule names
  every paragraph beginning "From stage" (R3-21).
- `STORAGE.md`: the lists that named the manifest and the free set alone (sections 16.5, 17.2, 11.3, 17.7; R4-21);
  section 12.5's binding row (R4-19); "never punched by the node, and absent after a restore until read" (R5-21).
- The architecture: the node list in a `Nodes` frame and not the Welcome, twice (R3-12, R5-9); "node list" for "member
  list", without an identity field (R3-22, R5-19); the list's version and tags as stage 5.9's work (R5-16); commits
  through no session going on while the group changes its primary (R1-18); the blind spot naming a roadmap line that
  no longer exists (R5-24); retention by count (U4).
- `openspec/notes/backlog.md` item 13 names stage 5.8 as where the round trip begins (R5-23).
- `ultragraphdb_protocol_wire/tests/ugs_steps.rs` and the feasibility note say stage 6.6 (R5-26).
- *Not applied:* `AGENTS.md` line 22 lists four principles and says "All three matter" (R5-25); the sentence is the
  user's. `PROTOCOL.md` Contents item 6 omits three bodies of section 6, and section 13.13 omits kinds 21 and 22
  (R5-27); section 4.7 has no row for Nodes and Roster (R3-19); sections 5.2 and 9.2 do not name Defaults among the
  frames that spend credit (R5-14). These go with task 0.2 of stage 5.8.

## What each change set writes in its specification step

- **Stage 5.8** (`openspec/changes/stage-5-8-system-group`, derived with this note): `PROTOCOL.md` brought to its
  text (task 0.2), with the corrections not applied above.
- **Stage 5.9:** K26's check at the partition; the golden frames of Nodes, Roster, Join and EpochClose; the clients'
  rules of K52; section 4.7's rows.
- **Stage 6.5** (task 0.7 of its change, extended): U1 once decided; the refusals and bounds of record kinds 21 and 22
  and of the two objects in section 19.4; the vectors of K30, K33, K36, K38 and K41; its `tile-grid` and
  `tile-simulation` deltas, which state the manifest, the reserve, step 2, recovery, repair and the scrub as they were
  before the extent records (R4-22); the object store's interface and its model in design D1.
- **Stage 6.6:** the bodies of the fence's command and of the offer at 540 (K24, K25); `CATALOG.md` and the
  `metadata-group`, `ugs-catalog` and `wire-protocol` deltas, which have the metadata group allocate and Describe
  answer from a core's memory (R2-1, R2-5); K27's `version` in Describe.
