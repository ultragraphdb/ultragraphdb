## Why

After stage 5.8 the frontend holds nothing durable: the cluster's books live in the system group on storage, and the
frontend asks for them by role command. What remains of the split between the frontend and compute is a relay. A
client's read arrives at the frontend, is wrapped into a ComputeRead, crosses to a compute process, and its rows cross
back, with the read's credit and cancel relayed after it. That hop is paid on every read, it needs rules of its own
for a compute process that closes or hangs, and at stage 7 it would force a wire format for query plans between two
processes that have no reason to be apart.

`openspec/designs/UltraGraphDB Distribution Architecture.md` decides the target: one role, named compute, that holds
the client endpoint and executes reads, beside storage. This change is the second of three strictly ordered steps
(`ROADMAP.md`): stage 5.8 moved the books, this stage merges two processes that are both stateless, and stage 6.5 then
builds the tile store under roles that no longer change. With the merge a cluster may run several compute nodes, so
the stage also gives clients the way to find them: seed addresses, a node list with tags and a version, and the rules
for a node that drains (`specification/binary_protocol/PROTOCOL.md` sections 5.4, 11.12a and 11.14; reviewed in
`openspec/notes/UltraGraphDB Spec Consistency Review 2026-11-12.md`, K46 to K53).

## Success

1. **Two roles.** A cluster is compute and storage. No binary, image, crate, settings table or chart entry is named
   `frontend`, and ComputeRead and role kind 1 are retired.
2. **A read takes no hop.** A client's Read, Execute and Query run on the core its connection arrived on, by Scans and
   Frontier requests to the partitions. Every read scenario of fan-out, credit, resume keys and read levels passes
   unchanged; the scenarios of a relayed read end with the relay.
3. **A commit is not held up by a read.** A read gives its core back after every frame, measured by a commit relayed
   between two frames of a merge.
4. **Several compute nodes serve one cluster.** Two compute processes on one host serve clients that find them from
   one seed, spread by tag, follow a node that drains and one that dies, and keep exactly-once through a session that
   moves between them.
5. **Laws are stated as properties.** The node list, the client's list handling and the changed bodies carry Hegel
   property tests, run by `make test`'s property step.
6. **The baseline for the tile store.** The benchmark is rerun after the merge, so stage 6.5's comparison measures the
   tile store and not the hop this stage removes.

## What Changes

- **BREAKING: one role holds the client endpoint and executes reads.** `ultragraphdb_compute` takes the frontend's
  endpoint and keeps its read executor. A read is a task of the endpoint core, bounded by one frame per step.
- **BREAKING: the relay goes.** ComputeRead (523), the choice among compute processes, the timer of a hung one, the
  resend of an unsent read and the relay of a read's Credit and Cancel are removed, with compute's role address.
- **BREAKING: the role's name.** Role kind 1 and the SCRAM username `frontend` retire. The settings tables are
  `[compute]`, `[endpoint]` and `[storage]`; `[frontend]`, `compute.role_address` and instance names beginning
  `frontend` are refused.
- **Finding compute nodes.** A compute node joins with its advertised address, its tags and its state. The node list
  is logged state of the metadata state machine, versioned by the epoch of the close that names its change. A node
  reads it with Roster and sends it to each authenticated client as a Nodes frame.
- **The Rust clients.** Seeds, the list of the highest version swapped whole, a tag per client, no new work on a
  draining node, backoff to a node that cannot be reached, and a return to the seeds when none can.
- **A partition checks a Drop's catalog version,** since a node one close behind may pass what one frontend used to
  refuse for the whole cluster.
- **Deployment.** The supervisor starts the config service, storage and compute. The Helm chart runs compute as a
  StatefulSet without volumes, each pod advertising its stable name, behind a client Service for seeds.
- **Removed, after the user's approval at their tasks:** the `ultragraphdb_frontend` binary and image,
  `ultragraphdb_frontend_tests`, the chart's frontend entries, the `FrontendTable` and the frontend's profiles, once
  their contents live under compute's names.

This change's deltas are written against the capabilities as stage 5.8's change leaves them, so stage 5.8 is archived
before this change's first group starts.

## Capabilities

### New Capabilities

- `node-discovery`: how a compute node joins, the node list as logged state with its version, Roster and the close
  that names a change, the Nodes frame after authentication, the client's list, seeds and tags, and draining.

### Modified Capabilities

- `client-endpoint`: compute serves the clients; requests routed to partitions alone; reads executed in place; the
  relay's requirement removed; Members read for partitions; six requirements renamed.
- `quad-reads`: a read runs on its connection's core, one frame per step, with the client's frame and credit applied
  directly; the relay's wording gone from the transaction, level and batch requirements.
- `role-binaries`: two role binaries and images; the supervisor's order; readiness and draining; nothing named
  frontend.
- `wire-protocol`: Nodes and Roster; Join's and EpochClose's additions; Members for storage alone; ComputeRead and
  role kind 1 retired.
- `system-group`: the endpoint's table for two roles; the node list served; a silent subscriber's connection closed.
- `metadata-group`: the node list among its state; the role's name.
- `sequencer`: a close names a change of the node list; the role's name.
- `schema-changes`: the partition's check of a Drop's catalog version; the role's name.
- `config-service`: the roles, the compute table's address and tags, the retired settings refused.
- `kubernetes-deployment`: compute as a StatefulSet without volumes behind a client Service; no frontend objects.
- `transactions`, `partitions`, `commit-rounds`, `schema-storage`, `term-dictionary`: the role's name in the
  requirements that speak of the client endpoint's role, with no change of behaviour.

## Impact

- **Renamed:** `ultragraphdb_common_frontend` to `ultragraphdb_common_endpoint`, by `git mv`.
- **Rewritten or shrunk:** `ultragraphdb_common_endpoint` (reads executed, the relay gone), `ultragraphdb_common_compute`
  (the executor called in process, its role session and listener gone), `ultragraphdb_compute`,
  `ultragraphdb_compute_tests`, `ultragraphdb_common_system` and `ultragraphdb_cluster_metadata` (the node list),
  `ultragraphdb_cluster_sequencer` (the close's flag), `ultragraphdb_cluster_partition` (the Drop's check),
  `ultragraphdb_protocol_wire`, the three client crates, `ultragraphdb_common_role`,
  `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`, `ultragraphdb_supervisor`,
  `ultragraphdb_tests`, `ultragraphdb_tool_bench`.
- **Deleted, each after approval:** `ultragraphdb_frontend`, `ultragraphdb_frontend_tests`, their images and chart
  entries.
- **Specifications:** `PROTOCOL.md` sections 1 to 13 brought to "compute", ComputeRead's section and the relay's rules
  removed; golden frames and fuzz targets for Nodes, Roster, Join and EpochClose.
- **Documents:** `AGENTS.md`, `ROADMAP.md`, `SECURITY.md`'s tiers, the backlog, the benchmark record.
- **Stages 6.5 and 6.6** are rebased on this change's roles before stage 6.5 starts.
