Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order, as stage 5.8's groups do: (1) the
group's public surface compiling with bare `unimplemented!()` bodies; (2) the full suite written against it and
observed failing for that reason, its run and test count recorded in `notes/group-N.md`: the group's scenarios verbatim
as Gherkin in the owning crate's `features/<capability>/`, bound by its `bdd` target with `fail_on_skipped`, unit tests
for the corner cases enumerated in advance, every law of the surface as a Hegel property test under `tests/property/`
in a `property` target tagged `property-test` (`tdd-protocol.md`, "Phase 2 states every law as a property test"), every
error variant constructed through the public API, and every expected value from a source independent of the code, the
golden frames first; (3) the suite's inputs reviewed against the corner-case enumeration; (4) the implementation,
judged by the suite alone; (5) as the group's last step, `scripts/mutants.sh <crate> --diff <base>` once per crate the
group changed, and for a group decoding bytes from a peer, its fuzz targets and seeds written and built. Before an
existing crate's behaviour changes, its group lifts it into `openspec/protocols/ULTRA_STYLE.md` in a commit of its own.
Tests return `TestResult`; a property test asserts. Bazel is the gate: a group is done when `make test` passes,
cached; the user commits after each group. Removing files waits for the user's approval at the task that removes
them. A weakness found in a specification is fixed in that specification first.

A group that moves code moves its suite with it first: the moved suite is green with the same test count before any
behaviour changes, in a commit of its own.

Waves: 0; then 1 and 2 in parallel; 3 and 4 in parallel; 5; 6; 7; 8; 9. A wave's agents work on disjoint crates, and
the whole repository is gated by `make build` and `make test` when a wave returns.

## 0. Specifications first

- [ ] 0.1 Stage 5.8's change archived, and this change's deltas checked against the capabilities as archived: every MODIFIED requirement equal to the archived text but for this change's edits
- [ ] 0.2 `PROTOCOL.md` brought to this stage's text: sections 1 to 13 saying "compute" for the role holding the client endpoint, section 13.12 and the relay's rules of section 11.12 removed, the "From stage 5.9" paragraphs folded into the sections they amend, section 4.7's rows for Nodes and Roster, no section renumbered
- [ ] 0.3 `SECURITY.md`'s tiers and `AGENTS.md`'s role descriptions brought to two roles
- [ ] 0.4 The user's answers to U2 and U3 of the review recorded in `design.md`; group 7 moved to stage 8 if several compute nodes wait
- [ ] 0.5 The laws of every group enumerated in `notes/laws.md` before its phase 2, each with the property that states it, and the groups that hold none named
- [ ] 0.6 The corner cases of every group enumerated in `notes/corner-cases.md` before its phase 2

## 1. The wire → `ultragraphdb_protocol_wire`

- [ ] 1.1 Scenarios: wire-protocol ("The node list's frames are bodies of the protocol"; the modified "Roles exchange role commands" and "A frame past its stream's credit is refused"), as Gherkin
- [ ] 1.2 Phases 1 to 3: builders and views for Nodes (27) and Roster (537); Join's `state`, advertised address and tags; EpochClose's `nodes`; Members for storage alone; command 523 and role kind 1 retired; the advertised address's form; golden frames `nodes.frame`, `roster.frame`, `roster_reply.frame`, `roster_reply_empty.frame`, `join_compute.frame`, `join_storage.frame` and `epoch_close_nodes.frame` from their generator
- [ ] 1.3 Property tests, written in phase 2: a Nodes and a Roster built from a drawn list within the bounds (1 to 1,024 nodes, one to eight of at most 256 tags) decode to that list and build again to the same bytes, and never pass 524,288 bytes; a list with one drawn rule broken (an address out of order or malformed, a repeated tag, a reference past the tags, `self` past the nodes) is refused with that rule's refusal and its index; an advertised address is accepted exactly when it is `tcp://`, a host of the stated forms and a port of 1 to 65,535
- [ ] 1.4 Phase 4: every body's checks in its "Order" line's order
- [ ] 1.5 After the user's approval: the ComputeRead builder, view, golden frame and fuzz target removed
- [ ] 1.6 Phase 5: fuzz targets and seeds for Nodes and Roster; `scripts/mutants.sh ultragraphdb_protocol_wire --diff <base>`

## 2. Settings → `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`, `ultragraphdb_common_role`

