## ADDED Requirements

### Requirement: Joining a group
A new replica SHALL join through `JoinCluster` on the leader, which SHALL add it as a voter, or as a learner when it
asks to be one, and SHALL refuse a duplicate Raft ID, another group's node, or an address a live peer holds.

#### Scenario: A new replica becomes a voter
- **WHEN** a fourth replica joins a group of three
- **THEN** it receives the group's data and counts toward the quorum

#### Scenario: A learner does not count toward the quorum
- **WHEN** a learner joins and two voters are then stopped
- **THEN** the learner holds the data and writes are not acknowledged

#### Scenario: A duplicate Raft ID is refused
- **WHEN** a node joins with a Raft ID a member holds
- **THEN** the join is refused

### Requirement: Removal through Zero
Removing a node through Zero SHALL make the group's leader propose its removal, and a node no longer in the
membership SHALL stop.

#### Scenario: A removed node stops
- **WHEN** Zero removes a replica of a group
- **THEN** the group's configuration no longer holds it and the removed process exits

### Requirement: Membership from Zero
Alphas SHALL receive membership from Zero's stream and SHALL apply a state only if its counter is not lower than the
one they hold; they SHALL report their members, leader and checkpoint to Zero at least every 10 s.

#### Scenario: An older membership state is ignored
- **WHEN** an Alpha holding membership counter 40 receives a state with counter 39
- **THEN** it keeps counter 40's state

### Requirement: Leadership is handed over at shutdown
A leader shutting down SHALL transfer leadership to a peer before it stops.

#### Scenario: Shutting down the leader moves leadership
- **WHEN** the leader receives a shutdown request
- **THEN** another replica becomes leader before the old leader exits, and writes continue
