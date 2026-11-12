## ADDED Requirements

### Requirement: The endpoint asks the system group
The frontend SHALL hold no copy of what the system group decides. Each endpoint core SHALL hold a role connection of
its own to the system group at `cluster.system_address` and SHALL ask on it: Session to register a session whose
identity it drew, answering the client's Register once the Session is answered; Check before it routes a Commit, a
Drop, a DirectWrite, a ConditionalWrite or an Ingest through a session, kind 2, and before it asks a partition a
Recall, kind 1, routing the request only once its check passed and answering the client with the code a failed check
gave; and Ended when a checked request ended before reaching a partition, its partition answered it, for an Ingest its last share's, or the role connection carrying it closed. When every partition has answered a Recall with 18, the core SHALL send a second Check of kind 1 and answer 12 on outcome 12, else 18. A core SHALL gather the Endeds of one turn into one frame and then the Checks of that turn into one frame, one check per session, a further request through a session waiting for the next frame, so an eviction takes effect for the next commit on any core and a commit's check costs one round trip per turn. A core whose connection to the system group closed SHALL refuse what needs a check with code 17 until it has dialled again (system-group, "A lost system group is waited for until the deadline"), and SHALL send the Ended of a request that ends meanwhile on its next connection.

#### Scenario: An eviction reaches every core at once
- **WHEN** a session is evicted while a connection on another core holds it, and that connection commits next
- **THEN** the commit is refused with the eviction's error, and no partition sees it

#### Scenario: A blocked commit does not block other connections
- **WHEN** one connection's commit waits on its Check
- **THEN** reads and writes of other connections on that core go on

#### Scenario: One turn's commits share one Check frame
- **WHEN** eight connections of one endpoint core each send a Commit through a session of their own within one turn
- **THEN** the core sends one Check frame holding eight checks, in ascending order of their sessions, and routes each
  Commit by its own outcome

#### Scenario: A restarted frontend serves the sessions it served before
- **WHEN** a client registers a session and commits request 1, the frontend is killed and restarted, and the client
  reconnects and commits request 2 through the same session
- **THEN** request 2 commits, and the frontend replayed no log and read no file of its own

#### Scenario: A request that ends during an outage is ended on the next connection
- **WHEN** a core's connection to the system group closes while a Commit of a session is at its partition, the
  partition answers the Commit, and the core dials again
- **THEN** the client receives the Commit's reply, the core sends an Ended naming the session on the new connection,
  and the session's next Commit passes its check

#### Scenario: A resend on another core is told the original is on its way
- **WHEN** a Commit of a session is at its partition, its reply not yet sent, and the client resends it on a connection
  to another endpoint core
- **THEN** the resend is answered code 14, and no partition is asked a Recall

#### Scenario: Two requests of one session in one turn take two frames
- **WHEN** a Commit and a resend through one session arrive at one core within one turn
- **THEN** the turn's Check frame holds one check of that session, the second request's check goes in the next frame,
  and the core answers neither by itself

### Requirement: An endpoint core keeps a view of what the closes announce
Each endpoint core SHALL subscribe to the sequencer's closes on its connection to the system group, with `latest` set since it holds no close of its own, opening a subscription the sequencer ended as no longer held the same way, and SHALL keep a view: the latest closed and the open epoch, the placement in effect with the tenants its runs list, read again with
Placement when a close assigns a placement version, each tenant's latest catalog version as the closes and its own
Declare replies name it, the versions it read with Catalog, and each version's schema id, which it computes from the
version's bytes. A Begin and the gate's question whether a tenant exists SHALL be answered from the view, asking no other process, so one connection's snapshots never go back while two connections may answer at an epoch and then the one before. A token whose epoch is above the core's open epoch SHALL not be refused: a Begin naming it SHALL be held until the core's stream delivers the close before the token's epoch and refused with code 24 at its deadline, and a one-frame write naming it SHALL go to its partition. A Describe SHALL ask the system group which version to report, by a Catalog request of `version` 0 naming the latest closed epoch. Nothing SHALL be decided from the view alone: a declaring Commit built on a version the
view holds is refused by the system group when that version is no longer current (schema-changes), and the frontend
SHALL pass the refusal on.

#### Scenario: A Begin asks no other process
- **WHEN** an authenticated client sends a Begin naming tenant 0
- **THEN** the frontend answers with the read version of the latest close its view holds, and sends no frame to the
  system group for it

#### Scenario: A stale view decides nothing
- **WHEN** one core commits a declaring transaction that makes version 4 of `t1`, and before the next close another
  core commits a declaring transaction built on version 3
- **THEN** the second Commit is refused with code 2, kind 5, the message naming versions 3 and 4, and the client's
  retry after the close builds on version 4

#### Scenario: A core that starts late begins at the latest close
- **WHEN** an endpoint core subscribes with `latest` after the sequencer has closed 40 epochs and trimmed the first 30
- **THEN** the first close on its stream is that of epoch 40, and its view holds epoch 40 before it answers its first
  Begin

