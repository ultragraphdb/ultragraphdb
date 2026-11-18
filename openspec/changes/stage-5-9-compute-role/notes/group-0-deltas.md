# Group 0, task 0.1: the deltas on stage 5.8's archived text

Nov 18, 2026. Every MODIFIED requirement of this change's 15 deltas over existing capabilities was compared, text and
scenarios, with the same requirement in `openspec/specs/`; `node-discovery` is new and holds ADDED requirements alone.
A delta now differs from the archived text only by this change's own edits: the role's name, the relay's removal, the
node list, Members for storage alone.

## 1. Result

- 65 MODIFIED requirements compared, 14 ADDED, 1 REMOVED, 7 RENAMED.
- `openspec validate stage-5-9-compute-role --strict` passes.
- A trial `openspec archive stage-5-9-compute-role -y` on a copy of `openspec/` applies cleanly: 14 added, 65 modified,
  1 removed, 7 renamed.
- After the trial archive, every scenario of today's `openspec/specs/` under a requirement this change modifies is
  present, present under its two-role name (section 4), or one of the removals of section 3.
- After the trial archive, `frontend` and `ComputeRead` stand in no requirement but where this change refuses them by
  name. Four Purpose lines still name the frontend (section 6).

## 2. Per capability

"Rename only" means the delta equals the archived text with the role's name changed.

### client-endpoint (14 MODIFIED, 1 REMOVED, 7 RENAMED)

| Requirement | Outcome |
| --- | --- |
| Compute serves the binary protocol's clients | Rename only |
| The client endpoint speaks TLS 1.3 when configured | Rename, and this change's sentence on the certificate's host |
| Connections authenticate against the metadata group's principals | Rename only |
| Requests are routed to the partitions that serve them | This change's edits kept. Three sentences the merge left circular corrected: "any later read compute answers so … relay code 23" is "any later read ending so … answer code 23"; "compute answers A with code 23" is "read A ends with code 23"; "resend the read" is "run the read again" |
| Errors reach the client through the error table | This change's edit kept (no read moves to another compute process) |
| Vector-index declarations wait for stage 8 | Rename only |
| Every request naming a tenant passes one gate | Rename only |
| Compute bounds what a connection's reads hold back | Rename. "compute reads the Credit and relays it" is "grants it to the read" |
| Compute bounds what a connection has on its way to a role | Rename only. The RENAMED entry named "…on its way to a partition" while the MODIFIED header named "…to a role": the archive would have found no such requirement. Both now say "to a role", as the body does |
| Compute times a read-write transaction | Rename only |
| A Commit nothing bound is answered by compute | Rename only |
| The endpoint asks the system group | Head brought over: the second Check's outcomes 21, 14 and 1, a frame's fill within `max_frame` in ascending order of sessions, the definition of a turn. Scenario "One turn's commits share one Check frame" brought over |
| An endpoint core keeps a view of what the closes announce | Head brought over: catalog versions with their effective epochs, a subscription opened again, a close arrived at the deadline, the early refusals. Scenario "A token ahead of the core's view waits and is not refused" brought over; "A subscription opened again reads what its skipped closes announced" restored |
| Compute finds the partitions in Members | This change's edits kept. "one of its role connections to a partition closes" set back to the archived "one of its role connections closes" |

REMOVED "Reads are spread over the compute processes" exists under that name. The seven RENAMED sources exist under
their names.

### commit-rounds (1), partitions (1), schema-storage (1), term-dictionary (1)

Rename only in each: "Commits are grouped into rounds", "A read-write transaction stays within one partition in this
stage", "The schema persists in the metadata group", "IRIs and blank nodes get system IDs".

### config-service (2 MODIFIED)

| Requirement | Outcome |
| --- | --- |
| The config service validates every setting before it serves one | The Data row's rule for comparing two directories brought over. New row "Tables" and scenario "Another role's table is refused by name" (section 5). This change's Roles and Compute rows and its three compute scenarios kept |
| A configuration error tells the administrator what to do | Rename. "A hung compute's stall of no time is refused" is "A hung partition's stall of no time is refused": compute's file sets the stall by which compute times a partition |

### kubernetes-deployment (5 MODIFIED)

| Requirement | Outcome |
| --- | --- |
| The roles deploy on Kubernetes from the repository's Helm chart | This change's edits kept |
| Role addresses are stable names | This change's edits kept |
| Settings, keys and secrets reach the roles as Kubernetes objects | Rename only |
| A pod's data survives its restart | Rename. "its Deployment starts another" is "its StatefulSet starts another", since compute is a StatefulSet from this change |
| The bench runs inside the cluster | Rename only |

