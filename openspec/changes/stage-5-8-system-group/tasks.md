Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order, as stage 6.5's groups do: (1) the
group's public surface compiling with bare `unimplemented!()` bodies; (2) the full suite written against it and
observed failing for that reason, its run and test count recorded in `notes/group-N.md`: the group's scenarios verbatim
as Gherkin in the owning crate's `features/<capability>/`, bound by its `bdd` target with `fail_on_skipped`, unit tests for the corner cases enumerated in advance, every law of the surface as a Hegel property test under `tests/property/` in a `property` target tagged `property-test` (`tdd-protocol.md`, "Phase 2 states every law as a property test"), every error variant constructed through the public API, and every expected value from a source independent of the code, the golden frames first; (3) the suite's inputs reviewed against the
corner-case enumeration; (4) the implementation, judged by the suite alone; (5) as the group's last step,
`scripts/mutants.sh <crate> --diff <base>` once per crate the group changed, and for a group decoding bytes from a
peer, its fuzz targets and seeds written and built. Before an existing crate's behaviour changes, its group lifts it
into `openspec/protocols/ULTRA_STYLE.md` in a commit of its own; the crate this change creates conforms from its first
commit, is created in full by the group that builds it, and gets its row in `AGENTS.md`'s crate table. Tests return
`TestResult`. Bazel is the gate: a group is done when `make test` passes, cached; the user commits after each group.
Removing files waits for the user's approval at the task that removes them. A weakness found in a specification is
fixed in that specification first.

A group that moves code moves its suite with it first: the moved suite is green with the same test count before any
behaviour changes, in a commit of its own (design, *Risks*).

Waves: 0; then 1 and 2 in parallel; 3; then 4, 5 and 6 in parallel; 7; 8; 9. A wave's agents work on disjoint crates,
and the whole repository is gated by `make build` and `make test` when a wave returns.

## 0. Specifications and the baseline first

- [x] 0.1 `PROTOCOL.md` sections 11.12a and 13.23 to 13.28 consistent with the other specifications by the review of `openspec/notes/UltraGraphDB Spec Consistency Review 2026-11-12.md`
- [ ] 0.2 `PROTOCOL.md` brought to this stage's text, with the mechanical corrections the review left for this task (its *Mechanical corrections*, not applied), as its preamble has each stage's change do: sections 3, 4.8, 11.1, 11.5, 11.9, 11.11, 11.12, 11.13 and 13.1 to 13.9 rewritten in place for the system group as the books' host, "From stage 5.8" paragraphs folded into the sections they amend, section 11.12a keeping what stage 5.9 changes; no section renumbered
- [ ] 0.3 `SECURITY.md`'s tiers and `AGENTS.md`'s role descriptions brought to the system group hosted by storage
- [ ] 0.4 The read baseline: the read workloads of `ultragraphdb_tool_bench` run on the stage 5.7 cluster exactly as `openspec/benches/benches-2026-11-09.md` ran its writes (hosts, cores, data sizes, repetitions), recorded as `openspec/benches/benches-YYYY-MM-DD.md` with the commit, the commands and the environment
- [ ] 0.5 The laws of every group enumerated in `notes/laws.md` before its phase 2, each with the property that states it, and the groups that hold none named: groups 4, 6 and 7 wire binaries and deployment and hold none of their own
- [ ] 0.6 The corner cases of every group enumerated in `notes/corner-cases.md` before its phase 2: per decoder its refusals in its body's order, per state its boundary conditions (a full registry, a mark across a closed connection, a check during a sync, a view behind a close)

## 1. The wire → `ultragraphdb_protocol_wire`

