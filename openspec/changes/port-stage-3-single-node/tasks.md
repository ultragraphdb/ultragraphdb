Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. Scenarios of behaviour Go Dgraph shares are tagged `@differential` and
run against ultragraphdb and Go Dgraph v25.5.0, passing when both agree; scenarios of the port's own behaviour (the
one-request rule, the transaction forms, the fixed `AllocateIDs`, the admin replacements for GraphQL) run against
ultragraphdb alone.

## 1. Conformance harness

- [ ] 1.1 BDD step library driving `ultragraphdb_client` against a target server; normalisation of uids and latencies
- [ ] 1.2 Go Dgraph v25.5.0 as a second target through the `DgraphInstance` fixture, re-pinned from v25.4.0 to `dgraph/standalone:v25.5.0`, with the harness's address whitelisted; Dgraph's test data (`populateCluster`) loaded into both
- [ ] 1.3 The `@differential` tag: such a scenario passes only when both targets agree; untagged scenarios run against ultragraphdb alone
- [ ] 1.4 Add `ultragraphdb_query/*`, `ultragraphdb_server/*` and `ultragraphdb_bin` to the root `Cargo.toml`'s members

## 2. Zero: timestamps and leases → `ultragraphdb_server_zero`

- [ ] 2.1 Port `zero_test.go`'s lease cases; scenarios: timestamps-and-leases
- [ ] 2.2 Port `assign.go`: uid, namespace and timestamp leases, high-water marks persisted in Zero's store, bump leases, overflow refusal, per-namespace uid rate limits, read-only timestamp reuse
- [ ] 2.3 Port the done watermark and its stream to the Alpha; leased-timestamp reports (applied, abandoned)
- [ ] 2.4 The `Zero` gRPC subset an Alpha uses, the HTTP handlers, the `ultragraph zero` subcommand in `ultragraphdb_bin`

## 3. Task processing → `ultragraphdb_query_task`

- [ ] 3.1 Port `worker/sort_test.go`, `match_test.go`, `similar_to_options_test.go`, `precalculate_uids_test.go`; scenarios: root functions and filters
- [ ] 3.2 Port `processTask` and the function families: comparisons, terms and text, regexp, match, ngram, geo, has, uid_in, checkpwd, similar_to
- [ ] 3.3 Port `tokens.go`, `trigram.go`, `stringfilter.go`, `sort.go`, `aggregator.go`

## 4. Query engine → `ultragraphdb_query_engine`

- [ ] 4.1 Port the unit tests (`math_test.go`, `outputnode_test.go`, `fastjson_test.go`, `pagination_test.go`, `expand_edges_test.go`)
- [ ] 4.2 Features from `query0_test.go` to `query4_test.go`, `query_facets_test.go`, the vector and ngram suites; scenarios: query-execution
- [ ] 4.3 Port subgraph construction and processing, pagination, ordering, facets, languages, counts
- [ ] 4.4 Port variables, aggregation, math, groupby, recurse, shortest paths, cascade, normalize, expand
- [ ] 4.5 A synchronous entry point over a read view, for upsert blocks in the apply thread

## 5. Output → `ultragraphdb_query_engine`

- [ ] 5.1 Features from `rdf_result_test.go` and the JSON payloads of the query suites; scenarios: query-output
- [ ] 5.2 Port JSON output without the GraphQL field, RDF output, metrics, debug uids, limits

## 6. The request path → `ultragraphdb_server_alpha`

- [ ] 6.1 Scenarios: grpc-api; port the request checks and refusals (one-request rule, `best_effort`, empty request, limits)
- [ ] 6.2 Port `parseRequest`, `doQuery`, `processQuery`, `CheckVersion`, `RunDQL`, `AllocateIDs` (the named lease kind)
- [ ] 6.4 Ledger 9 and 3: `AllocateIDs` converts the lease type once at the API edge into `enum LeaseKind { Uid, Namespace, Timestamp }` and routes it by an exhaustive match, timestamps through Zero's `Timestamps`; a filtered read with `after:` past a list's last UID returns nothing and is excluded from the differential runs against Go Dgraph
- [ ] 6.3 Port mutation parsing, uid assignment, edge expansion (`S * *`), validation and the internal-writer flag

## 7. Transactions → `ultragraphdb_server_alpha`

- [ ] 7.1 Features from `alpha/upsert_test.go` (55) and `alpha/txn_test.go`, rewritten as one-request transactions; scenarios: one-request-transactions
- [ ] 7.2 The apply thread: batches, leased commit timestamps, stacked scratch layers, one store batch, reports to Zero
- [ ] 7.3 Upsert blocks in the apply thread: query, conditions as filters over `uid(0)`, `uid()` and `val()` substitution, condition validation, `@unique` requests as upsert blocks
- [ ] 7.4 Port `GetConflictKey` and the write-key check against the group's per-key commit record
- [ ] 7.5 Decide the read-set carrier (design, open question 1) and record it in the design and the overview
- [ ] 7.6 The read-set form: read tokens in query responses, ranges for index scans, the check at commit
- [ ] 7.7 Elle workload generator and `elle-cli` runs over both forms, tagged `integration_test`

## 8. Alter → `ultragraphdb_server_alpha`

- [ ] 8.1 Features from `edgraph/alter_test.go` and the schema cases of `alpha/run_test.go`; scenarios: alter-operations
- [ ] 8.2 Port `Alter`: schema changes, background runs, drops, guardian checks, groot recreation

## 9. Access control → `ultragraphdb_server_acl`

- [ ] 9.1 Port `access_test.go`, the FORK capability and anonymous tests; features from `acl/acl_test.go` without GraphQL; scenarios: access-control
- [ ] 9.2 Port the admin-source check, login, JWTs, refresh, the ACL cache and its refresh, query, mutation and alter authorization, guardians, the groot guard, the superadmin, the capabilities and anonymous postures, ACL-off behaviour
- [ ] 9.3 Port the `ultragraph acl` subcommand over one-request DQL

## 10. Namespaces → `ultragraphdb_server_alpha`

- [ ] 10.1 Port `namespace_test.go`; features from `systest/multi-tenancy`; scenarios: namespaces
- [ ] 10.2 Port namespace creation, listing, dropping, isolation, and the namespace taken from the token

## 11. HTTP and processes → `ultragraphdb_server_alpha`, `ultragraphdb_server_zero`

- [ ] 11.1 Features from `alpha/http_test.go`; scenarios: http-api, server-processes
- [ ] 11.2 Port the HTTP handlers without GraphQL; health, state, draining, shutdown; the `ultragraph alpha` subcommand
- [ ] 11.3 Both processes install telemetry through `ultragraphdb_common_telemetry` at start: `--log-level`, `--log-format plain|json`, `--otlp-endpoint`; spans for each request, query and commit; Prometheus metrics on the HTTP port with Dgraph's metric names where Dgraph exports them

## 12. The client → `ultragraphdb_client`

- [ ] 12.1 Replace `Txn<ReadWrite>`'s `mutate` and `commit` with a one-request transaction builder; keep the acceptance test running against both servers

## 13. Close the stage

- [ ] 13.1 Every scenario of this stage has its feature; `scripts/bdd_counts.sh` reports equal counts; `make test` green; the conformance features agree on both targets
- [ ] 13.2 The isolation benchmark: Dgraph's write-key check, the read-set check and serial upsert blocks on scrith's workload, uniform to Zipfian skew; throughput, p99, abort rate, head-of-line blocking; results in the overview
- [ ] 13.3 Attribution headers; `AGENTS.md` crate table updated
