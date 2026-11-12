## Context

The target is `openspec/designs/UltraGraphDB Distribution Architecture.md` (*The system group*; *The path*, item 1).
The wire is `specification/binary_protocol/PROTOCOL.md`: section 11.12a states what this stage changes, sections 13.8,
13.9 and 13.23 to 13.28 the bodies it adds or first serves. The decisions behind that text are S1 to S24 of
`openspec/notes/UltraGraphDB Specification Update.md`, checked by
`openspec/notes/UltraGraphDB Spec Consistency Review 2026-11-12.md`. Where this file and those disagree, those win and
this file is corrected.

The code today. `ultragraphdb_common_frontend` holds `Owner`: the metadata state machine, the sequencer, the
membership, the marks of commits in flight and the authentication throttle, on one thread (`start_owner`) over two
write-ahead logs with snapshots under `frontend.data_dir`. Endpoint cores ask it through `OwnerHandle`, an in-process
channel carrying the `Asked` enum: 24 asks, of which storage and compute reach nine as role commands through
`RoleSession` at the frontend's role address. Storage and compute are configured with `cluster.frontend_address`, join
there, and exit with code 69 once the frontend is gone for `join_deadline_ms`. The supervisor starts the frontend
before storage. The storage binary already links `ultragraphdb_cluster_metadata` and `ultragraphdb_cluster_sequencer`
for their types.

Stakeholders: the user, who commits after each group; stage 5.9, which merges the frontend and compute over this
change's link to the system group; stages 6.5 and 6.6, rebased on both; stage 8, which replicates the group.

## Goals / Non-Goals

**Goals:**

- The system group hosted by a storage process, serving every command section 11.12a's stage 5.8 table lists.
- A frontend that holds nothing durable, accepts no role connection and can be killed and restarted with no recovery.
- Every client-facing scenario unchanged and green; the scenarios naming where the books live rewritten.
- The round trip a commit's Check adds, measured against a baseline taken before the move.

**Non-Goals:**

- The merge of the frontend and compute, `Nodes`, `Roster`, tags and client discovery (stage 5.9). One frontend still
  runs, and the config service still refuses a second.
- Replication of the system group (stage 8). It has one replica, as the metadata group and the sequencer have today.
- The offer of a compiled catalog version at command 540 and the fence carried in `EpochClose` (stage 6.6).
- A session check that needs no round trip (item 13 of `openspec/notes/backlog.md`) and load control.
- Changing a state machine. `ultragraphdb_cluster_metadata` and `ultragraphdb_cluster_sequencer` keep their commands,
  logs and snapshots; only their host changes. The Lean models are untouched.

## Decisions

**D1. One new crate, `ultragraphdb_common_system`.** It takes what leaves `ultragraphdb_common_frontend`:

| From `common_frontend` | In `common_system` |
|---|---|
| `Owner`, its turn, its answers while a log syncs, its catalog and check code | `SystemGroup` |
| `OwnerHandle`, `Asked`, `ReportAsked`, `DropStep` | `SystemHandle`, `SystemAsked`: what the group's sessions ask its core |
| `start_owner`, `OwnerStart`, the owner thread, the snapshot file | `start_system`, `SystemStart` |
| `RoleSession` | `SystemSession`: one per role connection to the group's address |
| `Membership` | `Membership` |
| the building of a catalog version from declarations (`utils/declarations.rs`) | the same, behind Declare |

`common_system` depends on `cluster_metadata`, `cluster_sequencer`, `cluster_placement`, `common_role`,
`common_service`, `core_catalog`, `protocol_wire`, `protocol_scram`, `traits_log` and `transactions_log`.
`common_frontend` loses its dependencies on `cluster_metadata`, `cluster_sequencer`, `traits_log` and
`transactions_log`. `LoginAsked` splits: the verifier goes (D8), and `Admit` and `Outcome` stay in the frontend with
`AuthThrottle`. *Alternative:* putting the group into `common_storage` ties the books to the partitions' crate, which
stage 6.5 rewrites, and a crate of its own is what stage 8 gives a replica group to.

**D2. One storage instance hosts the group, named by its settings.** A storage instance whose file holds a `[system]`
table hosts the system group:

| Setting | Meaning |
|---|---|
| `system.role_address` | where the group listens for role connections, an address of its own beside the partitions' |
| `system.data_dir` | the directory holding `meta/` and `seq/`, the two logs and their snapshots |
| `cluster.system_address` | on every instance, the address it dials; replaces `cluster.frontend_address` |

