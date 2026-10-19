## Context

Stage 3 puts stages 1 and 2 behind Dgraph's API: one Zero process and one Alpha process, one group, no
replication. The Go code is `worker/task.go` with `sort.go`, `tokens.go`, `trigram.go`, `stringfilter.go`,
`match.go`; `query/`; `edgraph/` without GraphQL; `acl/`; Zero's `assign.go`, `oracle.go` and the membership calls an
Alpha makes; the HTTP handlers of `dgraph/cmd/alpha` and `dgraph/cmd/zero`.

In Dgraph a mutation is proposed when sent, held as a pending transaction in the Alpha, decided by Zero's
oracle at commit (first committer wins on written keys, `oracle.go:72`), and applied when Zero's delta reaches the
group (`draft.go:966`). An upsert block's query runs at the start timestamp, outside the apply path, so its safety
rests on `@upsert` conflict keys. The port replaces both: a transaction is one request
(`dgraph-port-overview.md`, §Transactions).

The fork's embedded mode (`worker/embedded.go`, `hooks/config.go`) already calls Zero's functions in-process for
one group. Stage 3 follows the same shape over gRPC, because Zero stays a separate process.

## Goals / Non-Goals

**Goals:**

- Dgraph's query results, error texts and response encodings, checked against Go Dgraph on the same requests.
- One-request transactions in both forms, plus Dgraph's write-key check for requests carrying neither.
- Zero as its own process for timestamps, leases and membership.
- Access control and namespaces without the GraphQL admin API.

**Non-Goals:**

- Replication, Raft, snapshots: stage 4.
- Several groups, tablets, predicate moves, cross-group commits: stage 5.
- Export, backup, restore, the loaders: stage 5.
- GraphQL in any form.

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `query_task` | `ugdb_query/query_task` | `worker/task.go`, `sort.go`, `tokens.go`, `trigram.go`, `stringfilter.go`, `match.go`, `aggregator.go` |
| `query_engine` | `ugdb_query/query_engine` | `query/` without `outputnode_graphql.go`: subgraphs, functions, variables, aggregation, math, groupby, recurse, shortest, cascade, normalize, expand, mutations' uid assignment and edge expansion, JSON and RDF output |
| `server_zero` | `ugdb_server/server_zero` | Zero's `assign.go`, `oracle.go`, the `Zero` gRPC service subset an Alpha uses, its HTTP handlers; binary `ultragraph-zero` |
| `server_acl` | `ugdb_server/server_acl` | `acl/`, `edgraph/access*.go`, `predicate_acl.go`, JWTs |
| `server_alpha` | `ugdb_server/server_alpha` | `edgraph/` (server, namespaces, multi-tenancy), the request path, the apply thread, HTTP handlers; binary `ultragraph-alpha` |

### All writes go through the group's apply thread

Every request carrying mutations is queued to the group's apply thread. The thread takes what is waiting, as one
batch, and for each request in order:

1. takes its commit timestamp from a block leased from Zero for the batch;
2. checks it: an upsert block runs its query and conditions against the state so far, earlier requests of the
   batch included; a read-set request is checked against the group's record of the last commit per key and range;
   a request carrying neither is checked on its write keys against the same record, as Dgraph checks it;
3. builds its writes in a scratch layer stacked on the batch's.

The batch is written as one store batch (stage 2), the per-key commit record updated with it, and Zero told which
of the leased timestamps were applied and which abandoned. Zero advances its done watermark past them and streams
it to the Alpha, and reads at `t` wait for the watermark to reach `t`, as `WaitForTs` does.

The conflict check lives in the group because one group holds every key of its predicates and applies their
commits in order; the per-key record is derived from the applied log, so in stage 4 every replica computes the same
outcome. Zero keeps timestamps, leases and the done watermark.

Alternatives:
- Dgraph's path: propose, hold pending, let Zero decide, apply its delta. It keeps the uncommitted layer the port
  removes, and two round trips per transaction.
- Zero checks every commit, as Dgraph's oracle does. That serializes all groups' commits through Zero, the
  bottleneck the investigation measured. Stage 5 revisits Zero's role for transactions spanning groups.

### Upsert blocks run the query engine inside the apply thread

The apply thread calls `query_engine` synchronously on a read view of the store snapshot plus the batch's scratch
layers. Query execution therefore has a synchronous entry point over a read view; the async gRPC edge awaits the
apply thread's result.

### Conflict keys ported as Dgraph derives them

`GetConflictKey` (`posting/list.go:863`) and its farm fingerprints are ported unchanged for the write-key check,
which also serves as the baseline in the isolation benchmark.

### GraphQL cut and what replaces it

- `outputnode_graphql.go`, `ToJson`'s GraphQL field, `QueryGraphQL`, persisted queries, GraphQL schema updates in
  `Alter`: not ported.
- The `IsGraphql` context flag, which Dgraph also uses to let internal writers touch reserved predicates
  (`server.go:351`, `2361`), becomes an explicit internal-writer flag.
- User and group management, through GraphQL `/admin` in Dgraph, becomes DQL on the ACL predicates and the
  `ultragraph acl` command, both one-request.
- Namespace management in tests uses the gRPC namespace calls.

### Conformance against Go Dgraph

Dgraph's query tests are black-box tests against a cluster (`query/integration_test.go:19`). They are ported as BDD
features whose steps drive `ultragraph_client`, and the same features run against Go Dgraph v25.5.0 in a
container through `ultragraph_client`'s `DgraphInstance` fixture, with uids and latencies normalised. A scenario
passes when both servers agree and match the expectation; Dgraph's test data (`populateCluster`,
`common_test.go:355`) is loaded into both. Multi-request tests (`alpha/txn_test.go`) are rewritten as one-request
transactions and run against ultragraphdb alone.

### Isolation checked by Elle

A workload generator records histories of list-append and register transactions against ultragraphdb in both
transaction forms, and Elle (Jepsen's checker, run as `elle-cli`) checks them for serializability. The runs are
integration tests, tagged `integration_test`.

## Risks / Trade-offs

- [A long upsert block stalls the group's writes] → Measured as head-of-line blocking in the isolation benchmark; a
  per-block cost bound is a candidate if it bites.
- [The read-set carrier is undecided] → The read-set form cannot ship before it is decided; the task group
  implementing it starts with that decision.
- [Black-box suites need Go Dgraph in a container] → They are integration tests; their scenarios skip with a printed
  notice where no container runtime exists, as scrith's suites do.
- [Read-your-writes timing differs] → Dgraph answers a commit before the Alpha applies it and relies on the next
  read's wait. The port answers after the apply thread has written the batch, so a read at a later timestamp waits
  only for the watermark.

## Migration Plan

New crates and binaries. `ultragraph_client` changes its read-write transaction API to one request, which
scrith's manager adopts.

## Open Questions

- How does a read-set transaction carry its read set to the server: a field added to `Request`, a reserved key in
  `vars`, or a read token the Alpha keeps for a bounded time?
- Past what size does a read set coarsen to whole-predicate ranges?
- How many leased timestamps does an apply batch take from Zero at once, and how long does Zero hold an unreported
  lease before treating it as abandoned?
