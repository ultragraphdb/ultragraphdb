## Why

The frontend process holds the cluster's books: the metadata group, the sequencer, the session registry and the
principals run on one owning core inside it, with their two logs in its data directory. Every other process joins the
frontend, and a storage or compute process that loses it for `join_deadline_ms` exits. One process is therefore both
the door clients knock at and the only keeper of what the cluster agrees on, and it is the one process that cannot be
run twice. The architecture review of 2026-11-12 found that this is what stops the cluster after 60 seconds without a
frontend, and what would force a third replicated service at stage 8.

`openspec/designs/UltraGraphDB Distribution Architecture.md` decides the target: two roles, compute and storage, and
the books in the *system group*, a replica group that storage hosts under the data's own replication protocol. This
change is the first of three steps towards it, and the order is strict (`ROADMAP.md`): stage 5.8 moves the books,
stage 5.9 merges the frontend and compute, which are then two processes holding nothing durable, and stage 6.5 builds
the tile store under roles that no longer change. The protocol for both role stages is specified already
(`specification/binary_protocol/PROTOCOL.md` sections 11.12a and 13.23 to 13.28; the record is
`openspec/notes/UltraGraphDB Specification Update.md`, checked by the review of
`openspec/notes/UltraGraphDB Spec Consistency Review 2026-11-12.md`).

The stage moves behaviour and adds none. What the frontend's endpoint cores ask their owner by message today, they
ask the system group by role command afterwards, so every client-facing suite compares the cluster before and after.

## Success

1. **The books live in storage.** A storage process whose settings hold a `[system]` table hosts the system group: the
   metadata state machine and the sequencer on a core of their own, their logs on that process's volume, served at the
   system group's role address. The frontend holds no log, no snapshot and no data directory.
2. **Every client-facing suite green, unchanged.** The acceptance batteries of `ultragraphdb_tests` and the role test
   crates pass with the scenarios they hold today; only the scenarios that name where the books live change.
3. **A restarted frontend loses nothing.** A frontend killed and started again serves the sessions, principals,
   catalog versions and epochs it served before, with no recovery of its own.
4. **Storage starts first and depends on no other process.** The hosting storage process reaches `/ready` with no
   frontend running, and a frontend or compute process that loses the system group retries until its deadline.
5. **The round trip is measured.** Reads are recorded on the stage 5.7 cluster before the move, since the benchmark
   record of 2026-11-09 holds writes alone, and the whole benchmark is rerun after it, so what a commit's Check costs
   is known before stage 5.9 removes a read's hop.

## What Changes

- **The system group.** A new crate holds what leaves the frontend: the owner of the metadata state machine, the
  sequencer and the membership, its thread over the two logs with their snapshots, and the session serving its role
  commands. The storage binary starts it before its partitions when its settings name it.
- **BREAKING: the role commands of the books.** The system group's endpoint serves Join, Members, Placement,
  Catalog, Subscribe, CloseRequest, Lease, DefaultEntry and Report as the frontend's did, and seven the frontend asked
  itself in process: Challenge (519) and Verify (520) for a client's authentication, Session (534), Check (535) and
  Ended (536) for the session registry, the owner check and the mark of a commit in flight, Declare (538) for the
  catalog versions a declaring Commit and a Drop call for, and Defaults (539) for the default-entry records a Describe
  names.
- **BREAKING: the frontend holds nothing durable and accepts no role connection.** Each endpoint core dials the
  system group, subscribes to the sequencer's closes as a partition does, and keeps a view of what the closes and the
  catalog announce. It sends no Join, reads the partitions' and compute's addresses from Members and dials them. Its
  role address and its data directory go.
- **BREAKING: authentication.** A principal's stored key and server key no longer leave the metadata state machine's
  host: the frontend relays a client's SCRAM exchange through Challenge and Verify. The counts of failed
  authentications, the throttle and the ban stay in the frontend, held by one of its cores for all of them.
