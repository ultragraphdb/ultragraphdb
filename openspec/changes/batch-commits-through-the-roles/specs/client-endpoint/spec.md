## MODIFIED Requirements

### Requirement: Connections authenticate against the metadata group's principals
A connection SHALL authenticate with SCRAM-SHA-256 against a principal the metadata group holds before any data
request, and every request SHALL run with that principal's rights. A failed authentication SHALL be answered with the
wire's authentication error without saying whether the name or the proof was wrong. The frontend's owner SHALL count each
source's consecutive failed authentications, a source an IPv4 address or an IPv6 /64, a success clearing its count: past
`[endpoint] auth_failures_before_throttle` failures, 50 by default, each attempt of the source SHALL wait a delay doubling from
100 ms to at most 30 s; past `auth_throttled_failures_before_ban` more, 20 by default, the source SHALL be banned for `auth_ban_ms`,
24 hours by default, each of its connections closed as accepted, before TLS or any frame, until the ban ends on its own. The counts
SHALL be one for every endpoint core. The server SHALL answer an unknown name with a verifier derived from the secret the metadata
group holds (metadata-group, "Principals live in the metadata group"), so a name's salt tells no prober whether it exists.

#### Scenario: A wrong password is refused without detail
- **WHEN** a client authenticates as `ann` with a wrong password, and as an unknown name
- **THEN** both are refused with the same authentication error

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

### Requirement: One owner answers for the metadata group
The frontend SHALL run its metadata group, sequencer and session registry on one owning core, and its connections on
endpoint cores, which SHALL hold no copy of what the owner decides. An endpoint core SHALL ask the owner by message for a
session's owner check on every commit and for a principal's verifier on every authentication, so an eviction takes effect
for the next commit on any core. An endpoint core MAY gather the checks of one turn into one message. While the owner waits
on a sync of the metadata group's log or of the sequencer's, it SHALL go on answering what a commit asks of it, the
tenant's facts and the owner check, from what is durable: the metadata group's durable state, its registry with every
eviction appended, and the last close the sequencer's log holds durable. It SHALL answer every other ask once the sync has
ended, in the order the asks came.

#### Scenario: An eviction reaches every core at once
- **WHEN** a session is evicted while a connection on another core holds it, and that connection commits next
- **THEN** the commit is refused with the eviction's error, and no partition sees it

#### Scenario: A blocked commit does not block other connections
- **WHEN** one connection's commit waits on its owner check
- **THEN** reads and writes of other connections on that core go on

#### Scenario: A commit's checks are answered while a close syncs
- **WHEN** the sequencer's sync of a close is in flight, and a commit asks for its tenant's facts and its owner check, and a
  session's registration is asked
- **THEN** the facts name the close before it as the last closed and the check is answered before the sync ends, and the
  registration is answered only after it
