## ADDED Requirements

### Requirement: The rebalancer moves the right tablet
Every rebalance interval (8 minutes by default) the Zero leader SHALL, given two or more groups, move from the
largest group to the smallest the largest tablet no bigger than half their difference, and SHALL do nothing when the
difference is under 10% of the smallest group.

#### Scenario: A small difference moves nothing
- **WHEN** two groups differ in size by 5%
- **THEN** no move starts

#### Scenario: The largest fitting tablet moves
- **WHEN** group 1 holds tablets of 10, 40 and 90 GB, group 2 holds 20 GB, and a rebalance runs
- **THEN** the 40 GB tablet moves to group 2

### Requirement: Rebalance interval and backoff
An interval of 0 SHALL disable rebalancing and a negative interval SHALL be refused at startup; a move that failed
after running at least a minute SHALL back its tablet off for 1 hour, doubling per failure up to 24 hours; a manual
move SHALL ignore the backoff.

#### Scenario: Failed moves back off
- **WHEN** a tablet's moves fail three times, each after more than a minute
- **THEN** the rebalancer skips it for 4 hours after the third failure

#### Scenario: A manual move ignores the backoff
- **WHEN** a superadmin moves a backed-off tablet
- **THEN** the move starts
