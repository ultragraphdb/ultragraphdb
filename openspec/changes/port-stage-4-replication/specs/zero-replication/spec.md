## ADDED Requirements

### Requirement: Zero's state is replicated
Three Zero replicas SHALL replicate the membership state, the leases' high-water marks and the cluster ID through
Raft, and only the leader SHALL serve leases and the watermark stream. A new Zero leader SHALL issue leases above its
predecessor's high-water marks, and SHALL not advance its done watermark past them until every group's leader has
reported the leased timestamps it holds open.

#### Scenario: A new Zero leader resumes above the old leases
- **WHEN** the Zero leader is killed after leasing uids and timestamps
- **THEN** the new leader issues uids and timestamps above every one the old leader issued

#### Scenario: A Zero leader without quorum stops serving
- **WHEN** a Zero leader is partitioned from both other Zero replicas
- **THEN** within 5 s of its last quorum contact it refuses leases and closes its watermark streams, where Dgraph
  keeps open streams running

#### Scenario: The watermark survives a leader change
- **WHEN** the Zero leader changes while commits are in flight
- **THEN** no Alpha's done watermark moves backwards, none passes a timestamp still open, and Alphas resubscribe
  within 3 s

### Requirement: Commit leases go to group leaders alone
Zero SHALL refuse a commit-timestamp lease to a node its membership does not name as its group's leader, so that an
isolated former leader cannot keep leasing timestamps that never commit.

#### Scenario: A deposed leader cannot lease commit timestamps
- **WHEN** a group's former leader, partitioned from its group but not from Zero, asks Zero for commit timestamps
  after a new leader is reported
- **THEN** Zero refuses the lease

### Requirement: Zero leadership is read without the Raft loop
Zero's leadership check SHALL read an atomic the Raft loop publishes, together with the time of the last quorum
contact.

#### Scenario: A lease request does not wait on the Raft loop
- **WHEN** Zero serves 100,000 timestamp requests
- **THEN** none of its leadership checks waits on the Raft loop
