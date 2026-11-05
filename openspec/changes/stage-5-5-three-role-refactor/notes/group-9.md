# Group 9: role foundation and storage

## 9.0 `ultragraphdb_common_service_config`

One type per table, one field per setting. The tables hold what the specs name today, and a later task adds a setting where it
needs one. The crate checks a value's kind and that it fits its field; a bound stays with the constructor of the domain type it
configures. Mutants: 125 caught, 13 unviable, none missed.

## 9.1 The binaries and their test harness

The three binaries hold `main` alone, over `ultragraphdb_common_service`. They run one runtime, serve `/health` and `/ready`, and stop
on a signal. Until its role exists, storage reports ready, having no partition to recover, and the frontend and compute report 503,
having nothing that could answer them. Roles join in with their tasks.

`ultragraphdb_tests` starts the binaries as processes, each copied into the sandbox with `copy_file`: 7 scenarios and 14 steps, cached by
Bazel on a second run, tagged `exclusive` and `integration_test`, with no sandbox opt-out in the test action. They start through `service_utils` 0.4, the same tool scrith uses, which is `std` alone and reads a process's exit code, as the `sysexits` scenarios need.

Fuzz target `health_request`: 376,125 executions in 30 s, coverage 172, no failure. Its 6 seeds are hand-written requests.

**Images.** Each binary's `BUILD.bazel` declares its `glibc_multiarch_img`, on the base the server ships on, its `sha256_tag`, and an
`image_push` to `index.docker.io`, `hansenmarvin/ultragraphdb_<role>`, tagged by the content tag. Built, never pushed.

**Pinning.** Only Linux can read a binding back. On macOS a role asks for nothing, since compio would set only a cache-locality hint, and
logs that it runs unpinned; that is the path verified here. The Linux path has run nowhere yet.

**Mutants.** `error_role`: 3 caught, 2 unviable. `common_service`: 53 caught, 9 unviable, 9 missed on the second pass, before the test that
kills `delete !` in `serve`. The remainder change nothing the crate's own suite can observe on macOS:

- `run_service.rs`, `stop_flag -> Ok(Arc::new(..))`: installs no handler. The process scenario "A signal stops a role cleanly" kills it, in
  `ultragraphdb_tests`; a unit test would have to signal its own test process.
- `thread_allowed` (four mutants) and `role_runtime`'s `== Some(true)`: reach `/proc`, which macOS lacks. To run again on Linux.
- `read_request`, `count > 0` to `true` and to `>=`: a peer that closed reads 0 bytes until the loop's bound, and gets the same 400. It
  only takes longer.

## 9.2 Role connections

`ServerConnection` takes an `Endpoint`, the table of `PROTOCOL.md` §11.12: the client endpoint as before, the frontend's, a partition's
and a compute process's role addresses. A role command opens a stream of its own, a Bind a transaction's, and a partition takes Commit and
Drop only on a Bind's stream. Whatever the endpoint does not serve is refused as `CommandNotServed` and closes the connection.

`ultragraphdb_common_role` holds what every role does on these connections and `ultragraphdb_common_frontend` the frontend role.
`run_service` takes a role's work as a future, which sets its readiness; a failure of that work ends the process with its code. Storage
and compute listen, dial, authenticate, Join, read the Members, and retry until `[cluster] join_deadline_ms`, then exit 69. The frontend
connects back to each role that joined, and is ready once it has reached a partition.

`ultragraphdb_tests`, 11 scenarios: the start order to every role's `/ready` over TCP and over Unix sockets at mode 0600, the frontend's
Members naming both roles, a wrong secret refused and retried until exit 69 with nothing joined, the role secret refused at the client
endpoint with a wrong password's error, a missing secret exiting 78 naming the file.

Two settings joined `[cluster]`: `id`, the identity every frame's header names, which §2 says each role is configured for and no setting
held, and `join_deadline_ms`, 60 s by default. `PROTOCOL.md` §11.12 now states that a role connection has no idle limit, since a role reading
a Subscribe's closes sends nothing.

What the later tasks replace: the client endpoint knows no principal and serves clear text alone until 9.3 and 9.5.5, every command an
endpoint serves beyond Join and Members is refused as unavailable until its host exists, and storage is ready once joined until 9.4 opens
its partitions. A role drops its connection to the frontend after joining; 9.3's Subscribe holds one.

## 9.3 The frontend's hosts