The hosting instance also holds the `[metadata]` and `[sequencer]` tables, which leave the frontend's file.
`frontend.role_address` and `frontend.data_dir` are removed, and `ServiceConfig` refuses them as unknown. The config
service refuses a target with no hosting storage instance or more than one, a `[metadata]` or `[sequencer]` table on
an instance that does not host, and a `cluster.system_address` that differs from the host's `system.role_address` in
scheme, port or Unix path. *Alternative:* the instance holding partition 1 hosts, with no table. A table states the
choice where an operator reads it, and stage 8 turns "one instance" into "these three" by lifting one check.

**D3. The group runs on a core of its own, over logs of its own.** The storage process starts one more thread and
runtime for the group, as the frontend started its owner's, pinned after the partitions' when `pin_cores` asks. The
metadata log and the sequencer's log stay two logs with their own syncs, apart from every partition's log. The group
goes on answering Check, and what else a commit asks, from what is durable while one of its logs syncs
(client-endpoint, "One owner answers for the metadata group", moves to system-group). *Why:* the commit measurements
found a close's sync delaying the checks behind it, which answering from durable state already resolves; sharing a
flush with a partition's log would make a partition's commit wait on a close and a close on a commit. Stage 8 replaces
these logs with the group's replicated log and decides the flush then.

**D4. A co-hosted partition dials the group like any other.** The partitions of the hosting process open role
connections to `cluster.system_address`, as partitions of another process do: one path, tested once, and the
development container exercises what a cluster runs. No partition asks the group anything per commit (Lease runs
ahead of need, DefaultEntry at a predicate's first write, Report once per catalog version, closes arrive pushed), so
the socket costs no commit a round trip. `PROTOCOL.md` section 11.12a is brought to this.

**D5. Each endpoint core holds its own connection to the group.** A core dials the system group as it dials each
partition, subscribes on that connection, and sends its Checks and Endeds there. Nothing crosses cores on the commit
path, and the marks a core began end when its connection does. *Alternative:* one connection per process needs a
funnel across cores on every commit, a hop and a shared structure the thread-per-core design avoids elsewhere.

**D6. The frontend keeps a view, and the group stays the authority.** From its subscription and its reads a core holds:
the latest closed and the open epoch; the placement in effect and the tenants its runs list, read again when a close
assigns a placement version; per tenant the catalog versions it read with Catalog and the latest version a close or its
own Declare named; the schema id it computes from a version's bytes. A Begin answers from the view, with no round trip.
A core holds no close of its own, so it subscribes with `latest` (review K3), and a token ahead of its view is waited
for and not refused (K4). The view may lag by the versions another core committed since the last close, and no decision
rests on it: a declaring Commit built on a stale version is refused by the group with code 2, kind 5, as section 11.13
states, and a tenant's existence changes only by a command of stage 7. Describe asks the group which version to report,
by a Catalog request of `version` 0 naming the latest closed epoch, since only the group knows a version being checked
or withdrawn (K17), and reads the default-entry records with Defaults. One connection's snapshots never go back; across
connections only a token carries the floor, which is the user's to confirm (review U2).

**D7. Sessions, the owner check and the mark travel as Session, Check and Ended.** A core sends the Endeds of one turn
in one frame and then its Checks in one frame, one check per session (section 11.12a). The group decides each check
as the owner did, in the order 12, 21, 14, and a check of kind 1 naming a marked session answers 14, as the owner's
`begin_recall` does today: the mark is the one place that knows a resend's original is still on its way, whichever
core the resend arrives at. A passed check of kind 2 marks the session against the connection that sent it. The mark
ends by an Ended; when its connection closes it is kept for `transaction_ms` and then ends, and an Ended from any
connection ends a mark whose connection has closed. For `transaction_ms` after it begins to listen the group evicts
nothing and answers every check of kind 1 with 14, since a restart loses the marks of requests still on their way.
A Session is never sent again: a lost answer is the client's to repeat under a new identity (review K1, K2, K8 to
K10). *Alternative:* ending a mark with its connection, which the update first wrote, lets a resend be told "not
committed" while its original is on its way whenever the two connections fail apart.