#### Scenario: A token ahead of the core's view waits and is not refused
- **WHEN** a client's commit returned a token of epoch 9 while the core's stream has delivered the close of epoch 7, and
  the client sends a Begin naming the token on that core
- **THEN** the Begin is answered once the close of epoch 8 arrives, and is refused with code 24 only if its deadline
  passes first

#### Scenario: Describe never reports a version being checked
- **WHEN** a tightening of `nick` is being checked, and a client describes the tenant on a core that did not send the
  Declare
- **THEN** the Describe reports the version before it

### Requirement: The frontend finds the roles in Members
The frontend SHALL accept no role connection and SHALL send no Join. It SHALL read Members from the system group at
its start, again until Members lists every partition of the placement or `[cluster] join_deadline_ms` has passed,
again when one of its role connections closes, and again before it refuses a read for want of a compute process; and
each endpoint core SHALL dial the addresses Members names. A compute process that joined while others served SHALL be
found at the next of those reads.

#### Scenario: The frontend listens at no role address
- **WHEN** the frontend has started
- **THEN** it listens at its client address and its observability address alone

#### Scenario: A storage process restarted at its address is dialled again
- **WHEN** a storage process restarts at the same role address while the frontend runs
- **THEN** the frontend's connection to it closes, the frontend reads Members again and dials the address, and its next
  write to that partition is answered

#### Scenario: A compute process that joins later is found before a read is refused
- **WHEN** the only compute process the frontend knows stops, another has joined the system group meanwhile, and a
  client sends a Read
- **THEN** the frontend reads Members, dials the new process and relays the Read to it

## MODIFIED Requirements

### Requirement: Connections authenticate against the metadata group's principals
A connection SHALL authenticate with SCRAM-SHA-256 against a principal the metadata group holds before any data request,
and every request SHALL run with that principal's rights. The frontend SHALL hold no principal's verifier: for each
exchange it SHALL send the name in a Challenge and the AuthMessage and the client's proof in a Verify to the system
group, and pass on the salt, the iteration count and the ServerSignature it answers (system-group, "A client's proof is
checked in the system group"). A failed authentication SHALL be answered with the wire's authentication error without
saying whether the name or the proof was wrong. The frontend SHALL count each source's consecutive failed
authentications, in one of its runtimes for all of them, a source an IPv4 address or an IPv6 /64, a success clearing its
count: past `[endpoint] auth_failures_before_throttle` failures, 50 by default, each attempt of the source SHALL wait a
delay doubling from 100 ms to at most 30 s; past `auth_throttled_failures_before_ban` more, 20 by default, the source
SHALL be banned for `auth_ban_ms`, 24 hours by default, each of its connections closed as accepted, before TLS or any
frame, until the ban ends on its own. The counts SHALL be one for every endpoint core of a process, and each process
holding a client endpoint SHALL count for itself. The server SHALL answer an unknown name with a verifier derived from
the secret the metadata group holds (metadata-group, "Principals live in the metadata group"), so a name's salt tells no
prober whether it exists.

#### Scenario: A wrong password is refused without detail
- **WHEN** a client authenticates as `ann` with a wrong password, and as an unknown name
- **THEN** both are refused with the same authentication error

#### Scenario: The frontend never holds a stored key
- **WHEN** a client authenticates as `ann` and the frames the frontend receives from the system group are read
- **THEN** they hold `ann`'s salt, iteration count, principal and the exchange's ServerSignature, and neither her
  stored key nor her server key

#### Scenario: A data request before authentication is refused
- **WHEN** a client sends a Read before authenticating
- **THEN** the request is refused with the authentication error, and the connection stays open for authentication

#### Scenario: An unknown name answers as a known one does
- **WHEN** a client asks to authenticate as an unknown name, twice, across a restart of the frontend
- **THEN** both challenges carry the same salt and iteration count, and the exchange fails with the same authentication
  error as a wrong password

#### Scenario: A source past its failures is throttled, then banned
- **WHEN** one address fails to authenticate past the throttle's threshold, and then past the ban's
- **THEN** its attempts past the first threshold wait a growing delay, its connections past the second are closed as accepted,
  another address still authenticates, and once the ban's time has passed the address authenticates again

## REMOVED Requirements

### Requirement: One owner answers for the metadata group
**Reason**: The metadata group, the sequencer and the session registry leave the frontend for the system group, which
a storage process hosts (stage 5.8). The frontend has no owning core.
**Migration**: What an endpoint core asked its owner by message it asks the system group by role command
("The endpoint asks the system group"); answering a commit's check while a log syncs is the group's (system-group,
"The system group goes on answering checks while a log syncs"); the tenant's facts come from the core's view ("An
endpoint core keeps a view of what the closes announce").
