## ADDED Requirements

### Requirement: A compute node joins with what clients need of it
A compute node SHALL send the system group, in this order, a Join naming its kind, its state, the address it
advertises to clients and its tags, a Subscribe and a Roster (`PROTOCOL.md` sections 11.14, 13.1 and 13.26). The
advertised address SHALL be `[compute] advertised_address`, of the form `tcp://host:port` with a host of at most 253
bytes, and its tags `[compute] tags`, one to eight, each 1 to 32 bytes of `a` to `z`, `0` to `9` and `_`; a node SHALL
carry exactly the tags its settings name, and `query` alone when they name none, so a node tagged `gpu` or
`high_memory` serves `query` clients only when its settings say so. A node SHALL join on one of its connections to the
system group, and when that connection closes while another of its connections is open it SHALL join again at once on
an open one, which moves its entry there. A Join SHALL be answered once it is durable. The system group SHALL refuse,
with code 15 and `detail` the bound, a Join that would take the list past 1,024 nodes or the cluster past 256 distinct
tags.

#### Scenario: A node's Join names its address and tags
- **WHEN** a compute node whose settings advertise `tcp://c1.db.example:7080` with tags `query` and `high_memory`
  starts
- **THEN** its Join names kind 2, state 1, that address and both tags in the order of their bytes, and its Subscribe
  and its Roster follow on the same connection

#### Scenario: A node without tags carries query
- **WHEN** a compute node's settings name no `compute.tags`
- **THEN** its Join names the one tag `query`

#### Scenario: A node tagged for GPUs alone serves no query client
- **WHEN** a node's settings name the one tag `gpu`, and a client of tag `query` holds a list with that node and one
  tagged `query`
- **THEN** the client sends that node nothing

#### Scenario: A node whose joining connection closes stays listed
- **WHEN** the connection a compute node joined on closes while its other cores' connections to the system group stay
  open
- **THEN** the node joins again on an open connection at once, and no close names a change of the list

#### Scenario: A tag past the cluster's bound is refused
- **WHEN** the list's nodes carry 256 distinct tags and a Join names a 257th
- **THEN** the Join is refused with code 15, `detail` 256, and the list is unchanged