`Owner<M, S>` holds the metadata group, the sequencer and who joined on one core, generic over `OrderedLog`, with `Wal` today's log.
It runs on a thread and runtime of its own; endpoint cores ask it through `OwnerHandle`, by a message and a one-shot answer, and keep
nothing it decides. A turn takes everything asked, syncs once, answers what that sync made durable, and closes an epoch that is due. A
close is durable in the sequencer's log before the group learns it; a close the group never learned, because the process stopped
between the two syncs, is learned at recovery. A first start forms the even placement from `[cluster] partition_count`; a restart keeps
the map it holds, warns when the setting disagrees, and bounds claims by the map.

`serve_connection` now takes a `Session`, whose `next_push` sends what no request asked for. The read stays pinned across pushes, so a
frame is never cut short. The connection splits through `Splittable`, which `Carrier` implements as two handles on one socket:
`compio::io::split` guards the stream with a lock its pending read holds, so a push deadlocks behind it.

`RoleSession` serves Join, Members, Placement, Catalog, Lease, Subscribe and CloseRequest. `ClientSession` asks the owner for a verifier on
every authentication: `ScramServer::name_in` reads the name out of the first message, and the machine, which cannot wait, looks up what
came back. `RoleError` gains codes 70 and 74, so a failed log stops the frontend with 74.

The owner stores a snapshot, written beside, synced, renamed and its directory synced, before it trims a log through it: the metadata
group when its `snapshot_rounds` say, the sequencer every 4,096 closes.

`PROTOCOL.md` §13.3 now states that a connection holds one subscription.

Scenarios: `ultragraphdb_common_frontend`, 9, the owner on its thread over real logs; `ultragraphdb_tests`, 13, adding five closes in order
on a Subscribe and a Lease and a Placement over the wire.

What later tasks replace or add: DefaultEntry and Report are refused as unavailable; the frontend's connections all run on one endpoint
core; no setting creates the bootstrap principal, so no client can authenticate at a running binary until 9.5.5; a TLS stream needs a
`Splittable` of its own in 9.5.5; `RoleClient::request` reads the next frame as its reply, which 9.4's storage replaces where a Subscribe
shares the connection; an EpochClose is always one frame, its lists holding at most 4,352 records of 16 bytes.

## 9.4 Storage's host

**The frame write path.** `ChangeRef` is a change with its terms borrowed, and `Changes` what a write applies, in order. A `Changeset`
lends its terms and `FrameChanges` lends a Write frame's, so `Transaction::write`, `Partition::references` and `ChangesetWrite::apply` take
either through one path. `FrameTerms` validates the table once and owns only what text cannot lend: a triple term, and a language tag
not in RDF's lowercase. A declaration in a Write is refused by rule, since it commits at the metadata group and never reaches a partition.
The scenario "A frame and a changeset give one record" proves the two records byte-identical. `WriteSetRecord::encode_into` writes into
the log's buffer with no copy between.

**The host.** `ultragraphdb_common_storage` runs each partition on a thread and runtime of its own. One task on each core owns the
`PartitionHost`; a connection hands it each request by message, its frame whole, and waits for the answer, so nothing on the write path
is shared. A Write whose references are unanswered is held with its frame until they are, and that holds its connection, bounded by the
transaction's 10 s limit. A Commit's reply comes later as a pushed frame, once durable.

`serve_connection`'s `Session::answer` takes the `ReceivedFrame` by value. `run_service` passes the role `Stopping` and waits for a role still
working: at a stop each core aborts what is open, runs a last turn and syncs its store, so a restart replays nothing.

A partition's address comes from `[storage] role_address`: over TCP the port plus the partition's place among the process's partitions,
over a Unix socket `-p<partition>` before the extension. Each core joins for itself and subscribes from its own last close, which recovery
reads as one below the epoch of its last applied commit. The placement is read from the frontend and checked against the even map this
stage holds fixed. `[storage] data_dir` is required.

Registrations reach another process's home by Reference, on connections each core keeps per address. `PROTOCOL.md` §13.15 holds `tenant`
in the command header: a home resolves an IRI in its tenant's dictionary. A partition's mailbox hands what it
cannot deliver in its process to its shell as a `RemoteMessage`.

`RefillBlocks` hands out what `LeasedBlocks` holds and notes each type taken, and the core leases ahead. `LeasedBlocks` keeps its existing
`Mutex`, taken once per block on the one core holding the partition, so never contended. Every other crossing is a message.

