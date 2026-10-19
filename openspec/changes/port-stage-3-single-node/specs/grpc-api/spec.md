## ADDED Requirements

### Requirement: The Dgraph service is served from api.proto unchanged
The Alpha SHALL serve `service Dgraph` from Dgraph's `api.proto` (dgo v250): `Login`, `Query`, `Alter`,
`CheckVersion`, `RunDQL`, `AllocateIDs`, `CreateNamespace`, `DropNamespace` and `ListNamespaces`, with the
messages and field numbers of that file.

#### Scenario: The Go client and the Rust client both connect
- **WHEN** dgo v250 and `ultragraph_client` each connect, check the version and run a query
- **THEN** both succeed against ultragraphdb as they do against Go Dgraph

#### Scenario: CheckVersion reports the server version
- **WHEN** a client calls `CheckVersion`
- **THEN** the response carries ultragraphdb's version tag

### Requirement: Calls that leave a transaction open are refused
The Alpha SHALL refuse with `FAILED_PRECONDITION`, naming the one-request rule, `CommitOrAbort`, a `Request` with
mutations and `commit_now` false, and a `Request` with mutations and a nonzero `start_ts`. It SHALL refuse
`UpdateExtSnapshotStreamingState` and `StreamExtSnapshot` with `UNIMPLEMENTED` until stage 5.

#### Scenario: A mutation without commit_now is refused
- **WHEN** a request carries a mutation and `commit_now` false
- **THEN** it is refused with `FAILED_PRECONDITION` and nothing is written

#### Scenario: CommitOrAbort is refused
- **WHEN** a client calls `CommitOrAbort`
- **THEN** it is refused with `FAILED_PRECONDITION`

#### Scenario: Mutations continuing an earlier transaction are refused
- **WHEN** a request carries mutations, `commit_now` true and a nonzero `start_ts`
- **THEN** it is refused with `FAILED_PRECONDITION`

### Requirement: Request fields for queries
A query request SHALL accept `read_only`, `best_effort` only together with `read_only`, a `start_ts` with its
`hash` for a read at an earlier timestamp, `vars`, and `resp_format` JSON or RDF; an empty request SHALL be refused.

#### Scenario: best_effort without read_only is refused
- **WHEN** a request sets `best_effort` and not `read_only`
- **THEN** it is refused with Dgraph's error that best-effort queries must be read-only

#### Scenario: An empty request is refused
- **WHEN** a request carries neither a query nor mutations
- **THEN** it is refused with `empty request`

### Requirement: RunDQL is a one-request call
`RunDQL` SHALL run its DQL text as one request, committing any mutations it carries.

#### Scenario: RunDQL commits an upsert
- **WHEN** `RunDQL` carries an upsert block
- **THEN** the mutations are committed and the response carries the commit timestamp