- [ ] 2.1 Scenarios: config-service ("The config service validates every setting before it serves one", its Roles and Compute rules; "A configuration error tells the administrator what to do"), as Gherkin
- [ ] 2.2 Phases 1 to 4: `compute.advertised_address` and `compute.tags`; `compute.role_address` and the `[frontend]` table removed; the role names `compute` and `storage`; the checks Roles and Compute; the profiles rewritten with `compute` instances and passing `ultragraphdb_config_cli`
- [ ] 2.3 Property tests, written in phase 2: over drawn targets, the Compute check refuses exactly those with a compute instance lacking an address, a tag list empty, longer than eight, repeated or malformed, two instances advertising one address, or more than 256 distinct tags
- [ ] 2.4 Phase 5: `scripts/mutants.sh` over the three crates

## 3. The node list → `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_system`

- [ ] 3.1 Ultra Style lift of `cluster_metadata` and `cluster_sequencer`, each in its own commit
- [ ] 3.2 Scenarios: node-discovery ("A compute node joins with what clients need of it", "The node list is a function of the log", "A close tells compute that the list changed"); metadata-group ("The metadata group holds the node list"); sequencer ("A close names a change of the node list"); system-group ("The system group serves the node list" and its endpoint's table), as Gherkin
- [ ] 3.3 Phases 1 to 3: the list's commands and snapshot in the metadata state machine; the close record's flag; `SystemSession` serving Roster and a compute node's Join; the removal proposed after two closes; a silent subscription's connection closed
- [ ] 3.4 Property tests, written in phase 2, as a Hegel state machine against a model written from `PROTOCOL.md` section 11.14: drawn sequences of Joins of both states, connections' closes, closes of epochs and restarts of the group, with the invariants that one version names one list, a version never falls, an entry belongs to the connection of its latest Join, an entry leaves exactly two closes after its connection closed and never while it is open, the list never passes 1,024 nodes or 256 tags, and recovery from any prefix of the log gives the list the model holds
- [ ] 3.5 Phase 4: the above
- [ ] 3.6 Phase 5: fuzz seeds for the frames a `SystemSession` reads; `scripts/mutants.sh` over each changed crate

## 4. A partition's check of a Drop → `ultragraphdb_cluster_partition`, `ultragraphdb_common_storage`

- [ ] 4.1 Scenarios: schema-changes ("A partition checks the catalog version a Drop names"), as Gherkin
- [ ] 4.2 Phases 1 to 4: a Drop of kind 3 or 4 whose `catalog` is not the version in effect at its round's epoch refused with code 2, kind 5, decided at the round as an Ingest share's is
- [ ] 4.3 Property test, written in phase 2: over drawn catalog histories and a Drop decided at a drawn epoch, the Drop commits exactly when its `catalog` is the version in effect at that epoch
- [ ] 4.4 Phase 5: `scripts/mutants.sh` over each changed crate

## 5. The endpoint executes reads → `ultragraphdb_common_endpoint` (renamed), `ultragraphdb_common_compute`

- [ ] 5.1 Ultra Style lift of `common_compute`, in its own commit
- [ ] 5.2 The rename, in its own commit: `ultragraphdb_common_frontend` to `ultragraphdb_common_endpoint` by `git mv`, every dependant updated, its suite green with the same count
- [ ] 5.3 Scenarios: quad-reads ("Compute runs each read on one core", "A read's step is bounded by one frame", "A read's batches fit the client's frame", the modified transaction and level requirements); client-endpoint ("Requests are routed to the partitions that serve them", "Errors reach the client through the error table", "Compute finds the partitions in Members", the renamed requirements), as Gherkin
- [ ] 5.4 Phases 1 to 3: the executor taking a read as a message from `ClientSession` and answering by messages; a core's `RoleLink` to a partition carrying Scans, Frontier requests and Closed beside Bind streams; the client's `max_frame` and credit applied directly; Credit and Cancel acting in the turn they are read; the step bound of design D3
- [ ] 5.5 Property tests, written in phase 2: the merge returns the rows of any drawn partitions' sorted entries in key order, each once, whatever the drawn boundaries of the Entries frames and of the client's frames; a read resumed from the key of any drawn row returns exactly the rows after it; under any drawn interleaving of a read's steps and commits' relays on one core, no commit waits for more than one step
- [ ] 5.6 Phase 4: the above; every read feature of `compute_tests` green against the in-process executor
- [ ] 5.7 After the user's approval: `ComputeSession`'s role session, `CoreListener`, the relay code of `RoleLinks` (compute spread in turn, the hung-compute timer, the resend of an unsent read) and their tests removed
- [ ] 5.8 Phase 5: `scripts/mutants.sh` over each changed crate

## 6. The compute binary → `ultragraphdb_compute`, `ultragraphdb_compute_tests`, `ultragraphdb_supervisor`, `deploy/helm/ultragraphdb`, `bazel/oci_images`

- [ ] 6.1 The scenarios of `ultragraphdb_frontend_tests` moved into `ultragraphdb_compute_tests`, in their own commit, green with the same count against the frontend binary
- [ ] 6.2 Scenarios: role-binaries (all modified requirements); kubernetes-deployment (the modified requirements), as Gherkin
- [ ] 6.3 Phases 1 to 4: `ultragraphdb_compute` running the endpoint and the executor, joining as a compute node, with no role listener; readiness, draining on SIGTERM and the exit for a lost system group; the supervisor's order config service, storage, compute; the development image with two role binaries; the chart's compute StatefulSet without volumes, its headless Service and the client Service; the profiles; `scripts/cluster_local.sh`
- [ ] 6.4 After the user's approval: `ultragraphdb_frontend`, `ultragraphdb_frontend_tests`, the frontend's image, chart entries, `FrontendTable` and profiles removed; `AGENTS.md`'s crate table and the workspace's members follow
- [ ] 6.5 Phase 5: `scripts/mutants.sh` over each changed crate; this group's binaries and deployment hold no law of their own

## 7. Several compute nodes and the clients → `ultragraphdb_client_core`, `ultragraphdb_client_compio`, `ultragraphdb_client_tokio`, `ultragraphdb_common_endpoint`

- [ ] 7.1 Ultra Style lift of the three client crates, each in its own commit
- [ ] 7.2 Scenarios: node-discovery ("A client receives the list once it has authenticated", "A client holds the list of the highest version", "A client is started with seeds and picks nodes by tag", "A draining node is announced before it stops"), as Gherkin: the server's in `common_endpoint`, the client's in `client_core`
- [ ] 7.3 Phases 1 to 3: the endpoint sending Nodes after authentication and on a higher version, with `self`; a draining node's Join and its last Nodes; the client core's seeds, list, tag, choice of a node, backoff and return to the seeds, over `TestServer` extended to several servers; the shells dialling listed addresses
- [ ] 7.4 Property tests, written in phase 2, as a Hegel state machine in `client_core` against a model: drawn sequences of Nodes frames of any versions from any connection, connections refused and restored, and requests begun, with the invariants that the list held is the highest version received and never goes back, no transaction begins on a node the list names draining or names no longer, every node chosen carries the client's tag, a client with any reachable serving node of its tag is never refused, and the seeds are dialled only while no listed node can be reached
- [ ] 7.5 Phase 4: the above
- [ ] 7.6 Phase 5: fuzz seeds for the Nodes frame as a client reads it; `scripts/mutants.sh` over each changed crate

## 8. The cluster end to end → `ultragraphdb_tests`, `ultragraphdb_tool_test`

- [ ] 8.1 Every client-facing feature green with its scenarios unchanged, the count before and after recorded; each edited scenario listed with the step that named the relay or the frontend
- [ ] 8.2 Two compute processes and one storage process on one host: node-discovery ("Two compute nodes serve one cluster", "A drained node takes no new transaction"); a session used through both nodes; a resend on the second node while the original is on its way from the first, answered code 14; a node killed, its entry gone two closes later and its clients served by the other; a Drop through a node one close behind refused by the partition
- [ ] 8.3 A read and a commit on one core: a commit's relay measured beside a merge over four partitions, recorded with the bound of design D3
- [ ] 8.4 The development image booted once per architecture, amd64 under emulation, reaching `/ready` with two roles
- [ ] 8.5 The search for `frontend` across the workspace's members, images, chart and profiles finding nothing

## 9. The benchmark and the exit gate → `ultragraphdb_tool_bench`, `openspec/benches/`

- [ ] 9.1 The benchmark of `openspec/benches/benches-2026-11-09.md` and the read record of stage 5.8 rerun on this change exactly as recorded, in process and across two virtual machines
- [ ] 9.2 The run recorded as a dated baseline, every write and read figure beside stage 5.8's with the change in absolute and relative terms and whether it lies outside both runs' spread; named in `ROADMAP.md` as the baseline stage 6.5 compares against
- [ ] 9.3 Each regression outside the spread root-caused and fixed, or recorded with its cause and the user's decision
- [ ] 9.4 The exit gate: `ROADMAP.md`'s stage 5.9 row marked done with the headline figures; this change archived; the change sets of stages 6.5 and 6.6 rebased on two roles and the system group before stage 6.5 starts
