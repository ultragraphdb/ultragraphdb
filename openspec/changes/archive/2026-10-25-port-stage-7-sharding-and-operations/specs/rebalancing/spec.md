## ADDED Requirements

### Requirement: The rebalancer moves the right tablet
Every rebalance interval (8 minutes by default) the Zero leader SHALL, given two or more groups and a leader in the
smallest, take source groups from the largest downward and move to the smallest group the largest non-reserved,
non-backed-off tablet whose on-disk size is at most half their difference, skipping a source whose difference is under
10% of the smallest group's size.

#### Scenario: A small difference moves nothing
- **WHEN** the larger of two groups exceeds the smaller by 5% of the smaller's size
- **THEN** no move starts

#### Scenario: The largest fitting tablet moves
- **WHEN** group 1 holds tablets of 10, 40 and 90 GB, group 2 holds 20 GB, and a rebalance runs
- **THEN** the 40 GB tablet moves to group 2

### Requirement: Rebalance interval and backoff
An interval of 0 SHALL disable rebalancing and a negative interval SHALL be refused at startup. A move that failed
after running at least a minute SHALL back its tablet off for the larger of the attempt's duration and `2^(n−1)` hours
capped at 24, `n` being its consecutive such failures; a failure under a minute SHALL neither count nor reset the
count, and a successful move SHALL clear it. A manual move SHALL ignore the backoff.

#### Scenario: Failed moves back off
- **WHEN** a tablet's moves fail three times, each after more than a minute and under four hours
- **THEN** the rebalancer skips it for 4 hours after the third failure

#### Scenario: A manual move ignores the backoff
- **WHEN** a superadmin moves a backed-off tablet
- **THEN** the move starts