- [ ] 1.1 Ultra Style lift of `protocol_wire`, in its own commit
- [ ] 1.2 Scenarios: wire-protocol ("The system group's commands are bodies of the protocol"; the modified "Roles exchange role commands" and "Data requests, prepared statements and describe"), as Gherkin
- [ ] 1.3 Phases 1 to 3: builders and views for Session (534), Check (535), Ended (536), Declare (538) and Defaults (539); the command table's rows, 540 kept and refused; Subscribe's and CloseRequest's stage 5.8 fields; golden frames `session.frame`, `check.frame`, `check_reply.frame`, `ended.frame`, `declare.frame`, `declare_drop.frame`, `declare_reply.frame`, `defaults.frame`, `defaults_reply.frame`, `challenge.frame`, `challenge_reply.frame`, `verify.frame` and `verify_reply.frame` from their generator, committed with its source
- [ ] 1.4 Phase 4: every body's checks in its "Order" line's order; a builder refusing what its decoder refuses
- [ ] 1.5 Phase 5: fuzz targets and seeds for the five new decoders; `scripts/mutants.sh ultragraphdb_protocol_wire --diff <base>`
- [ ] 1.6 Property tests, written in phase 2: each of Session, Check, Ended, Declare, Defaults, Challenge and Verify built from drawn fields decodes to those fields and builds again to the same bytes; a body with one drawn rule broken (an unsorted or repeated session, a nonzero padding byte, an outcome outside its set, a section of a broken length) is refused with that rule's refusal and its index; a Defaults answer cut at any drawn frame size reads back as the same predicates in order

## 2. Settings → `ultragraphdb_common_service_config`, `ultragraphdb_common_config_check`, `ultragraphdb_common_role`

- [ ] 2.1 Ultra Style lift of the three crates, each in its own commit
- [ ] 2.2 Scenarios: config-service ("The config service validates every setting before it serves one", its System and System address rules), as Gherkin
- [ ] 2.3 Phases 1 to 3: `SystemTable` (`role_address`, `data_dir`); `cluster.system_address` in place of `cluster.frontend_address`; `frontend.role_address` and `frontend.data_dir` removed; the checks System, System address and Fixed; `common_role::system_address` in place of `frontend_address`
- [ ] 2.4 Phase 4: the table, the checks, the profiles under `ultragraphdb/ultragraphdb_config/profiles` and `deploy/helm/ultragraphdb/profile` rewritten and passing `ultragraphdb_config_cli`
- [ ] 2.5 Property tests, written in phase 2: over drawn targets of one frontend, any compute and one to eight storage instances, the System check accepts exactly the targets with one `[system]` table holding both settings and both moved tables, and the System address check exactly those whose scheme, port and path agree
- [ ] 2.6 Phase 5: `scripts/mutants.sh` over the three crates

## 3. The system group → `ultragraphdb_common_system` (new), `ultragraphdb_common_frontend`

- [ ] 3.1 Ultra Style lift of `common_frontend`, in its own commit
- [ ] 3.2 The move, in its own commit: `Owner`, its turn, its answers while syncing, its catalog and check code, `OwnerHandle`, `Asked`, `ReportAsked`, `DropStep`, `start_owner`, `OwnerStart`, the owner thread, the snapshot file, `RoleSession`, `Membership` and `utils/declarations.rs` moved into `common_system` under the names of design D1, with their tests and features; the suite green with the same count; `common_frontend` depending on `common_system` until group 5
- [ ] 3.3 Scenarios: system-group (all requirements but "The hosting process starts the group first and stops it last" and "A lost system group is waited for until the deadline", which run in groups 4 and 8); metadata-group and sequencer (the modified and added requirements); schema-changes ("A restarted host of the system group takes a check up again"), as Gherkin
- [ ] 3.4 Phases 1 to 3: `SystemSession` serving Challenge, Verify, Session, Check, Ended, Declare and Defaults beside the nine commands `RoleSession` served; marks held per connection; a connection's role from its SCRAM username
- [ ] 3.5 Phase 4: the registry asked by Session, Check and Ended with the outcomes' precedence; marks ended by Ended, kept `transaction_ms` past their connection's close, and the group's quiet time after it begins to listen (`PROTOCOL.md` section 11.12a); Check answered from the registry as decided while a log syncs; Declare of kinds 1 and 2 with the stream held open; Defaults in order with continuation frames; Challenge and Verify with no stored key in any reply; Subscribe starting at the latest close for the client endpoint's role; a Catalog request of `version` 0 naming an epoch; CloseRequest with `partition` 0
- [ ] 3.6 Property tests, written in phase 2, as Hegel state machines against a model written from `PROTOCOL.md` section 11.12a: the registry under drawn sequences of Session, Check of both kinds, Ended, a connection's close, time passing and the group's restart, with the invariants that a marked session is never evicted, the registry never holds more than `sessions_max`, every outcome follows the order 12, 21, 14, a mark of a closed connection ends at `transaction_ms` and not before, and no check of kind 1 passes within `transaction_ms` of a start; the catalog under drawn sequences of Declare of both kinds and partitions' reports, with the invariants that versions are numbered without gap, a stale base is refused, and the version a Catalog request of `version` 0 and an epoch returns is never one being checked or withdrawn
- [ ] 3.7 Phase 5: fuzz seeds for the frames a `SystemSession` reads; `scripts/mutants.sh ultragraphdb_common_system --diff <base>`

