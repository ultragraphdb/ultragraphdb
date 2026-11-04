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
Bazel on a second run, tagged `exclusive` and `integration_test`, with no sandbox opt-out in the test action. A small `std::process`
helper replaces scrith's `service_utils`, whose `wait_utils` depends on `tokio` and `tonic`, and which cannot read an exit code, as the
`sysexits` scenarios must.

Fuzz target `health_request`: 376,125 executions in 30 s, coverage 172, no failure. Its 6 seeds are hand-written requests.

**Images.** Each binary's `BUILD.bazel` declares its `glibc_multiarch_img`, on the base the server ships on, its `sha256_tag`, and an
`image_push` to `index.docker.io`, `hansenmarvin/ultragraphdb_<role>`, tagged by the content tag. Built, never pushed.

**Pinning.** Only Linux can read a binding back. On macOS a role asks for nothing, since compio would set only a cache-locality hint, and
logs that it runs unpinned; that is the path verified here. The Linux path has run nowhere yet.

**Mutants.** `error_role`: 3 caught, 2 unviable. `common_service`: 53 caught, 9 unviable, 9 missed on the second pass, before the test that
kills `delete !` in `serve`. The remainder change nothing the crate's own suite can observe on macOS:

- `run_role.rs`, `stop_flag -> Ok(Arc::new(..))`: installs no handler. The process scenario "A signal stops a role cleanly" kills it, in
  `ultragraphdb_tests`; a unit test would have to signal its own test process.
- `thread_allowed` (four mutants) and `role_runtime`'s `== Some(true)`: reach `/proc`, which macOS lacks. To run again on Linux.
- `read_request`, `count > 0` to `true` and to `>=`: a peer that closed reads 0 bytes until the loop's bound, and gets the same 400. It
  only takes longer.
