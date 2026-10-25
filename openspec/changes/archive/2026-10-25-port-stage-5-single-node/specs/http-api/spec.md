## ADDED Requirements

### Requirement: The Alpha's HTTP endpoints
The Alpha SHALL serve `/login`, `/query` (with `debug`, `timeout`, `startTs`, `hash`, `be`, `ro`, `respFormat`),
`/mutate` (requiring `commitNow=true` and refusing `startTs` without a read set), `/alter`, `/health` (with `all`),
`/state`, and as direct handlers `/admin/shutdown`, `/admin/draining` and `/admin/config/cache_mb`, which Dgraph routes
through its GraphQL admin server, with Dgraph's request and response bodies; `/commit` SHALL be refused, and `/graphql`,
`/admin`, `/admin/schema` and `/probe/graphql` SHALL not be served.

#### Scenario: A query over HTTP returns Dgraph's envelope
- **WHEN** a DQL query is posted to `/query`
- **THEN** the body holds `data` and `extensions` with `txn`, `server_latency` and `metrics`, as Go Dgraph's does

#### Scenario: A mutation over HTTP without commitNow is refused
- **WHEN** a mutation is posted to `/mutate` without `commitNow=true`
- **THEN** it is refused with an error naming the one-request rule

#### Scenario: The GraphQL endpoint is absent
- **WHEN** a request is posted to `/graphql`
- **THEN** the response is 404

### Requirement: Zero's HTTP endpoints
Zero SHALL serve `/health` (plain `OK`, or JSON when asked with `Accept: application/json`), and, unless admin HTTP is
disabled, `/state` and `/assign?what=uids|timestamps|nsids&num=N` behind the admin guard.

#### Scenario: Zero assigns UIDs over HTTP
- **WHEN** `/assign?what=uids&num=100` is requested
- **THEN** the response holds a range of 100 UIDs no lease has issued before