**D8. Authentication is relayed, and the throttle stays.** For each client exchange a core sends Challenge with the
name and Verify with the AuthMessage and the proof, and passes the group's salt, iteration count and ServerSignature
on. The frontend holds no `ScramVerifier`; `AskedCredentials` goes. `AuthThrottle` stays in the frontend: its first
endpoint core holds the counts and the others ask it by message, `Admit` at an accept and `Outcome` after an
exchange, as they asked the owner. The role secret is still verified locally by each side of a role connection, now
because a role connection must open before the group can be asked anything.

**D9. The frontend reads Members and takes no Join.** It reads Members at start until every partition of the
placement is listed, retrying until `join_deadline_ms`; again when one of its role connections closes; and again
before it refuses a read for want of a compute process. `JoinFanOut` and `CoreJoins` go. A compute process that joins
while others serve is found at the next of those events. The rule lives one stage: stage 5.9 has no compute process
to find.

**D10. Start, readiness, stop and exit.**

| Process | `/ready` once | Exits 69 when |
|---|---|---|
| Storage hosting the group | the group has recovered and listens, and its partitions have recovered and joined | never for a lost peer |
| Storage not hosting | its partitions have recovered and joined | the group is gone for `join_deadline_ms` |
| Compute | the group and a partition answer | the group or its storage is gone for its deadline, as today |
| The frontend | the group answers, the view holds a placement and a close, Members lists every partition and one answers | the group is gone for `join_deadline_ms` |

Inside the hosting process the group starts first and its partitions after it, since a partition's recovery reads the
catalog versions its log names. The supervisor starts the config service, then storage and waits for `/ready`, then
compute, then the frontend. On SIGTERM the frontend stops accepting first, compute drains, and storage flushes its
partitions and then stops the group.

**D11. Data and deployment.** `system.data_dir` holds `meta/` and `seq/`. The development profile sets it to
`/data/system`, so `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`. The Helm chart runs the frontend
as a Deployment with no volume and keeps its Service for clients; the first storage StatefulSet hosts the group, with
the bootstrap secret mounted into its pod alone; `cluster.system_address` names that pod's stable DNS name. No data
written before this change is read after it.

**D12. Verification.**

- **Before and after.** The client-facing features of `ultragraphdb_tests`, `ultragraphdb_frontend_tests`,
  `ultragraphdb_storage_tests` and `ultragraphdb_compute_tests` run unchanged. A feature is edited only where a step
  names the frontend as the books' host.
- **The group alone.** `common_system` is tested against an `OrderedLog` double as the owner is today, and its
  session against the frames of each command: golden frames for Session, Check, Ended, Declare, Defaults, Challenge
  and Verify, fuzz targets for each decoder.
- **Laws.** Property tests with Hegel, from this stage on (`tdd-protocol.md`): the new bodies round-trip and refuse by rule, and the registry and the catalog run as state machines against a model written from the specification, which is where the rules for marks, evictions and checks were found wanting in review.
- **Failures.** The group's host killed with commits in flight; the frontend killed during a tightening's check; the
  frontend's connection to the group closed and dialled again with requests at partitions; a frontend started before
  storage.
- **The benchmark.** Group 0 records reads on the stage 5.7 cluster. The last group reruns
  `openspec/benches/benches-2026-11-09.md` and the read baseline, and records both in a dated file with the
  difference a commit's Check makes.

## Risks / Trade-offs

- **A commit gains a round trip to another process** → batched per turn and core; measured in group 9 against the
  half millisecond one hop between two virtual machines cost on 2026-11-09; answered for good by backlog item 13.
- **Two round trips per authentication** → an authentication happens once per connection, and the stored key stays
  in one process.
- **A closed connection's marks hold their sessions for `transaction_ms`, and a restarted group answers no resend
  for as long** → a session answers 14 for up to ten seconds after its endpoint's connection or the group was lost;
  nothing is paid while connections stand, and stage 8 revisits it with backlog item 13.
- **The frontend's view lags** → nothing is decided from it (D6).
- **A tenant created at stage 7 is unknown to a frontend's view until it reads Placement again** → stage 7's command
  announces a tenant with a close, as a catalog version is; recorded for that change.
- **Moving `Owner` invites rewriting it** → the group's suite is the owner's suite moved, green with the same count
  before any behaviour changes, in a commit of its own.

## Migration Plan

No data migrates: a development cluster starts fresh on the new layout, and no other cluster exists. The groups land
in the order of `tasks.md`; the tree builds and `make test` passes after each. Rolling back is reverting the group's
commit.

## Open Questions

- Does a storage instance that hosts the group but no partition make sense before stage 8, or does the config service
  refuse it?