### metadata-group (3 MODIFIED, 1 ADDED)

| Requirement | Outcome |
| --- | --- |
| Catalog versions take effect at an epoch boundary | Rename. Scenario "A catalog version past its cap is refused at its Commit" brought over |
| Principals live in the metadata group | Rename only |
| The metadata group holds the tenants | Rename only |

### quad-reads (6 MODIFIED, 2 ADDED)

| Requirement | Outcome |
| --- | --- |
| Compute runs each read on one core | This change's edits kept |
| Storage's read kernel answers what compute scans | Rename only |
| A partition answers a Recall from its session table | Rename only |
| A read through compute is a read of its transaction | This change's edits kept |
| A read's transaction names its level | Head brought over: a snapshot's epoch is "the latest closed epoch the endpoint core's view held". This change's edit of one scenario kept |
| A read's batches fit the client's frame | This change's edits kept. "with no relay" and the scenario name "A Cancel stops a read without a relay" named what is gone: now "in the turn compute reads it" and "A Cancel stops a read in the turn it is read" |

### role-binaries (8 MODIFIED)

| Requirement | Outcome |
| --- | --- |
| Each role is its own binary and image | This change's edits kept |
| Roles meet through the transport at their addresses | This change's edits kept |
| The supervisor starts, stops and reports the roles | This change's edits kept. "the compute node is killed" set back to the archived "the compute process is killed" |
| Each role runs one runtime per core | Rename. "to the system group and to the other roles" is "to the system group and to the partitions" |
| Readiness, health and shutdown | The sentence on `/ready` answering 503 while the system group is unreachable brought over, for compute. Scenario restored as "Compute is unready while its system group is unreachable" |
| Configuration is explicit and validated at start | Rename only |
| A role takes a port a killed process leaves held | Rename only |
| Roles authenticate each other with the cluster's role secret | Rename only |

### schema-changes (3 MODIFIED)

| Requirement | Outcome |
| --- | --- |
| A partition checks the catalog version a Drop names | Head brought over: what the early refusal is checked against. Three scenarios restored: "Compute refuses a Drop naming another version than the one in effect", "A pending version leaves a Drop naming the version in effect to the partition", "During a check a Drop names the version being checked" |
| Drops | Rename. Three scenarios the delta held and stage 5.8 moved to the backlog on 2026-11-17 (items 21 and 22) taken out again: "Dropping a vector predicate drops its vector index", "Dropping a tenant's everything survives a restart", "A tenant's drop leaves a tenant sharing its run" |
| A tightening is checked before it is acknowledged | Rename only |

### sequencer (3 MODIFIED, 1 ADDED)

| Requirement | Outcome |
| --- | --- |
| The sequencer closes epochs at its own wall-clock time | The archived text still has the frontend host the sequencer (section 6). Renamed whole it would say compute hosts it. The delta says: a value out of bounds stops "the storage process hosting the sequencer"; the close is asked "when compute asks"; the scenario starts "the hosting storage process"; "compute asks the sequencer" |
| Partitions learn each closed epoch and stamp the open one | Rename only |
| An early close is asked by CloseRequest from either role | Head and scenario "A request for a closed epoch closes nothing" brought over: a request naming a closed epoch is answered by no frame |

### system-group (4 MODIFIED, 1 ADDED)

| Requirement | Outcome |
| --- | --- |
| One storage instance hosts the system group | Rename only |
| The system group serves the books' role commands at its role address | This change's table kept. The refusal of a command another role requests, and the `FieldValue` rules of Subscribe and CloseRequest, brought over. Scenario "A command another role requests is refused" restored. The table is cited as "the endpoint table of `PROTOCOL.md` section 11.12", as archived, since task 0.2 folds section 11.12a's table into it |
| The hosting process starts the group first and stops it last | Rename only |
| A lost system group is waited for until the deadline | The sentence on joining again brought over, for storage; compute's rejoin is cited from node-discovery. Five scenarios of storage's readers restored verbatim: "A connection error ending a subscription's connection is joined again", "A frame on a stream storage has not opened closes the subscription's connection", "A late frame among a catalog version's frames is dropped", "A catalog version's frame on stream 0 closes the connection", "A catalog version's frame on a stream storage has not opened closes the connection" |

ADDED "The system group serves the node list" repeated a rule stage 5.8 added to sequencer, "A subscriber holding no
close begins at the latest": a subscription unacknowledged for twice `epoch_ms_max` is ended. The requirement now cites
it and states only what it means for a node's entry.