### Requirement: The node list is a function of the log
The list of compute nodes SHALL be state of the metadata state machine (metadata-group, "The metadata group holds the
node list"): a Join SHALL add its entry or change it, and an entry SHALL belong to the connection of the latest Join
for its address, so that a Join for an address another connection holds replaces the entry and the other connection's
end then changes nothing. An entry SHALL leave by a logged removal, which the system group proposes once the entry's
connection has been closed for twice `epoch_ms_max` by its own clock, counted from the later of the connection's close
and the system group's start to listen: a time and not a count of closes, since the shortest epoch is one millisecond
and a count would drop a live node at every reconnect. The list's version SHALL be the epoch of the close that announced its latest change, so one version
names one list on every replica and across a restart, and two changes within one epoch share a version.

#### Scenario: A restart of the system group keeps the list
- **WHEN** two compute nodes are listed at version 12, the system group's host is killed and restarted, and both nodes
  dial again within twice `epoch_ms_max`
- **THEN** the list still holds both nodes, and no close names a change for them

#### Scenario: A node that is gone leaves after the grace
- **WHEN** a listed compute node is killed, and twice `epoch_ms_max` passes with its connection closed
- **THEN** a removal of its entry is logged, the next close names a change, and the list of that close's version does
  not hold it

#### Scenario: A reconnect within the grace removes nothing
- **WHEN** `epoch_ms_min` is 1 ms, a listed node's connection closes, twenty epochs close, and the node dials again and
  joins within twice `epoch_ms_max`
- **THEN** no removal is logged, and no close names a change of the list

#### Scenario: A second process at one address replaces the first
- **WHEN** a compute process joins with an address a running process already holds, and the first process's connection
  then closes
- **THEN** the list holds one entry for the address, the second process's, and the first's close removes nothing

#### Scenario: One version names one list
- **WHEN** a Roster is read at version 20 before and after a restart of the system group's host
- **THEN** both replies hold the same nodes, tags and states

### Requirement: A close tells compute that the list changed
The sequencer's close SHALL name a change of the node list since the previous close by `nodes` 1 (sequencer, "A close
names a change of the node list"). A compute node SHALL read the list with Roster at its start and for a close whose
epoch is above the version it holds, and for no other, and SHALL hold the list of the highest version it has read. A
Roster reply before any close named a change SHALL read as no list yet, and a reply MAY hold no node.

#### Scenario: A change reaches every compute node with a close
- **WHEN** a third compute node joins a cluster of two
- **THEN** the next close names a change, each node reads Roster once, and each holds three nodes at that close's
  epoch as version

#### Scenario: A replayed close reads nothing
- **WHEN** a compute node holding version 20 receives a close of epoch 18 flagged `nodes`
- **THEN** it sends no Roster

### Requirement: A client receives the list once it has authenticated
A compute node SHALL send a client connection the node list as a `Nodes` frame on stream 0, unasked: once it holds a
list with at least one node and no earlier than the connection's first final Authenticate, or the Welcome on a
connection of `auth` 0, and again whenever it holds a list of a higher version than the one it last sent that
connection. `self` SHALL name the sender's own entry, or `ABSENT` when the list does not hold it yet. No connection
SHALL receive a list before it has authenticated.

#### Scenario: An authenticated client is sent the list
- **WHEN** a client authenticates on a node that holds a list of three nodes at version 20
- **THEN** it receives one Nodes frame of version 20 naming three nodes, `self` the index of the node it is connected
  to

#### Scenario: An unauthenticated peer learns nothing of the cluster
- **WHEN** a peer completes Hello and Welcome on an endpoint of `auth` 1 and does not authenticate
- **THEN** it receives no Nodes frame

#### Scenario: A higher version follows on an open connection
- **WHEN** a node holding version 20 reads a list of version 24 while a client's connection is open
- **THEN** the client receives a Nodes frame of version 24, and none again until a higher version exists

### Requirement: A client holds the list of the highest version
The Rust clients SHALL hold the list of the highest version received on any connection to the cluster, replace it
whole by one of a higher version, and drop one at or below the version held, which is no error. A client SHALL never
wait for a list: until one arrives it SHALL use the connection it has, and it SHALL check tags only once it holds a
list. It SHALL never edit its list itself.

#### Scenario: An older list is dropped without error
- **WHEN** a client holding version 24 receives a Nodes frame of version 20 from a node that lags
- **THEN** it keeps the list of version 24, and the connection stays open

#### Scenario: A client works before any list arrives
- **WHEN** a client connects to a seed whose node holds no list yet, and commits
- **THEN** the commit is answered on that connection

### Requirement: A client is started with seeds and picks nodes by tag
A client SHALL be configured with one or more seed addresses and SHALL try them in the configured order, with
backoff; a refused authentication SHALL end the attempt for all of them. Seeds serve the first contact, and a client
that can reach no node of its list SHALL return to them, keeping the version it holds. A client SHALL be given a tag,
`query` when it names none, and SHALL send its requests to serving nodes that carry it; no request SHALL name a tag. A
client asked for a tag no serving node of its list carries SHALL refuse the request itself, naming the tags the list
holds. It SHALL keep the connections a new list still names, dial a newly listed node after a random delay, and skip
a listed node it cannot reach, trying it again with exponential backoff and jitter. A client SHALL verify a node's
certificate against the host of the address it dialled, a seed's or an advertised one. A session SHALL serve through
any node; a transaction SHALL stay on the connection it began on; and a prepared statement SHALL belong to its
connection, so a client prepares again on another node.

#### Scenario: A client spreads over the nodes of its tag
- **WHEN** a list holds two nodes tagged `query` and one tagged `gpu`, and a client of tag `query` begins 100
  transactions
- **THEN** both `query` nodes serve some of them, and the `gpu` node serves none

#### Scenario: A tag nobody carries is refused by the client
- **WHEN** a client of tag `high_memory` holds a list in which no serving node carries it
- **THEN** the request is refused by the client, the error naming `high_memory` and the tags the list holds, and no
  frame is sent

#### Scenario: A client with no reachable node returns to its seeds
- **WHEN** every node of a client's list refuses its connection, and a seed answers
- **THEN** the client connects to the seed, keeps the version it held, and takes a list of a higher version when the
  seed's node sends one

#### Scenario: A session serves through a second node
- **WHEN** a client registers a session on one node, commits request 1 there, and commits request 2 through the same
  session on a connection to another node
- **THEN** request 2 commits

#### Scenario: A statement is prepared again on another node
- **WHEN** a client executes on one node's connection a statement it prepared on another's
- **THEN** the Execute is refused with code 10, and succeeds once the client has prepared the statement on that
  connection

#### Scenario: A wrong password ends the attempt for every seed
- **WHEN** a client's first seed refuses its authentication
- **THEN** the client reports the refusal and dials no further seed

### Requirement: A node's certificate names the address it advertises
A compute node whose client endpoint speaks TLS SHALL check at start that its certificate names the host of its
`compute.advertised_address`, and SHALL exit with code 78 otherwise, naming the host and the certificate's names, so an
entry no client could verify never enters the list.

#### Scenario: A certificate for another host stops the start
- **WHEN** a compute node advertises `tcp://c1.db.example:7080` with a certificate naming `c2.db.example` alone
- **THEN** it exits with code 78 naming both, and sends no Join

#### Scenario: A wildcard certificate serves every node of a Service
- **WHEN** two compute nodes advertise `c1.db.example` and `c2.db.example` with one certificate naming `*.db.example`
- **THEN** both start, and a client verifies each against the host it dialled

### Requirement: A draining node is announced before it stops
A compute node about to stop SHALL join again with `state` 2 and SHALL stop no earlier than after it has sent each of
its client connections a Nodes frame naming it draining. It SHALL then wait for its read-write transactions to end,
at most `transaction_ms`, which bounds them already, and stop; a commit the stop cuts is in doubt and SHALL be resolved
by its resend on another node. A client SHALL begin no transaction and send no one-frame
request on a node its list names draining or names no longer; a transaction open there SHALL run to its end, its reads
included, and a read-only transaction, which has no limit on its life, ends when the node stops.

#### Scenario: A drained node takes no new transaction
- **WHEN** one of two compute nodes receives SIGTERM while a client holds connections to both
- **THEN** the client receives a Nodes frame naming that node draining before the node closes its connection, begins
  its next transaction on the other node, and a transaction open on the draining node commits there

#### Scenario: A draining node stops within the transaction limit
- **WHEN** a node receives SIGTERM while a client holds a read-write transaction open on it and sends nothing more
- **THEN** the node stops no later than `transaction_ms` after the transaction began, and a read-only transaction
  open on it ends with its connection

#### Scenario: Two compute nodes serve one cluster
- **WHEN** two compute processes and one storage process run on one host, and two clients each commit and read through
  a different node
- **THEN** each client reads the other's commit once it carries the commit's token