## 4. Storage hosts the group → `ultragraphdb_common_storage`, `ultragraphdb_storage`, `ultragraphdb_storage_tests`

- [ ] 4.1 Ultra Style lift of the crates group 3 did not lift, each in its own commit
- [ ] 4.2 Scenarios: system-group ("One storage instance hosts the system group", "The system group runs on a core of its own over logs of its own", "The hosting process starts the group first and stops it last"); partitions ("A storage process hosts partitions over bucket ranges"); role-binaries (the storage parts of "Readiness, health and shutdown", "Each role runs one runtime per core", "Configuration is explicit and validated at start"), as Gherkin
- [ ] 4.3 Phases 1 to 3: the storage service starting `start_system` on a runtime of its own when its settings hold `[system]`, before its partitions; partitions dialling `cluster.system_address`, co-hosted ones included; readiness and the stop order; exit 74 for a failed log of the group, and no exit for a lost peer in the hosting instance
- [ ] 4.4 Phase 4: the above; `storage_tests` starting a hosting and a non-hosting instance
- [ ] 4.5 Phase 5: `scripts/mutants.sh` over each changed crate

## 5. The frontend without books → `ultragraphdb_common_frontend`, `ultragraphdb_frontend`, `ultragraphdb_frontend_tests`

- [ ] 5.1 Scenarios: client-endpoint ("The endpoint asks the system group", "An endpoint core keeps a view of what the closes announce", "The frontend finds the roles in Members", the modified "Connections authenticate against the metadata group's principals"); transactions ("Exactly-once commits through sessions"); quad-reads ("A read's transaction names its level"); schema-changes ("Drops", "A tightening is checked before it is acknowledged"); role-binaries (the frontend parts), as Gherkin
- [ ] 5.2 Phases 1 to 3: `SystemLink`, a core's multiplexed connection to the group with its subscription; `EndpointView` (epochs, placement and tenants, catalog versions, schema ids); the Check and Ended frames of a turn; `ClientSession` asking through the link where it asked `OwnerHandle`; authentication relayed by Challenge and Verify, `AskedCredentials` gone; `AuthThrottle` held by the first core and asked by message; Members read at the events of design D9; `RoleDouble` extended to play the system group
- [ ] 5.3 Phase 4: the above; what the frontend answers while the group is unreachable; a declaring Commit and a Drop through Declare; Describe from Catalog and Defaults; the early close by CloseRequest
- [ ] 5.4 Property tests, written in phase 2: the view under drawn sequences of closes, Declare replies and Catalog reads never lowers its latest closed epoch or a tenant's current version; a Begin naming a token is answered exactly when the close before the token's epoch has arrived or its deadline has passed; a turn's frames hold each session at most once whatever requests a drawn turn holds, Endeds before Checks; the authentication throttle, as a state machine over drawn failures, successes and time, delays and bans by its thresholds alone and counts nothing for an Error 17
- [ ] 5.5 After the user's approval: `JoinFanOut`, `CoreJoins`, the frontend's role listener (`accept_roles`) and `common_frontend`'s dependency on `common_system`, `cluster_metadata`, `cluster_sequencer`, `traits_log` and `transactions_log` removed
- [ ] 5.6 Phase 5: `scripts/mutants.sh` over each changed crate