### transactions (4 MODIFIED)

| Requirement | Outcome |
| --- | --- |
| Interactive transactions | Rename only |
| Exactly-once commits through sessions | Head brought over: the second Check's outcomes after every partition answered 18. Scenario "A retry checked in the quiet time stays undecided" restored |
| A request declares its type | Rename only |
| Commits answer a session token | Head brought over: a Begin naming a token ahead of the view is held, and a one-frame write goes to its partition. Scenario "A token above the open epoch is refused", which stage 5.8 retired, taken out again |

### wire-protocol (9 MODIFIED, 1 ADDED)

| Requirement | Outcome |
| --- | --- |
| Data requests, prepared statements and describe | Head brought over: Describe's `catalog` field and `read_version` from the core's view. Scenarios "Describe names the catalog version it is drawn from" and "A client's Describe naming a catalog version is refused" restored |
| Requests name terms by their text | Rename only |
| A write commits in one frame | Rename only |
| Roles exchange role commands | This change's edit kept. Scenario "The retired relay command is refused" stated history in its name and its WHEN: now "Command 523 is refused as unknown" |
| A parked request holds no role connection | Rename. "its compute node sends" is "a compute node sends" |
| A frame past its stream's credit is refused | The delta held two scenarios of one name. The one adapted from the relay's scenario is "An Entries frame past its credit ends every read its connection carried" |
| A commit in flight is in doubt once sent | Rename only |
| A request on a stream that cannot take it is refused | Rename only |
| The client's timeout leaves a read its deadline | "a hung compute node is answered with code 17 by the deadline plus `scan_stall_ms`" had no meaning without the relay. It now gives the reason `PROTOCOL.md` section 11.11 gives: past the deadline a partition keeps a Scan or a Frontier with no frame able to leave for at most `scan_stall_ms` |

### ADDED requirements

None exists in `openspec/specs/` under its name. One held a rule stage 5.8 already states (system-group, above). The
others state what no archived requirement does: the node list, Roster, Nodes, a close's `nodes` flag, a read's step,
and compute's timer for a hung partition.

## 3. Scenarios removed on purpose

| Capability, requirement | Scenario | Reason |
| --- | --- | --- |
| client-endpoint, "Reads are spread over the compute processes" (REMOVED) | Its eight scenarios: "Reads alternate between two compute processes", "A closed compute process is skipped", "No compute process can be reached", "A read is never resent after a frame", "A hung compute process answers code 17", "A compute process waiting within the deadline is not hung", "A frame restarts a hung compute's timer", "A stream without credit holds off a hung compute's timer" | Compute executes a client's read itself, so there is no compute process to choose, skip or time. quad-reads, "Compute times a partition that stops answering", holds the timer that remains |
| client-endpoint, "Compute finds the partitions in Members" | "A compute process that joins later is found before a read is refused" | Members holds storage alone and no read is relayed. "Members names no compute node" stands in its place |
| quad-reads, "A read's batches fit the client's frame" | "An Execute too large for compute's frame is refused" | The refusal was of an Execute whose ComputeRead would not fit a frame. No ComputeRead exists |
| system-group, "A lost system group is waited for until the deadline" | "A compute process joins a restarted system group at once" | Its THEN is Members listing compute again. Members lists storage alone, and a compute node's entry outlives the group's restart by the log (node-discovery, "A restart of the system group keeps the list") |
| wire-protocol, "Roles exchange role commands" | "ComputeRead carries the client's limits" | ComputeRead is retired. "Command 523 is refused as unknown" stands in its place |
| wire-protocol, "A frame past its stream's credit is refused" | "A Result frame past its credit closes the frontend's role connection" | No Result frame crosses a role connection. Its rule for the reads a closed connection carried is kept for Entries, as "An Entries frame past its credit ends every read its connection carried" |

Two scenarios of role-binaries, "The frontend binary carries no storage engine" and "The frontend binary carries no
book", are one scenario, "The compute binary carries no storage engine and no book", naming every crate both named.

A MODIFIED block is copied whole into the main specification, so a reason written into it would become requirement
text. The reasons therefore stand here and not in the deltas.

## 4. Scenarios renamed to two roles

