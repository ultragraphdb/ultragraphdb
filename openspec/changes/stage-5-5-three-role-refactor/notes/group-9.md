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

Registrations reach another process's home by Reference, on connections each core keeps per address. `PROTOCOL.md` §13.15 gained `namespace`
in the command header: a home resolves an IRI in its namespace's dictionary, and the body named none. A partition's mailbox hands what it
cannot deliver in its process to its shell as a `RemoteMessage`.

`RefillBlocks` hands out what `LeasedBlocks` holds and notes each type taken, and the core leases ahead. `LeasedBlocks` keeps its existing
`Mutex`, taken once per block on the one core holding the partition, so never contended. Every other crossing is a message.

Scenarios: `ultragraphdb_tests`, 17, adding two processes sharing the partitions, a reference crossing processes, a flush on shutdown and
recovery before `/ready`. `ultragraphdb_cluster_partition`: a vanished client's transaction and a closed connection's, on the simulated host.
A declaration in a Write is refused at the partition, so a vector index never reaches one; the frontend's own refusal naming the predicate
arrives with 9.5.5.

What later tasks replace or add: a Drop, a Recall, a Scan and a Closed are refused as unavailable; a close announcing a catalog version is
delivered without its version read, since nothing offers one until the frontend's endpoint; 9.5.5 relays a client's frames to Bind.

## 9.4b The config service

**9.4b.1 HTTP.** `ultragraphdb_transport_http_server` and `_client`, one request per connection, bounded in size and time, with no keep-alive and no chunked body, so there is no framing two parsers could read differently: a `Transfer-Encoding` or a second `Content-Length` is refused. A request owns the buffer it arrived in, so its body moves on with no copy. `/health` and `/ready` moved onto the server as `HealthService`, the old responder's files and its fuzz package moved by `git mv`. Each crate has a fuzz target, `http_request` and `http_reply`, with hand-written seeds.
