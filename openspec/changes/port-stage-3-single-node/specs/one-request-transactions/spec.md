## ADDED Requirements

### Requirement: A transaction is one request
A request carrying mutations SHALL be one transaction: it is decided and applied in full, or not at all, and its
response SHALL carry the start timestamp, the commit timestamp and the uids assigned to its blank nodes. A request
carrying neither a read set nor an upsert block SHALL take its start timestamp from Zero on arrival, before it is queued.

#### Scenario: A commit returns its timestamps and uids
- **WHEN** a request sets `_:a <name> "A" .` with `commit_now`
- **THEN** the response carries a commit timestamp greater than its start timestamp and a uid for `a`

#### Scenario: A later read sees the commit
- **WHEN** a commit returns commit timestamp `c` and a query follows with a fresh timestamp
- **THEN** the query sees the committed data

#### Scenario: A read at an earlier timestamp does not see the commit
- **WHEN** a query reads at a start timestamp below `c`
- **THEN** it does not see the committed data

### Requirement: Upsert blocks run serially in the apply thread
An upsert block SHALL run its query and its conditional mutations inside the group's apply thread, in the order the
thread takes requests, against every commit before it, so that its conditions never see stale data and it never aborts
for a conflict. Each mutation's `cond` (`@if(...)` or `@filter(...)`) SHALL be evaluated as a filter over `uid(0)` beside
the block's query, and the mutation SHALL apply only when the filter passes. An empty `uid(v)` SHALL create a new node
in a set and be skipped in a delete, and an N-Quad whose `val(a)` has no value for its subject SHALL be dropped. A
request setting an `@unique` predicate SHALL run as an upsert block, its uniqueness checked in the apply thread.

#### Scenario: An upsert creates a node only when none exists
- **WHEN** two upsert blocks run concurrently, each querying `email = "a@b"` into `v` and creating a node with that
  email `@if(eq(len(v), 0))`
- **THEN** exactly one node with that email exists afterwards, and neither request aborted

#### Scenario: A false condition skips its mutation
- **WHEN** an upsert block's mutation carries `@if(eq(len(v), 0))` and `v` holds one uid
- **THEN** that mutation is not applied and the others are

#### Scenario: Several mutations take their own conditions
- **WHEN** an upsert block carries two mutations with different `@if` conditions
- **THEN** each is applied only if its own condition holds

#### Scenario: uid and val variables are substituted
- **WHEN** an upsert block sets `uid(v) <age> val(a) .` from its query's variables
- **THEN** each node in `v` holding a value in `a` receives it, and nodes without one receive nothing

#### Scenario: An empty uid variable creates a node
- **WHEN** an upsert block sets `uid(v) <name> "N" .` and `v` is empty
- **THEN** one new node with `name` `N` is created

#### Scenario: An undefined variable in a mutation is refused
- **WHEN** a mutation uses `uid(w)` and the query defines no `w`
- **THEN** the request is refused with Dgraph's undefined-variable error naming `w`

#### Scenario: A condition injecting DQL is refused
- **WHEN** a `cond` field carries text that closes the condition and opens another block
- **THEN** the request is refused

#### Scenario: A unique value is enforced serially
- **WHEN** two concurrent requests set `email: string @index(exact) @unique .` to `a@b` on two different nodes
- **THEN** exactly one commits and the other is refused for the duplicate value

### Requirement: Read-set transactions are checked at commit
A request carrying a read set SHALL be committed only if no key or range in the read set was committed after the
request's read timestamp, `start_ts`; otherwise it SHALL be aborted with `ABORTED`, writing nothing.

#### Scenario: A read set overwritten since its read timestamp aborts
- **WHEN** a client reads a balance at timestamp 10, another commit changes it at 12, and the client submits a write
  with read timestamp 10 and that key in its read set
- **THEN** the write is aborted with `ABORTED`

#### Scenario: Write skew is prevented
- **WHEN** two requests each read keys `x` and `y` at the same timestamp, each writes one of them, and both carry both
  keys in their read sets
- **THEN** at most one commits

#### Scenario: An index range in the read set catches a phantom
- **WHEN** a request reads the index range `age >= 30` and another commit adds a node with age 35 before it commits
- **THEN** the request is aborted

### Requirement: Requests without a read set are checked on their writes
A request with mutations and neither a read set nor an upsert block SHALL be checked as Dgraph checks it: aborted if a
key it writes was committed after its start timestamp, with Dgraph's conflict keys (`@noconflict`: none; `@upsert`:
every key of the predicate, across uids; list data keys: per key and value; other data keys: per key; index and count
keys: per key and uid; reverse keys: none).

#### Scenario: Two writes to one scalar of one node conflict
- **WHEN** two such requests both set `name` of `0x1` and the second's start timestamp is below the first's commit
  timestamp
- **THEN** the first commits and the second is aborted with `ABORTED`

#### Scenario: Writes to a uid list do not conflict
- **WHEN** two such requests add different uids to `friend` of `0x1`
- **THEN** both commit

#### Scenario: A noconflict predicate never aborts
- **WHEN** two such requests set `name @noconflict` of `0x1`
- **THEN** both commit

### Requirement: Mutation limits
A request SHALL be refused when it carries more N-Quads than the configured limit, or a string value whose index token
exceeds 65,000 bytes less the predicate's length, with Dgraph's error texts.

#### Scenario: Too many N-Quads are refused
- **WHEN** a request carries one N-Quad more than the configured limit
- **THEN** it is refused and nothing is written

#### Scenario: A value too large for its index is refused
- **WHEN** an exact-indexed string of 70,000 bytes is set
- **THEN** the request is refused with `value in the mutation is too large for the index`