| Capability | Archived name | Name in the delta |
| --- | --- | --- |
| client-endpoint | The frontend never holds a stored key | Compute never holds a stored key |
| client-endpoint | An empty transaction commits at the frontend | An empty transaction commits at compute |
| client-endpoint | A restarted frontend serves the sessions it served before | A restarted compute node serves the sessions it served before |
| client-endpoint | The frontend listens at no role address | Compute listens at no role address |
| commit-rounds | Commits a frontend sends during a sync share the next round | Commits a compute node sends during a sync share the next round |
| config-service | The frontend's old settings are refused by name | The retired role's settings are refused by name |
| config-service | A hung compute's stall of no time is refused | A hung partition's stall of no time is refused |
| kubernetes-deployment | The frontend's pod holds no bootstrap secret | Compute's pod holds no bootstrap secret |
| kubernetes-deployment | A deleted frontend pod loses nothing | A deleted compute pod loses nothing |
| quad-reads | A read's stream starts with the frontend's credit | A read's stream starts with the client's credit |
| role-binaries | The frontend serves clients on every core | Compute serves clients on every core |
| role-binaries | The frontend is ready only once the books answer | Compute is ready only once the books answer |
| role-binaries | The frontend is unready while its system group is unreachable | Compute is unready while its system group is unreachable |
| role-binaries | The frontend's /health answers once its listener is bound | Compute's /health answers once its listener is bound |
| schema-changes | The frontend refuses a Drop naming another version than the one in effect | Compute refuses a Drop naming another version than the one in effect |
| schema-changes | A resent Drop builds the version a stopped frontend left | A resent Drop builds the version a stopped compute node left |
| schema-changes | A check outlives the frontend that asked for it | A check outlives the compute node that asked for it |
| sequencer | The frontend's request closes the open epoch | Compute's request closes the open epoch |
| system-group | The frontend holds no book | Compute holds no book |
| system-group | Storage is ready before any frontend exists | Storage is ready before any compute node exists |
| system-group | A frontend started before storage waits | A compute node started before storage waits |

## 5. Another role's table (config-service)

The scenario added to "The config service validates every setting before it serves one":

- **WHEN** `storage_1` holds a `[compute]` table
- **THEN** the revision is refused naming `compute` and the instance

with the check's row: an instance's file holds a table of another role, none of whose settings its own role reads: a
storage instance's file a `[compute]` or an `[endpoint]` table, a compute instance's file a `[system]` table.

The example is a storage file and not a compute file holding `[storage]`, because compute reads two settings of that
table from its own file today: `storage.scan_stall_ms` and `storage.read_retain_ms` (`ultragraphdb_compute`'s
`service.rs`). The same requirement group keeps a scenario in which `ci_compute.toml` sets `storage.scan_stall_ms`.
Refusing `[storage]` in a compute file needs those two settings moved first, which is the user's decision (section 7).

## 6. Found in the main specifications

Reported, not edited.

1. **Purpose lines name the frontend.** `client-endpoint` ("The frontend's client endpoint"), `role-binaries` ("the
   frontend, compute and storage as three binaries"), and `metadata-group` and `sequencer` ("one replica inside the
   frontend", wrong since stage 5.8). A delta cannot change a Purpose, so these four are edited by hand at this
   change's archive. `node-discovery` receives a placeholder Purpose from the archive, to be written then.
2. **sequencer, "The sequencer closes epochs at its own wall-clock time"** has the frontend host the sequencer and be
   stopped by its bounds. Stage 5.8 moved the sequencer to storage and left this text. Corrected in this change's
   delta.
3. **sequencer, "An early close is asked by CloseRequest from either role"** says "from a partition whose epoch is
   full, as before", which is history in a requirement. Left as archived.
4. **The deltas of stages 6.5 and 6.6** hold "Dropping a tenant's everything survives a restart" and "A tenant's drop
   leaves a tenant sharing its run" under schema-changes' "Drops", which stage 5.8 moved to the backlog. Their rebase on
   this change must take them out, or their archive puts them back.

## 7. Questions for the user

1. **`[storage]` in a compute file.** Should compute's `scan_stall_ms` and `read_retain_ms` move to a table of its own,
   so that a compute file holding `[storage]` is refused as another role's table? Until then the rule refuses only a
   table none of whose settings the role reads.
2. **The requirement's name.** "Compute bounds what a connection has on its way to a role", as its body says, or "…to a
   partition", as the RENAMED entry said?
3. **The client's read timeout.** Compute answers a read on a silent partition with code 17 at the deadline plus twice
   `scan_stall_ms`, 60 s at the default, while the Rust client's timeout is at least 30 s past the deadline. The
   client's timeout can end the connection first. Is the floor raised to twice `scan_stall_ms`'s bound, or is the
   stall's bound halved?
4. **Where a removal's reason stands.** Task 0.1 asks for it "in the delta". It stands in section 3 here, since the
   archive copies a MODIFIED block whole. Is that accepted?
