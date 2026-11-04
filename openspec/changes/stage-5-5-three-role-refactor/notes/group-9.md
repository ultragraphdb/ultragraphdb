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
