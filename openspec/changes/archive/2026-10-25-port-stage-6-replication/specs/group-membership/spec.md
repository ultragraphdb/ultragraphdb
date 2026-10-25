## ADDED Requirements

### Requirement: Joining a group
A new replica SHALL connect to Zero for its Raft ID and group, then join through `JoinCluster` on the group's leader,
which SHALL add it as a voter, or as a learner when it asks to be one. Zero SHALL refuse an address another member
holds under a different ID; Zero and the leader SHALL refuse a Raft ID a member holds at another address while that
member's connection is live; the leader SHALL refuse another group's node and its own ID.

#### Scenario: A new replica becomes a voter
- **WHEN** a third replica joins a group of two with replicas set to 3
- **THEN** it receives the group's data and counts toward the quorum

#### Scenario: A learner does not count toward the quorum
- **WHEN** a learner joins a group of three voters and two voters are then stopped
- **THEN** the learner holds the data and writes are not acknowledged

#### Scenario: A live member's Raft ID is refused
- **WHEN** a node joins with a Raft ID that a live member holds at another address
- **THEN** the join is refused

### Requirement: Removal through Zero
Removing a node through Zero SHALL make the group's leader propose its removal, and a node no longer in the
membership SHALL stop.

#### Scenario: A removed node stops
- **WHEN** Zero removes a replica of a group
- **THEN** the group's configuration no longer holds it and the removed process exits

### Requirement: Membership from Zero
Alphas SHALL receive membership from Zero's stream and SHALL apply a state only if its counter is not lower than the
one they hold; each Alpha SHALL report its ID, group, address and leadership to the Zero leader at least every 10 s
and on a leadership change, and the group's leader SHALL add its snapshot and applied timestamps.

#### Scenario: An older membership state is ignored
- **WHEN** an Alpha holding membership counter 40 receives a state with counter 39
- **THEN** it keeps counter 40's state

### Requirement: Leadership is handed over at shutdown
An Alpha leader shutting down SHALL transfer leadership to a voter and wait until it observes the new leader or 2 s
pass; Dgraph waits 1 s without observing.

#### Scenario: Shutting down the leader moves leadership
- **WHEN** an Alpha leader receives a shutdown request
- **THEN** another replica becomes leader, the old leader exits, and writes continue
