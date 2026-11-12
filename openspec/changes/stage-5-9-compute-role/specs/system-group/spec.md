## ADDED Requirements

### Requirement: The system group serves the node list
The system group SHALL take a compute node's Join, durable before the answer, propose the removal of an entry whose
connection has been closed for twice `epoch_ms_max`, and serve Roster (node-discovery). It SHALL end a subscription whose frames
have gone unacknowledged by the transport for twice `epoch_ms_max`, closing its connection, so a node that fell silent
does not count as connected.

#### Scenario: A silent node's connection is closed
- **WHEN** a compute node's machine stops answering without closing its connection, and twice `epoch_ms_max` passes
- **THEN** the system group closes the connection, and the node's entry leaves the list once twice `epoch_ms_max` more
  has passed with no Join for it

## MODIFIED Requirements

### Requirement: One storage instance hosts the system group
The system group SHALL be the replica group holding the cluster's books: the metadata state machine (metadata-group) and
the sequencer (sequencer). A storage instance whose settings hold a `[system]` table SHALL host it, beside its
partitions, and no other process SHALL hold either state machine or their logs. The table SHALL name
`system.role_address`, where the group listens for role connections, and `system.data_dir`, where its logs lie; the
hosting instance SHALL also take the `[metadata]` and `[sequencer]` tables. The group SHALL have one replica until stage
8, so a target SHALL name exactly one hosting instance (config-service, "The config service validates every setting
before it serves one"), and its interface SHALL be the one stage 8 replicates. A storage instance without the table
SHALL host partitions alone and SHALL listen at no system group address.

#### Scenario: The hosting instance serves the books
- **WHEN** `storage_1` holds a `[system]` table and `storage_2` none, and both start
- **THEN** `storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens
  at its partitions' addresses alone

#### Scenario: A system table without its directory stops the start
- **WHEN** a storage instance's `[system]` table names `role_address` and no `data_dir`
- **THEN** the process exits with code 78 and an error naming `system.data_dir`

#### Scenario: Compute holds no book
- **WHEN** compute binary's dependency graph is listed
- **THEN** it names neither `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor
  `ultragraphdb_transactions_log`

### Requirement: The system group serves the books' role commands at its role address
The system group SHALL accept role connections at `system.role_address`, authenticated with the cluster's role secret as
every role connection is (role-binaries, "Roles authenticate each other with the cluster's role secret"), and SHALL
serve there, besides Hello, Authenticate, Credit and Cancel, the commands `PROTOCOL.md` section 11.12a lists from stage
5.9:

| Command | Requested by |
|---|---|
| Join, Members, Placement, Catalog, Subscribe, CloseRequest | compute and storage |
| Lease, DefaultEntry, Report | storage |
| Challenge, Verify, Session, Check, Ended, Roster, Declare, Defaults | compute |

It SHALL refuse every other command as `CommandNotServed`, naming the command. Every process SHALL reach the group at
`cluster.system_address`, the partitions of the hosting process included: they dial the address as the partitions of
any other process do, so one path carries a partition's asks wherever it runs.

#### Scenario: A partition of the hosting process dials the group
- **WHEN** the hosting storage process starts with partition 1
- **THEN** partition 1 opens a role connection to `cluster.system_address`, joins, subscribes and receives the closes
  on it

#### Scenario: A partition of another process is served alike
- **WHEN** `storage_2`, which hosts no group, starts with partitions 3 and 4
- **THEN** both join at `cluster.system_address`, Members names their addresses, and each receives every close

#### Scenario: A command the endpoint does not serve is refused
- **WHEN** a role connection to the system group sends a Bind
- **THEN** it is refused as `CommandNotServed` naming Bind

### Requirement: The hosting process starts the group first and stops it last
The hosting process SHALL start the system group, wait until it has recovered and listens, and then start its
partitions, since a partition's recovery reads the catalog versions its log names from the group. Its `/ready` SHALL
answer once the group listens and its partitions have recovered and joined, with no compute node running. On SIGTERM it
SHALL apply and sync what its partitions' logs hold, then stop the group once the group's logs are synced. The hosting
process SHALL exit for no lost peer; a failed log of the group SHALL stop it with code 74.

#### Scenario: Storage is ready before any compute node exists
- **WHEN** the config service and the hosting storage process start, and no other role does
- **THEN** the storage process's `/ready` answers

#### Scenario: A partition recovers from the group of its own process
- **WHEN** the hosting process restarts with a partition whose log names catalog version 3
- **THEN** the group recovers first, the partition reads version 3 with Catalog and then replays

#### Scenario: A failed log of the group stops the process
- **WHEN** a write to the metadata log fails
- **THEN** the hosting process shuts down through its ordinary path and exits with code 74

### Requirement: A lost system group is waited for until the deadline
A process that dials the system group, a storage process not hosting it or a compute node, SHALL dial again when its
connection closes, and SHALL exit with code 69 once the group has been gone for `[cluster] join_deadline_ms`, naming the
address. Until then a partition of a storage process that does not host the group SHALL go on committing in its open
epoch, SHALL refuse with code 5 once that epoch is full and its close request has gone unanswered for
`close_request_ms`, and SHALL let its snapshot reads wait within their deadlines. Until then compute SHALL refuse what
needs the group with code 17 (Unavailable): a request through a session, cancelling the Bind stream of a transaction
whose Check went unanswered, a Register, a declaring Commit and a Describe; and SHALL answer an authentication it cannot
relay with Error 17 on stream 0, closing the connection and counting no failure, so an outage bans no source. A Begin, a
read and a commit through no session SHALL go on from its view.

#### Scenario: A compute node started before storage waits
- **WHEN** compute starts while no process listens at `cluster.system_address`, and the hosting storage process
  starts within the deadline
- **THEN** compute dials until it is answered, and its `/ready` answers after storage's

#### Scenario: A group gone past the deadline ends its dependants
- **WHEN** the hosting storage process is killed and does not return within `join_deadline_ms`
- **THEN** every compute node shuts down through its ordinary path and exits with code 69 naming the
  system group's address

#### Scenario: A commit during the outage is refused, not lost
- **WHEN** compute's connection to the group is closed and a client commits through its session
- **THEN** the Commit is answered code 17 and reaches no partition, and the same Commit succeeds once the group answers
  again

#### Scenario: A partition without closes commits until its epoch is full
- **WHEN** the hosting storage process is killed, and a client commits through a session-less write to a partition of
  another storage process until that partition's open epoch is full
- **THEN** each commit before that is acknowledged, and the first past it is refused with code 5 once
  `close_request_ms` has passed with no close

#### Scenario: An outage bans no source
- **WHEN** one address authenticates 80 times while the system group cannot be reached, and once after it answers again
- **THEN** each of the 80 is answered Error 17 and counted as no failure, and the last authenticates