## 6. Compute dials the group → `ultragraphdb_common_compute`, `ultragraphdb_compute`, `ultragraphdb_compute_tests`

- [ ] 6.1 Ultra Style lift of `common_compute`, in its own commit
- [ ] 6.2 Scenarios: role-binaries (compute's parts of "Readiness, health and shutdown" and "Exit codes name the cause"); system-group ("A lost system group is waited for until the deadline", compute's part), as Gherkin
- [ ] 6.3 Phases 1 to 4: compute joining and reading Members, Placement and Catalog at `cluster.system_address`; its deadline and exit for a lost group
- [ ] 6.4 Phase 5: `scripts/mutants.sh` over each changed crate

## 7. The supervisor and the deployment → `ultragraphdb_supervisor`, `ultragraphdb_supervisor_tests`, `deploy/helm/ultragraphdb`, `bazel/oci_images`, `scripts/cluster_local.sh`

- [ ] 7.1 Scenarios: role-binaries ("The supervisor starts, stops and reports the roles", "Roles meet through the transport at their addresses"); kubernetes-deployment (the four modified requirements), as Gherkin
- [ ] 7.2 Phases 1 to 4: the start order config service, storage, compute, frontend; the bootstrap password drawn for the hosting storage instance; the development profile's `[system]` table and `/data/system`; the chart's frontend Deployment without a volume, the bootstrap secret mounted into the hosting storage pod alone, `cluster.system_address` naming that pod; the local cluster script
- [ ] 7.3 Phase 5: `scripts/mutants.sh ultragraphdb_supervisor --diff <base>`

## 8. The cluster end to end → `ultragraphdb_tests`, `ultragraphdb_tool_test`

- [ ] 8.1 Every client-facing feature of `ultragraphdb_tests`, `frontend_tests`, `storage_tests` and `compute_tests` green with its scenarios unchanged, the count before and after recorded; each edited scenario listed with the step that named the books' host
- [ ] 8.2 Scenarios: system-group ("A lost system group is waited for until the deadline"); client-endpoint ("A restarted frontend serves the sessions it served before"); kubernetes-deployment ("The hosting storage pod killed keeps the books", "A deleted frontend pod loses nothing"), as Gherkin over the built binaries
- [ ] 8.3 The failure runs: the hosting storage process killed with commits in flight at a partition of another storage process; the frontend killed during a tightening's check; the frontend's connection to the group closed and dialled again with requests at partitions; a frontend started before storage; each asserting exactly-once through the session's resend
- [ ] 8.4 The development image booted once per architecture, amd64 under emulation, reaching `/ready` in the new order

## 9. The benchmark and the exit gate → `ultragraphdb_tool_bench`, `openspec/benches/`

- [ ] 9.1 `openspec/benches/benches-2026-11-09.md` and the read baseline of task 0.4 rerun on this change exactly as recorded, in process and across two virtual machines
- [ ] 9.2 The run recorded as a dated baseline in the 2026-11-09 file's structure, with a table of every write and read figure before and after, the change in absolute and relative terms, whether it lies outside both runs' spread, and what a commit's Check costs per turn
- [ ] 9.3 Each regression outside the spread other than the Check's round trip root-caused and fixed, or recorded with its cause and the user's decision
- [ ] 9.4 The exit gate: `ROADMAP.md`'s stage 5.8 row marked done with the headline figures; `AGENTS.md`'s crate table and role descriptions current; the architecture's path item 1 closed; stages 6.5 and 6.6 noted as rebased on this change's roles