- **BREAKING: settings.** `[system]` with `role_address` and `data_dir` on exactly one storage instance, which also
  takes the `[metadata]` and `[sequencer]` tables; `cluster.system_address` on every instance in place of
  `cluster.frontend_address`; `frontend.role_address` and `frontend.data_dir` removed. The config service checks them.
- **The start order reverses.** The supervisor starts the config service, storage, compute and then the frontend.
  Storage's readiness no longer waits on a frontend, the frontend's waits on the system group and its partitions, and
  the exit after a lost dependency moves from "the frontend" to "the system group", which the hosting storage process
  never loses.
- **Deployment.** The Helm chart runs the frontend as a Deployment without a volume, mounts the bootstrap secret into
  the hosting storage pod, and points every role at that pod's stable name. The development image's data layout
  follows.
- **Removed:** the frontend's `Owner`, `OwnerHandle`, `start_owner`, `RoleSession`, `Membership`, `JoinFanOut` and the
  in-process ask interface; the frontend's role listener; `common_role::frontend_address`.

No log, snapshot or data directory written before this change is read after it: the metadata and sequencer logs move
to another process's directory, and there are no outside users. A development cluster is started fresh.

## Capabilities

### New Capabilities

- `system-group`: the replica group storage hosts for the cluster's books: which storage instance hosts it, its core
  and logs, its role address and the commands it serves, what it answers while a log syncs, the session registry's
  marks by connection, its start before the partitions of its process and its stop after them.

### Modified Capabilities

- `metadata-group`: a state machine of the system group in place of the frontend; the bootstrap credential read by its
  host; sessions registered, checked and marked through role commands; the first start creating tenant 0 in the host.
- `sequencer`: a state machine of the system group; a pending version assigned after the host's crash.
- `role-binaries`: what each binary holds, the supervisor's start and stop order, readiness, the data layout, the
  exit after a lost system group, and why a role connection is verified locally.
- `client-endpoint`: the endpoint asks the system group in place of an owner; authentication through Challenge and
  Verify with the counts held in the frontend; the frontend's view of epochs, tenants and catalog versions; reading
  Members in place of taking Joins.
- `partitions`: the system group forms the placement, and a storage process claims its partitions there.
- `transactions`: the owner check and the mark of a commit in flight asked of the system group.
- `quad-reads`: a snapshot read's early close asked by CloseRequest.
- `schema-changes`: a Drop's catalog version and a tightening's check held by the system group, whichever frontend
  stops.
- `wire-protocol`: the role commands the system group serves, and Describe answered from Catalog and Defaults.
- `config-service`: the `[system]` table, the one hosting instance, `cluster.system_address` and the tables that move.
- `kubernetes-deployment`: the frontend without a volume, the hosting storage pod, the secrets' mounts and the address
  every role dials.

## Impact

- **New crate:** `ultragraphdb_common_system`.
- **Rewritten or shrunk:** `ultragraphdb_common_frontend` (the owner and its ask interface leave; a link to the system
  group and a local view arrive), `ultragraphdb_frontend`, `ultragraphdb_common_storage`, `ultragraphdb_storage`,
  `ultragraphdb_common_compute`, `ultragraphdb_compute`, `ultragraphdb_common_role`, `ultragraphdb_protocol_wire` (five
  new bodies, Challenge and Verify served), `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`,
  `ultragraphdb_supervisor`, the role test crates and `ultragraphdb_tests`, `ultragraphdb_tool_bench`'s profiles.
- **Specifications:** `PROTOCOL.md` sections 11.12, 11.13 and 13 brought to the text of section 11.12a for this stage;
  golden frames and fuzz targets for Session, Check, Ended, Declare, Defaults, Challenge and Verify.
- **Deployment:** `deploy/helm/ultragraphdb`, the development image and its profile, `scripts/cluster_local.sh`.
- **Documents:** `AGENTS.md`'s crate table and role descriptions, `ROADMAP.md`'s row, the benchmark record under
  `openspec/benches/`.
- **Stage 5.9** builds on this change's link to the system group and removes the frontend's relay of reads. **Stages
  6.5 and 6.6** are rebased on both.