Scenarios: `ultragraphdb_tests`, 17, adding two processes sharing the partitions, a reference crossing processes, a flush on shutdown and
recovery before `/ready`. `ultragraphdb_cluster_partition`: a vanished client's transaction and a closed connection's, on the simulated host.
A declaration in a Write is refused at the partition, so a vector index never reaches one; the frontend's own refusal naming the predicate
arrives with 9.5.5.

What later tasks replace or add: a Drop is refused as unavailable; a close announcing a catalog version is
delivered without its version read, since nothing offers one until the frontend's endpoint; 9.5.5 relays a client's frames to Bind.

## 9.4b The config service

**9.4b.1 HTTP.** `ultragraphdb_transport_http_server` and `_client`, one request per connection, bounded in size and time, with no keep-alive and no chunked body, so there is no framing two parsers could read differently: a `Transfer-Encoding` or a second `Content-Length` is refused. A request owns the buffer it arrived in, so its body moves on with no copy. `/health` and `/ready` moved onto the server as `HealthService`, the old responder's files and its fuzz package moved by `git mv`. Each crate has a fuzz target, `http_request` and `http_reply`, with hand-written seeds.

**9.4b.2–9.4b.4 The validator and the service.** `ultragraphdb_common_config_check` checks a target whole and reports every defect; an instance whose file is refused holds partitions no one can read, so partition coverage stands aside rather than accuse the rest. `ultragraphdb_common_config` owns its `OrderedLog` in one task reached by message. A revision seeded from the central file is one record, revision and activation together, so a crash cannot leave it stored and inactive. An activation is checked against the active revision as an addition is, or an identity could change through two revisions added while nothing was active. `ultragraphdb_config_tests` is the sister crate.

**9.4b.5 Roles take their settings from it.** `ServiceConfig` reads bytes and holds no knowledge of a file or a variable. A wait for the service watches the stop flag, since the signal handlers are installed by then and would otherwise leave a role deaf to SIGTERM for its minute. The harness starts whole clusters through the real service; a scenario starting a partial or broken target uses a double serving a file per instance, since the service rightly refuses to hold those. "The service is down" runs in-process with a short patience, so the 60 s constant itself is not exercised.

**9.4b.6 Loading a revision.** `listen` moved to `ultragraphdb_common_service`, since the admin listener needs its removal of a stale socket across an `exec`. A load's answer gets 50 ms to reach the wire before the role stops. `ULTRAGRAPHDB_CONFIG_REVISION` is set by the role on itself, so a supervisor's restart, holding the original environment, returns to the active revision. "Two releases share a revision" needs two releases to run, so the nearest scenario here is two instances reporting one revision.

**The specs crates removed.** With settings in the service, the three `*_specs` crates held only what one test crate used: binary names, test ports and settings. Their contents moved to `ultragraphdb_tests`' `one_host`, and real addresses live in the profiles and central files.

**The download.** `GET /v1/targets/{target}/download` answers with the active revision's files as received, which `POST .../revisions` takes back as they are, so an edit made through the API can be saved. It serves a retired target too, whose revisions stay stored.

**The restore.** `ultragraphdb_config_cli restore` takes each target's latest revision, so a folder holds one version of every file, and says when that is not the active one. It writes through `write_central`, the inverse of `read_central`, in the validator's crate so the crate reading a folder also says what one looks like. Each target has a folder of its own, since flat names can collide: target `a_storage` with key `storage`, and target `a` with key `storage_storage`. Names come from a peer, so they are checked before a file is written. The token is an argument, whose exposure and handling `specification/SECURITY.md` states under "Securing the CLI".

**A service binary is `main` and `service`.** The role entries moved from the common crates into each binary's `service.rs`, as the config service's start did, and what they reach in `ultragraphdb_common_storage` and `_frontend` became public, with private fields and constructors. Compute's one unit test, that it names its absent address, became a process scenario. Each binary then took a test-only sister crate, `ultragraphdb_frontend_tests`, `_compute_tests` and `_storage_tests` beside `_config_tests`, holding its scenarios as processes with no container; `ultragraphdb_tests` keeps the harness they share and the whole-cluster scenarios. Compute gained scenarios for its readiness, its refusal of a command it does not host, and its refusal of another secret.

