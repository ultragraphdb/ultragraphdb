## MODIFIED Requirements

### Requirement: A parked request holds no role connection
A partition SHALL copy every request out of its connection's receive buffer as it arrives, hand the copy to its core, and go on
reading that connection without waiting for the core to take the request, as `PROTOCOL.md` section 11.12's *Parked requests*
states; it SHALL send each request's answer on its stream once it has one, whether the request runs at the core's next turn or
waits on a home. A stream's requests SHALL take effect and be answered in the order they were sent, and the answers on one
connection SHALL leave in the order the core gives them. A connection's held bytes, its requests copied and not yet taken by the
core together with those parked, SHALL be bounded by `storage.held_bytes`, at which the partition stops reading that connection
until they fall below it, refusing nothing.

#### Scenario: A write parked on a home's Reference holds no role connection
- **WHEN** a Write on a Bind stream waits on a home's answer, and a second request arrives on its connection
- **THEN** the second request is answered while the Write waits, and the Write is answered Held on its stream once the home answers

#### Scenario: A one-frame write parked on a home's Reference holds no role connection
- **WHEN** a DirectWrite waits on a home's answer, and a second request arrives on its connection
- **THEN** the second request is answered while the DirectWrite waits, and its Commit reply follows once the home answers

#### Scenario: A write behind a read's wait holds no role connection
- **WHEN** a Scan of a transaction waits on a home's answer, a Write follows on the transaction's stream, and a second request arrives on the connection
- **THEN** the second request is answered while the Write waits its turn, and the Write is answered Held once the home answers

#### Scenario: What follows a parked write on its stream keeps its order
- **WHEN** a Write waits on a home's answer, and a second Write and the Commit follow on its stream
- **THEN** nothing is answered on the stream until the home answers, and then Held, Held and the Commit, in that order, and the
  commit holds both writes

#### Scenario: A home's refusal drops what is parked behind the write
- **WHEN** a Write waits on a home's answer, a second Write follows on its stream, and the home refuses the registration
- **THEN** the stream ends with the home's refusal alone, and the transaction holds nothing

#### Scenario: A Cancel reaches a parked write
- **WHEN** a Write waits on a home's answer, and its stream is cancelled
- **THEN** the partition holds no such transaction by the time a later request on the connection is answered, and no Held follows
  once the home answers

#### Scenario: A connection closing with a write parked aborts its transaction
- **WHEN** a Write waits on a home's answer, and its connection closes
- **THEN** the partition no longer holds the transaction

#### Scenario: A connection at its parked bound stops reading
- **WHEN** a Write's frame takes its connection's parked bytes to `storage.held_bytes`, the Write waiting on a home's answer, and a
  second request follows on the connection
- **THEN** the second request is unanswered until the home answers and is then answered, and nothing is refused

#### Scenario: Writes parked on each other's homes both answer
- **WHEN** both storage processes hold `storage.held_bytes` of 1, and a Write on partition 1 names an entity partition 3 owns while
  a Write on partition 3 names an entity partition 1 owns
- **THEN** both are answered Held and both transactions commit

#### Scenario: A stop with a write parked answers or closes
- **WHEN** a storage process stops while a Write on a Bind stream waits on a home's answer
- **THEN** the stream is answered with code 17 or the connection closes

#### Scenario: A connection is read on while its core syncs
- **WHEN** a partition's log sync is in progress and its frontend sends ten DirectWrites on ten streams of one role connection
- **THEN** the partition reads all ten frames before the sync completes, and each is answered on its stream once its round is
  durable

#### Scenario: Requests waiting for the core count against the bound
- **WHEN** `storage.held_bytes` is 4,096, the core is in a sync, and the frontend sends DirectWrites of 1,500 bytes on one role
  connection
- **THEN** the partition reads three of them and stops reading that connection until the core has taken them, refusing nothing

#### Scenario: An Abort's stream ends with no frame
- **WHEN** a transaction's Abort arrives on its Bind stream while the core is in a sync
- **THEN** the stream ends with no frame once the core takes the Abort, and a later request on the connection is answered
