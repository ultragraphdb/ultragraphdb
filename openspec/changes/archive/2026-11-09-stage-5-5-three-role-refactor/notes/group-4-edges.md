# Group 4, agent 4: the edge crates

The crates outside agents 1–3's brought onto the new APIs with minimal type adaptations and no other behaviour change.

## Baseline

At the wave's start only `ultragraphdb_protocol_wire` fails to build (`EncodedTerm::uid` removed by group 3), and every
crate below it is blocked: the clients, the transports, `protocol_scram`, `cluster_sequencer` (through
`transactions_log`) and `tool_bench` (through `transactions_commit`). `common_telemetry` builds.

Test attributes per crate at `HEAD` (`#[test]` and its async forms, counted in the source):

| Crate | Tests at `HEAD` |
|---|---|
| `tool_bench` | 68 |
| `client_core` | 94 |
| `client_tokio` | 15 |
| `client_compio` | 12 |
| `protocol_transport` | 116 |
| `protocol_transport_compio` | 18 |
| `protocol_scram` | 33 |
| `common_telemetry` | 44 |
| `cluster_sequencer` | 95 |

## Fuzz packages

Built with `cargo +nightly fuzz build --fuzz-dir fuzz/<package>` before any agent's change landed, none depending on
`protocol_wire`: `cluster_placement`, `cluster_sequencer`, `common_telemetry`, `core_codec`, `core_ids`, `core_keys`,
`core_terms`, `store_hnsw`, `tool_toml`. After wire's surface and the adaptations below: `protocol_scram`,
`protocol_transport`, `client_core`, `cluster_sequencer`.

## `ultragraphdb_tool_bench`

- `rollup_run.rs`: `uid_of` → `id_of` over `QuadStore::id_of`, the predicate and objects as `SystemId`, the reverse key's
  object `EncodedTerm::id`. `DEFAULT_GRAPH` is the `Graph` `key_prefix` and `QuadKey` take, unchanged in the source.
- `setup.rs`: `BenchApplier` names the block source; `open_applier` leases from a `TestBlockSource` of 2^20-sequence
  blocks (no metadata group runs beside the bench) and draws names from `RandomSource::new(1)`. The user accepted
  the test-utils source here until a production block source exists (2026-11-03).
- `Cargo.toml`: `ultragraphdb_core_ids` (the `SystemId` the run names) and `ultragraphdb_traits_blocks` (the source).
- `apply_run.rs`'s `Change::DeleteStatement { .. }` arm needs no change.
- `pipeline_run.rs` compiles against the commit step's new API unchanged (`Pipeline::new`, `RoundBounds`,
  `Transaction::begin/commit`, `Wal::open`; the run calls no `append`, `checkpoint` or `trim_through` itself). Its store
  now opens with `Durability::Log`, the mode a partition beside the write-ahead log runs, for `Process`; since fjall's
  journal then stays in its buffer until asked, the store is synced before the journal's bytes are read at the run's
  start, as it already was at its end. The module's doc says so.

## `ultragraphdb_client_core`

- `src/utils_test/test_server/test_server_commit.rs`: the test server answers each commit-time operation with
  `OperationResult::for_node(i, true)`, the node by its index in the reply's term table, for `new(100 + i, true)`.
- `src/utils_test/test_server/test_server_requests.rs` and `tests/bdd.rs`: the repeated IRI is
  `ResultTerm::iri("http://ex.org/same")`, named by its text's hash, for an `EncodedTerm::uid(42)` with its text.
- `Cargo.toml`: `ultragraphdb_core_terms` dropped from `test-utils` and the dev-dependencies, which nothing names now.
- `tests/types/client_core/client_core_commit_tests.rs`: the second result names node 1 (`node() == Some(1)`) for
  UID 101. Rewritten, not dropped.

## `ultragraphdb_cluster_sequencer`

Not on the brief's list; it failed to build on `traits_log`'s surface. `src/types/sequencer/mod.rs`: its rounds name
derivation version 1 (`DERIVATION`; they hold no quads, and the round header refuses 0), and the appended round's
position is read off the `DurabilityTicket`. `tests/utils/harness.rs` and `tests/types/sequencer/sequencer_recover_tests.rs`:
`RoundRef::new` takes derivation 1. Agent 2 then moved the three call sites to `NonZeroU32` (`DERIVATION:
NonZeroU32 = NonZeroU32::MIN`, `NonZeroU32::MIN` in the two tests) with `traits_log`'s amended surface.

## The client conformance assertions

Agent 1's regenerated `commit.frame` carries a counter add at −5 and a compare-and-set, neither naming a node, so the
four assertions (`client_core_conformance_tests.rs`, `client_core_conformance_faults_tests.rs`, and the tokio and
compio shells' `client_conformance_tests.rs`) expect `[OperationResult::counter(-5), OperationResult::compare_and_set()]`,
their comments with them. `result.frame` grew from 600 to 648 bytes (IRIs and blank nodes by hash, their texts in the
string list), so the shells' damaged-golden test walks 6,640 cases for 6,544, its sum's comment with it.

## Result

| Crate | Tests at `HEAD` | After |
|---|---|---|
| `tool_bench` | 68 | 68 |
| `client_core` | 94 | 94 |
| `client_tokio` | 15 | 15 |
| `client_compio` | 12 | 12 |
| `protocol_transport` | 116 | 116 (unchanged source) |
| `protocol_transport_compio` | 18 | 18 (unchanged source) |
| `protocol_scram` | 33 | 33 (unchanged source) |
| `common_telemetry` | 44 | 44 (unchanged source) |
| `cluster_sequencer` | 95 | 95 |

No test dropped; one rewritten to the new API (`client_core_commit_tests.rs`, UID 101 → node 1), four assertions and
one count moved to agent 1's regenerated goldens.

`bazel build //... --keep_going` succeeds, and `bazel test //... --test_tag_filters=-integration_test` passes all 104
targets. `cargo fmt -p` reports nothing and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` warns nothing for
`tool_bench`, the three clients and `cluster_sequencer`. All 12 fuzz packages outside agents 1–3's build, rebuilt
after their DONE.


## Mutation testing

`scripts/mutants.sh ultragraphdb_tool_bench <file> --diff e3371c71`, one run each, over the two files whose logic
changed:

- `src/utils/pipeline_run.rs`: 2 mutants, 2 caught.
- `src/utils/setup.rs`: 4 mutants, 2 unviable, 2 missed, both on `BLOCK_LEN`'s `(1 << 20) - 1` (`-` → `+`, `-` → `/`).
  Any positive block length leaves every figure the bench reports unchanged, since it only sets how often the in-memory
  source leases, so they are equivalent. With the user's consent they are excluded in
  `.cargo/mutants.toml`, the comm check matching exactly these two of the crate's 536.

`rollup_run.rs`, the client tests and the sequencer's sites are type renames or test expectations; no run.
