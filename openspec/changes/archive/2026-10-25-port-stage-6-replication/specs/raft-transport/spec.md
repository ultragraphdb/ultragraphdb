## ADDED Requirements

### Requirement: Messages travel in batches per peer
Raft messages to a peer SHALL travel on one stream per peer, batched up to 10 MB, each frame a little-endian u32
length and the message, each batch carrying the sender's Raft context; a full per-peer queue of 100 batches SHALL
drop the batch and leave recovery to Raft.

#### Scenario: Messages arrive in order
- **WHEN** a leader sends 10,000 messages to a follower
- **THEN** the follower steps them in the order sent

#### Scenario: An unreachable peer is reported
- **WHEN** a send to a peer fails, or its `IsPeer` probe (every 5 s, 3 s timeout) fails
- **THEN** the sender reports it unreachable to Raft, and the peer catches up after it returns

### Requirement: Connections are pooled and checked
Each address SHALL have one pooled connection with heartbeats every 500 ms; a connection without an echo for 2 s
SHALL be unhealthy, and a removed peer's connection SHALL be closed.

#### Scenario: A silent connection becomes unhealthy
- **WHEN** a peer's heartbeats stop for 2 s
- **THEN** its connection is reported unhealthy and requests are not routed over it

### Requirement: Peers are verified
A replica SHALL accept Raft messages only when the batch's Raft context names its own group, a check Dgraph's
`RaftMessage` does not make; `IsPeer` SHALL answer whether a Raft ID is a voter of the group.

#### Scenario: A stranger's messages are refused
- **WHEN** a node of another group sends Raft messages to a replica
- **THEN** the replica refuses them
