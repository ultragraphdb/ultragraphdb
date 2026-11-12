## ADDED Requirements

### Requirement: One storage instance hosts the system group
The system group SHALL be the replica group holding the cluster's books: the metadata state machine (metadata-group)
and the sequencer (sequencer). A storage instance whose settings hold a `[system]` table SHALL host it, beside its
partitions, and no other process SHALL hold either state machine or their logs. The table SHALL name
`system.role_address`, where the group listens for role connections, and `system.data_dir`, where its logs lie; the
hosting instance SHALL also take the `[metadata]` and `[sequencer]` tables. The group SHALL have one replica until
stage 8, so a target SHALL name exactly one hosting instance (config-service, "The config service validates every
setting before it serves one"), and its interface SHALL be the one stage 8 replicates. A storage instance without the
table SHALL host partitions alone and SHALL listen at no system group address.

#### Scenario: The hosting instance serves the books
- **WHEN** `storage_1` holds a `[system]` table and `storage_2` none, and both start
- **THEN** `storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens
  at its partitions' addresses alone

#### Scenario: A system table without its directory stops the start
- **WHEN** a storage instance's `[system]` table names `role_address` and no `data_dir`
- **THEN** the process exits with code 78 and an error naming `system.data_dir`

#### Scenario: The frontend holds no book
- **WHEN** the frontend binary's dependency graph is listed
- **THEN** it names neither `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor
  `ultragraphdb_transactions_log`

### Requirement: The system group runs on a core of its own over logs of its own
The hosting process SHALL run the system group on one runtime of its own (role-binaries, "Each role runs one runtime
per core"), sharing no mutable state with a partition's. The metadata state machine's log and the sequencer's log
SHALL lie under `system.data_dir`, in `meta/` and `seq/`, each with its snapshots, apart from every partition's log,
and each SHALL sync by itself: no partition's commit SHALL wait on a sync of the group's, and no close on a
partition's. The group SHALL recover both logs before it listens, and SHALL form the placement when it holds none, an
even map over `[cluster] partition_count`.

#### Scenario: The books recover in the storage process
- **WHEN** the group holds a catalog version, two block leases, a principal and twelve closes, and its host is killed
  and restarted
- **THEN** the group's state after recovery equals its state before the kill, read at the same log position, and the
  sequencer's last closed epoch is 12

#### Scenario: The group's logs lie beside the partitions'
- **WHEN** the development container has started with `system.data_dir = "/data/system"`
- **THEN** `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`

#### Scenario: A partition's sync holds up no close
- **WHEN** a partition's log sync is delayed past `epoch_ms_max` while the group's logs sync at once
- **THEN** the sequencer closes its epochs on time, and the delayed partition receives them once it reads again

### Requirement: The system group serves the books' role commands at its role address
The system group SHALL accept role connections at `system.role_address`, authenticated with the cluster's role secret
as every role connection is (role-binaries, "Roles authenticate each other with the cluster's role secret"), and SHALL
serve there, besides Hello, Authenticate, Credit and Cancel, the commands `PROTOCOL.md` section 11.12a lists for stage
5.8:

| Command | Requested by |
|---|---|
| Join, Members, Placement, Catalog | the frontend, compute and storage |
| Subscribe, CloseRequest | the frontend and storage |
| Lease, DefaultEntry, Report | storage |
| Challenge, Verify, Session, Check, Ended, Declare, Defaults | the frontend |

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

### Requirement: The registry is asked by Session, Check and Ended
The system group SHALL register a session when a Session command names it with its principal, durable before the answer; a session the registry already holds, for any principal, SHALL be refused with Error 1, naming the session, a principal the metadata state machine does not hold with code 7, and a full registry holding no session it may evict SHALL refuse with code 14, registering nothing. It SHALL decide the checks of a Check frame in
their order and answer the frame whole, each check with its outcome, decided in this order: 12 when the registry does not hold the session, 21 when the session belongs to another principal, 14 when the session has a request in flight, whatever the check's kind, and 1 otherwise. A check SHALL read the registry as every registration decided before it left it, durable or not. A check of kind 2 that passes SHALL mark the session in
flight; a check of kind 1 SHALL mark nothing. An Ended SHALL take away the mark of each session it names and SHALL be
answered by nothing; a session it names without a mark SHALL be passed over. The registry SHALL evict only a session
with no mark (metadata-group, "The metadata group registers sessions").

#### Scenario: A held identity is refused whoever holds it
- **WHEN** a Session registers a session for `alice`, and the same Session is sent again
- **THEN** the second is refused with Error 1 naming the session, and the registry holds the session once

#### Scenario: A session of another principal is refused
- **WHEN** the registry holds a session for `alice` and a Session names it for `bob`
- **THEN** it is refused with Error 1 naming the session, and the session stays `alice`'s

#### Scenario: An unknown principal registers nothing
- **WHEN** a Session names a principal the metadata state machine does not hold
- **THEN** it is refused with code 7 naming the principal

#### Scenario: A frame of checks is answered whole and in order
- **WHEN** one Check frame holds a kind 2 check of `alice`'s session, a kind 2 check of a session never registered, and
  a kind 1 check of `bob`'s session naming `alice`
- **THEN** one reply holds the outcomes 1, 12 and 21 in that order, and `alice`'s session alone is marked

#### Scenario: A second commit of a marked session is refused
- **WHEN** a session is marked by a passed check of kind 2, and another check of kind 2 names it before any Ended
- **THEN** the second check's outcome is 14

#### Scenario: A resend's check sees the original in flight
- **WHEN** a session is marked by a passed check of kind 2 on one connection, and a check of kind 1 names it on another
- **THEN** the kind 1 check's outcome is 14, and it marks nothing

#### Scenario: A marked session is not evicted
- **WHEN** the registry holds `sessions_max` sessions, the least recently registered is marked, and one more is
  registered
- **THEN** the least recently registered without a mark is evicted, and the marked one stays

#### Scenario: An Ended sent again changes nothing
- **WHEN** an Ended names a marked session twice in two frames
- **THEN** the first takes the mark away, the second is passed over, and neither is answered

### Requirement: A mark outlives its connection by the transaction limit
The system group SHALL hold each mark against the role connection whose Check began it. An Ended SHALL end a mark
begun on its own connection, or one whose connection has closed, and SHALL pass over any other. When a connection
closes, the group SHALL keep its marks for `transaction_ms`, the longest a request no endpoint speaks for can still
reach a commit step, and end them then, so a session whose request is still on its way is neither evicted nor told
"not committed". For `transaction_ms` after the group begins to listen, when it holds no mark of the requests then on
their way, it SHALL evict no session, refusing a Session that would need an eviction with code 14, and SHALL answer
every check of kind 1 with 14.

#### Scenario: A closed connection's marks hold for the limit
- **WHEN** a connection marks a session and closes, and a check of kind 1 names the session on another connection
  before `transaction_ms` has passed, and again after it
- **THEN** the first check's outcome is 14 and the second passes

#### Scenario: An orphaned mark is ended from another connection
- **WHEN** a connection marks a session and closes, and another connection sends an Ended naming the session
- **THEN** the mark ends, and a check of kind 2 naming the session passes

#### Scenario: Another open connection's Ended leaves a mark
- **WHEN** one connection marks a session and another sends an Ended naming it while the first is open
- **THEN** the session stays marked

#### Scenario: A restarted group answers no resend at once
- **WHEN** the system group's host restarts, and a check of kind 1 arrives before `transaction_ms` has passed since it
  began to listen
- **THEN** the check's outcome is 14, a check of kind 2 naming an unmarked session passes, and a Session that would
  evict is refused with code 14

### Requirement: The system group goes on answering checks while a log syncs
While the system group waits on a sync of the metadata state machine's log or of the sequencer's, it SHALL go on
answering Check from what is durable: the registry with every eviction appended. It SHALL answer every other command
once the sync has ended, in the order the commands came, so a close's sync delays no commit's check.

#### Scenario: A check is answered while a close syncs
- **WHEN** the sequencer's sync of a close is in flight, and a Check and a Session arrive
- **THEN** the Check is answered before the sync ends, and the Session only after it

#### Scenario: An eviction appended is seen by the next check
- **WHEN** an eviction is appended to the metadata log and its sync is in flight, and a Check names the evicted session
- **THEN** the check's outcome is 12

### Requirement: A client's proof is checked in the system group
The system group SHALL serve Challenge and Verify (`PROTOCOL.md` sections 13.8 and 13.9): to a Challenge it SHALL
answer the named principal's salt and iteration count, or for a name no principal holds a salt and a count derived
from the secret the metadata state machine holds (metadata-group, "Principals live in the metadata group"); to a
Verify it SHALL check the client's proof against the principal's stored key and answer whether it holds, the
principal, and, when it holds, the ServerSignature. An unknown name and a wrong proof SHALL be answered alike. A
principal's stored key and server key SHALL leave the group in no frame.

#### Scenario: A proof that holds is answered with its signature
- **WHEN** a Verify carries `ann`'s name, the exchange's AuthMessage and the proof `ann`'s password gives
- **THEN** the reply holds `verified` 1, `ann`'s principal and a ServerSignature of 32 bytes

#### Scenario: An unknown name and a wrong proof answer alike
- **WHEN** one Verify carries a wrong proof for `ann` and another any proof for the unknown name `nobody`
- **THEN** both replies hold `verified` 0, `principal` 0 and no ServerSignature

#### Scenario: No frame carries a stored key
- **WHEN** a Challenge and a Verify for `ann` are answered
- **THEN** neither reply holds `ann`'s stored key or server key, and no other command the endpoint serves returns them

### Requirement: The system group builds the catalog versions it is asked for
The system group SHALL serve Declare (`PROTOCOL.md` section 13.27): of kind 1 it SHALL build a catalog version from
the transaction's declarations on the version the request names, and of kind 2 the version a committed Drop calls
for, commit it as pending, and answer as `PROTOCOL.md` section 11.13 states, holding the stream open until then: for
kind 1 once the version's epoch is assigned and, for a tightening, once every partition has passed its check; for kind
2 once the version is pending and durable, or at once with `version` 0 when the Drop changes nothing in the catalog.
The wait of a tightening's check, its withdrawal and the restoring version SHALL be the group's (metadata-group, "The
metadata group holds a tightening's check"), so a requester whose connection closes under a Declare changes nothing
about them. A Declare for a Drop of kind 4 SHALL remove the tenant's default-entry records in the same command. It SHALL serve Defaults (section 13.28), answering the predicates a tenant holds default-entry records for, in the order of their texts' bytes, and a Catalog request of `version` 0 naming a closed epoch (section 13.7), answering the version a client may read at that epoch, which leaves out a version being checked and one withdrawn. The hosting binary SHALL link the catalog's model, its decoder, its encoder, the comparison of two versions and the application of declaration records and drops, and no parser of UGS or query text.

#### Scenario: A declaring transaction's version is built on the version it names
- **WHEN** version 3 of `t1` is current and a Declare of kind 1 names `built_on` 3 with one declaration
- **THEN** the group commits version 4 as pending, and answers once a close has assigned its epoch, `version` 4 and
  `commit_version` the first version of that epoch

#### Scenario: A version built on a stale one is refused
- **WHEN** version 3 of `t1` is current and a Declare of kind 1 names `built_on` 2
- **THEN** it is refused with code 2, kind 5, the message naming versions 2 and 3, and version 3 stays current

#### Scenario: A Drop that changes nothing builds no version
- **WHEN** a Declare of kind 2 names a Drop of a predicate the catalog does not declare
- **THEN** the reply holds `version` 0 at once, and the tenant's current version is unchanged

#### Scenario: A check outlives the connection that asked for it
- **WHEN** a Declare of kind 1 tightens a predicate, its connection closes before any partition reports, and a
  partition then reports that the data refuses the version
- **THEN** the group holds a version restoring the previous declaration, and a Catalog request of `version` 0 naming the latest closed epoch never returns the tightened one

#### Scenario: Defaults lists the recorded predicates in order
- **WHEN** `t1` holds default-entry records for `score` and `nick`, and a Defaults names `t1`
- **THEN** the reply lists `nick` before `score`, and `position` names the log position they were read at

#### Scenario: The storage binary links no schema compiler
- **WHEN** the storage binary's dependency graph is listed
- **THEN** it names no crate under `ultragraphdb_schema/`

### Requirement: The hosting process starts the group first and stops it last
The hosting process SHALL start the system group, wait until it has recovered and listens, and then start its
partitions, since a partition's recovery reads the catalog versions its log names from the group. Its `/ready` SHALL
answer once the group listens and its partitions have recovered and joined, with no frontend and no compute process
running. On SIGTERM it SHALL apply and sync what its partitions' logs hold, then stop the group once the group's logs
are synced. The hosting process SHALL exit for no lost peer; a failed log of the group SHALL stop it with code 74.

#### Scenario: Storage is ready before any frontend exists
- **WHEN** the config service and the hosting storage process start, and no other role does
- **THEN** the storage process's `/ready` answers

#### Scenario: A partition recovers from the group of its own process
- **WHEN** the hosting process restarts with a partition whose log names catalog version 3
- **THEN** the group recovers first, the partition reads version 3 with Catalog and then replays

#### Scenario: A failed log of the group stops the process
- **WHEN** a write to the metadata log fails
- **THEN** the hosting process shuts down through its ordinary path and exits with code 74

### Requirement: A lost system group is waited for until the deadline
A process that dials the system group, a storage process not hosting it, a compute process or the frontend, SHALL
dial again when its connection closes, and SHALL exit with code 69 once the group has been gone for
`[cluster] join_deadline_ms`, naming the address. Until then the frontend SHALL refuse what needs the group with code 17 (Unavailable): a request through a session, cancelling the Bind stream of a transaction whose Check went unanswered, a Register, a declaring Commit and a Describe; and SHALL answer an authentication it cannot relay with Error 17 on stream 0, closing the connection and counting no failure, so an outage bans no source. A Begin, a read and a commit through no session SHALL go on from its view.

#### Scenario: A frontend started before storage waits
- **WHEN** the frontend starts while no process listens at `cluster.system_address`, and the hosting storage process
  starts within the deadline
- **THEN** the frontend dials until it is answered, and its `/ready` answers after storage's

#### Scenario: A group gone past the deadline ends its dependants
- **WHEN** the hosting storage process is killed and does not return within `join_deadline_ms`
- **THEN** the frontend and compute each shut down through their ordinary path and exit with code 69 naming the
  system group's address

#### Scenario: A commit during the outage is refused, not lost
- **WHEN** the frontend's connection to the group is closed and a client commits through its session
- **THEN** the Commit is answered code 17 and reaches no partition, and the same Commit succeeds once the group answers
  again

#### Scenario: An outage bans no source
- **WHEN** one address authenticates 80 times while the system group cannot be reached, and once after it answers again
- **THEN** each of the 80 is answered Error 17 and counted as no failure, and the last authenticates