**9.5.1 The wire.** DirectWrite, ConditionalWrite, Scan's term table, Entries' `continues` and object records are on the wire, and no role executes them yet. Writing them exposed holes in the protocol's text, so it was strengthened before the code followed. A value marked as continuing must hold an entry and the next frame must continue its key, so a value promised to continue cannot stop. `LAST` falls exactly on the last range's end, so an answer ending early cannot read as whole. A one-frame write refuses any name, since only a declaration names one, so a receiver rebuilds each frame byte for byte. A Scan naming a transaction states `at` 1, `epoch` 0 and IRIs alone in its frame. A request number is at least 1 with a session and 0 with none, on Commit and Drop too. Only Result and Entries frames spend credit, so Scan's answers wait on credit, which 9.5.3 and 9.5.4 now state. A later Welcome is refused. `EntriesSequence` holds what compares frames, in the wire crate beside `StatementCache`, for compute to hold per stream; refusing one closes the connection, as any side that dialled does. "A DirectWrite is relayed unchanged" waits for 9.5.5.

**9.5.2 Transactional scans.** The kernel is one walk, `walk_lists`, over a `ListSource`: `Lists` adds no key, and a `Request` adds its written keys and answers with its writes on top, so a transaction's scan and a plain one cannot disagree about a key's row. `Request::span` replaces the eager `Request::scan`; its one caller wanting keys alone gives a visitor that reads no list. A span holds nothing and marks nothing read whole, and spans of one range merge, so 10,000 lists cost one range. `Transaction::scan_into` checks room after the walk and before returning, dropping the rows if the span does not fit. Conflict kinds now report an absent term before a range: a read of a term nobody holds records both, both are true, and the absence is the more precise reason. The store has no range scan over its keyspace without versions, so the dictionary is read by key: `Transaction::dictionary_into` over `WriteTerms`, which suits a lookup being a range holding one key. So a Scan's range of the dictionary holds one key, and §13.13 refuses a wider one: the keys are hashes and IDs, whose order means nothing to a reader, and the keyspace's contract is the point lookup its bloom filters and cache serve. `PartitionWorkload` now reads every partition's forward lists by a transaction's scan at closed epochs and compares them with the reference model.

**9.5.3 The read kernel.** `PartitionCore` answers Scan, Closed and Recall. A Scan is checked whole at arrival, in the frame's order, its keys copied so no receive buffer stays held, then lives as an `OpenScan`: waiting on its epoch, on its transaction's references, queued, or parked until its last frame has left. One frame is built per turn for the whole partition, in turn, so no stream's backlog stands in another's way and a stalled reader holds one frame. `Ready` is what a frame leaving says; design.md states how it travels. The version is chosen once, when any wait ends, and pinned by a `ReadPin` unless a transaction already holds it; every frame reads it. `scan_version` is pure, and a version within a closed epoch reads what was applied at or below it, so nothing reads above the watermark, where `Lists::gate` would block the core's thread. `Version::bound_of` is the bound's one home, `Fanout` included.

The core is a task on a runtime whose `block_on` polls it after every driver poll, with a waker that notifies the driver. While a Scan is queued the loop yields once instead of waiting out its tick, which is what lets the connection take the frame and the next `Ready` arrive without a millisecond lost per frame.

A foreign term follows the path a read of it already had: `Partition::read_references`, `PartitionHost::ask`, `finish_bound`, and `take_bound_ready`, which `resume_ready` now reads for Scans as well as Writes. `resolve_iri` answers a foreign IRI by the partition's copy or by what `adopt` gave it, and with neither the dictionary refuses, since `Dictionary::resolve` creates no ID in a bucket it does not own. That refusal also keeps `objects_into` from minting one, and a test holds it there. `take_bound_refused` names each transaction a home's refusal ended, with why, and the request waiting and the Bind stream carry it, a parked Write's included. A foreign skolem IRI whose home holds none answers no entry and leaves the transaction open, as one of the partition's own does, so placement decides nothing a read can see; the transaction notes it, so its reads do not ask again. §13.15's outcome 7 is what says so, and it also covers a home's store failing, which a read of a foreign skolem IRI takes for absence, the reply having no other outcome to say it with. Scans of a transaction waiting on a home wait together, each asking for its own terms once the answers before it came; only a Write or Operation waiting so refuses a Scan, since only that breaks the order the frontend keeps.

