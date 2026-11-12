## MODIFIED Requirements

### Requirement: Commits are grouped into rounds
The commit step SHALL close a round when the previous round's log sync completes, or at once when no sync is in
progress, with no timer, holding the commit requests waiting then, bounded by the maximum frame and a transaction count;
requests past the bounds wait for the next round. A round SHALL be validated, logged, synced, applied and acknowledged
as a unit, in round order. The commits a partition's role connections deliver while a sync is in progress SHALL be among
those waiting when it completes, however many arrive on one connection. A caller handing over several requests together,
as a partition's core hands over what one turn takes from its inbox, SHALL have them share a round: while it hands them
over, a request arriving with no round in flight waits, and the round closes once they have all arrived.

#### Scenario: A lone commit waits about one sync
- **WHEN** one transaction commits on an idle node
- **THEN** it forms a round of its own and is acknowledged after one log sync and its apply

#### Scenario: Commits arriving during a sync share the next round
- **WHEN** ten transactions commit while a round's sync is in progress
- **THEN** they are decided in the next round, written with one log append and one sync

#### Scenario: Commits a compute node sends during a sync share the next round
- **WHEN** a partition's log sync is in progress and compute sends ten DirectWrites, each a commit of its own, over one role
  connection
- **THEN** the ten are decided in the next round, written with one log append and one sync, and each is acknowledged once that
  round is applied

#### Scenario: Commits a turn takes together share one round
- **WHEN** a partition's core takes ten commits in one turn with no round in flight
- **THEN** they are decided in one round, written with one log append and one sync

#### Scenario: Through the binaries a sync carries the commits of many connections
- **WHEN** sixteen client connections each commit DirectWrites one after another through compute to one partition
- **THEN** the partition's phase log counts fewer log syncs than commits over the run
