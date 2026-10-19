## Why

With storage proven, the port can answer DQL. Stage 3 runs one Alpha and one Zero, as separate processes, over
one group and no replication, serving Dgraph's gRPC API. It is the first stage that Go Dgraph can be compared
with on the same requests, and the first that carries the port's own transaction model: a transaction is one
request, checked by its read set or run serially as an upsert block.

## What Changes

- Query execution ported from `worker/task.go`, its helpers and `query/`: every root function and filter, pagination,
  ordering, variables, aggregation, math, groupby, recurse, shortest paths, cascade, normalize, expand, facets,
  languages, counts, vector similarity, and schema queries; JSON and RDF output.
- The request path ported from `edgraph`: queries, mutations, upsert blocks, `Alter`, `CheckVersion`, `RunDQL`,
  `AllocateIDs`, login, namespaces.
- Zero's timestamp and ID leases and its oracle for one group, in its own process, reached over gRPC.
- **BREAKING (relative to Dgraph's API):** mutations without `commit_now`, requests with mutations and a
  `start_ts`, and `CommitOrAbort` are refused; HTTP `/commit` likewise.
- Two transaction forms: a read-set check, and upsert blocks run serially in the group's apply thread; a request
  carrying neither is checked on the keys it writes, as Dgraph checks it.
- Access control and namespaces: logins, JWTs, predicate permissions, guardians, the superadmin, namespace
  creation, deletion and isolation; user and group management through DQL instead of GraphQL.
- The Alpha's and Zero's HTTP endpoints, without GraphQL.
- **Behaviour change (Dgraph bug fixed):** `AllocateIDs` leases the kind of ID it is asked for; Dgraph leases
  UIDs for every lease type (`edgraph/zero.go`, `worker/mutation.go:608`).
- GraphQL removed from `query` and `edgraph`: `outputnode_graphql.go`, `QueryGraphQL`, persisted queries, the
  GraphQL schema updates in `Alter`, and the `IsGraphql` context flag, replaced by an explicit internal-writer flag.

## Capabilities

### New Capabilities

- `grpc-api`: the RPCs served, the request and response fields, and what is refused.
- `one-request-transactions`: the transaction forms, their checks, and what a commit guarantees.
- `timestamps-and-leases`: start and commit timestamps, read-only timestamps, UID, namespace and timestamp leases.
- `query-execution`: DQL evaluation over posting lists.
- `query-output`: the JSON and RDF encodings of results, metrics and latency.
- `alter-operations`: schema changes and drops through the API.
- `access-control`: logins, tokens, permissions, guardians and the ACL data.
- `namespaces`: creating, listing, dropping and isolating namespaces.
- `http-api`: the Alpha's and Zero's HTTP endpoints.
- `server-processes`: the Alpha and Zero binaries, their configuration, health and state.

### Modified Capabilities

None.

## Impact

- New crates under `ugdb_query/` and `ugdb_server/`, and the binaries `ultragraph-alpha` and `ultragraph-zero`.
- `ultragraph_client`: `Txn<ReadWrite>`'s `mutate` and `commit` become one-request transactions; its acceptance
  test runs against both servers.
- Dgraph's black-box query tests (`query/*_test.go`, `dgraph/cmd/alpha/*_test.go`, `acl/acl_test.go`) are ported as
  BDD suites run against ultragraphdb and against Go Dgraph v25.5.0.
- Multi-request tests (`alpha/txn_test.go`) are rewritten as one-request transactions.