`walk_lists` reads the next key's list once past the limit, so a range holding exactly `limit` rows is exhausted and `end` 2 means a key with something visible remains, within 256 keys holding nothing: past those it stops looking, and a continuing Scan finds the range ended. A key whose list holds nothing at the version stays no row. `error_reply` states what §9.4's table says: a conflict's kind, `TooLarge`'s limit in bytes, code 4 and its limit for a transaction past it, the request for `NotCommitted`, code 3 for a read below what is retained, and a request of an ended transaction as a protocol error instead of `TimedOut`.

Referrer sets are versioned: their keys are ordinary versioned store keys, `values_page` reads them through a snapshot at the Scan's version, and the scenario "A dictionary and a referrer set are read by Scan" reads one absent at a version before its registration.

On the wire `deadline_ms` is 0 when `epoch` is 0, since it bounds only the wait for a close; once that is over `storage.scan_stall_ms` bounds a Scan, 30,000 ms by default. `Transaction::begin`'s check of a named version is left as it is: bound transactions name none, and this stage has no client path to it, so a read-only transaction's named version reaches the partition as a Scan with `at` 4 and no `Transaction`.

Scenarios: `ultragraphdb_storage_tests`' `read_kernel.feature`, over a real storage process beside a real frontend, decoded with `EntriesSequence`, `EntriesView` and `ObjectRecords`, adding the tenant's rule, a range holding exactly its limit, a transaction's own write with its conflict of kind Range, a Scan of what is not its transaction's, a repeated Bind, a foreign term across processes, and a home played by the test itself, which holds its answers or refuses, for what no real home can be made to do on demand. `ultragraphdb_cluster_partition`'s `foreign_terms.feature` takes the two scenarios of a foreign term on two simulated partitions: one partition alone cannot tell an ID its home gave from one it minted. `set_discard_ts` has no production caller yet, so a pin holds what nothing would discard today, and a read below a discard is proved at the store's level.

What later tasks replace or add: 9.5.4 is the first caller of Closed and of every Scan path and keeps `EntriesSequence` per stream; a Drop's hosting is 9.5.5's, as a Bind of kind 2 is refused as not hosted yet, and its resend goes as a first Drop does, its stored reply still a Drop frame in Recall; a record too large for any frame is code 6, and only a dictionary value reaches that refusal in a test, since `statements_per_object_max` keeps a list's record within a floor frame; a run of one-key dictionary ranges in one frame, the reverse postings' statement IDs, the statements keyspace and the resumed `after` are 9.5.3b's; the frontend's own order of a transaction's requests is 9.5.5's, so a Scan arriving while a write's references are unanswered is refused here; mutation testing of all of this is 9.6.

**9.5.3a The tenant rename.** The u64 the code called namespace is the tenant, the word `PROTOCOL.md` and `SCHEMA.md` use: `Namespace` is `Tenant`, `NamespaceRun` `TenantRun`, `namespace_base` `tenant_base`, and likewise every identifier, getter, view, builder, error text, file name, test name, feature file, scenario line, tool, Lean name and note, `openspec/specs/` included, so a baseline scenario and its feature file still read alike. The only bytes changed are the skolem base, `…/tenant/{n}/`, 3 shorter per base: the golden frames and seeds holding it are written again, with their pinned sizes and checksums. `buckets.json` and `unique_values.json` changed their field names and hold the same values, and six seeds took new file names with their bytes unchanged. `tenant` collided once, so two test parameters are `number` and a pattern binds `key_tenant`. Behaviour and test counts are unchanged.

A grep of the old word finds only these senses: the unit `SCHEMA.md` section 1 defines, inside a tenant, in the specifications, `design.md`, `proposal.md` and the deltas; Dgraph's, Badger's and the fork's, in citations (`ATTRIBUTION.md`, `tools/golden/hnsw.go`, `x.NamespaceOffset`, `isBanned`, `edgraph/`, `namespaced attribute`, `openspec/notes/security.md`, the Dual Query Engine Plan's requirements table); UGS's vocabulary declaration, now `prefix`, in the Schema and Query Language note, the Shard-per-Core note's IRI templates and the stage review's row; XSD's, Lean's keyword, Linux's user namespaces and the Go hex-dump input; the two baseline requirement headers the deltas' `FROM` lines must match until archive, and those lines; `SCHEMA.md` section 15's quotation of the old `SECURITY.md` sentence; this task's own text; and archived notes and changes. `ns` remains only as nanoseconds, the plural of `n`, the Galician and Portuguese word, Dgraph's key layout in hnsw's `translated`, and IRIs under `/ns/`.
