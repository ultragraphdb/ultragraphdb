# Rebase on stage 5.8

Stage 5.8's change (`openspec/changes/archive/2026-11-17-stage-5-8-system-group`) synced its deltas into
`openspec/specs/` on 2026-11-18. Archiving copies a MODIFIED requirement's whole text from this change's delta, so each
requirement below, which this change modifies and stage 5.8 changed, is brought to the archived text first. Then only
this change's own edits differ from it (task 0.1). Every stage 5.8 scenario is kept unless this change
removes it on purpose, and each such removal is stated in the delta.

33 requirements are modified by both. Under each, the scenarios of stage 5.8's archived text that this
change's delta does not hold, compared on 2026-11-17:

- `client-endpoint`, "An endpoint core keeps a view of what the closes announce":
  - "A subscription opened again reads what its skipped closes announced"
- `client-endpoint`, "Connections authenticate against the metadata group's principals":
  - "The frontend never holds a stored key"
- `client-endpoint`, "The endpoint asks the system group":
  - "A restarted frontend serves the sessions it served before"
- `config-service`, "The config service validates every setting before it serves one":
  - "The frontend's old settings are refused by name"
- `kubernetes-deployment`, "A pod's data survives its restart":
  - "A deleted frontend pod loses nothing"
- `kubernetes-deployment`, "Role addresses are stable names": every scenario held; the requirement's text is compared
- `kubernetes-deployment`, "Settings, keys and secrets reach the roles as Kubernetes objects":
  - "The frontend's pod holds no bootstrap secret"
- `kubernetes-deployment`, "The roles deploy on Kubernetes from the repository's Helm chart": every scenario held; the requirement's text is compared
- `metadata-group`, "Catalog versions take effect at an epoch boundary": every scenario held; the requirement's text is compared
- `metadata-group`, "Principals live in the metadata group": every scenario held; the requirement's text is compared
- `metadata-group`, "The metadata group holds the tenants": every scenario held; the requirement's text is compared
- `quad-reads`, "A read's transaction names its level": every scenario held; the requirement's text is compared
- `role-binaries`, "A role takes a port a killed process leaves held":
  - "The frontend's /health answers once its listener is bound"
- `role-binaries`, "Configuration is explicit and validated at start": every scenario held; the requirement's text is compared
- `role-binaries`, "Each role is its own binary and image":
  - "The frontend binary carries no storage engine"
  - "The frontend binary carries no book"
- `role-binaries`, "Each role runs one runtime per core":
  - "The frontend serves clients on every core"
- `role-binaries`, "Readiness, health and shutdown":
  - "The frontend is ready only once the books answer"
  - "The frontend is unready while its system group is unreachable"
- `role-binaries`, "Roles authenticate each other with the cluster's role secret": every scenario held; the requirement's text is compared
- `role-binaries`, "Roles meet through the transport at their addresses": every scenario held; the requirement's text is compared
- `role-binaries`, "The supervisor starts, stops and reports the roles": every scenario held; the requirement's text is compared
- `schema-changes`, "A partition checks the catalog version a Drop names":
  - "The frontend refuses a Drop naming another version than the one in effect"
  - "A pending version leaves a Drop naming the version in effect to the partition"
  - "During a check a Drop names the version being checked"
- `schema-changes`, "A tightening is checked before it is acknowledged":
  - "A check outlives the frontend that asked for it"
- `schema-changes`, "Drops":
  - "A resent Drop builds the version a stopped frontend left"
- `sequencer`, "An early close is asked by CloseRequest from either role":
  - "The frontend's request closes the open epoch"
- `sequencer`, "Partitions learn each closed epoch and stamp the open one": every scenario held; the requirement's text is compared
- `system-group`, "A lost system group is waited for until the deadline":
  - "A frontend started before storage waits"
  - "A compute process joins a restarted system group at once"
  - "A connection error ending a subscription's connection is joined again"
  - "A frame on a stream storage has not opened closes the subscription's connection"
  - "A late frame among a catalog version's frames is dropped"
  - "A catalog version's frame on stream 0 closes the connection"
  - "A catalog version's frame on a stream storage has not opened closes the connection"
- `system-group`, "One storage instance hosts the system group":
  - "The frontend holds no book"
- `system-group`, "The hosting process starts the group first and stops it last":
  - "Storage is ready before any frontend exists"
- `system-group`, "The system group serves the books' role commands at its role address":
  - "A command another role requests is refused"
- `transactions`, "Commits answer a session token": every scenario held; the requirement's text is compared
- `transactions`, "Exactly-once commits through sessions":
  - "A retry checked in the quiet time stays undecided"
- `wire-protocol`, "Data requests, prepared statements and describe":
  - "Describe names the catalog version it is drawn from"
  - "A client's Describe naming a catalog version is refused"
- `wire-protocol`, "Roles exchange role commands":
  - "ComputeRead carries the client's limits"
