## ADDED Requirements

### Requirement: Zero's state is replicated
Three Zero replicas SHALL replicate the membership state, the leases' high-water marks, the done watermark and the
cluster ID through Raft, and only the leader SHALL serve leases and the watermark stream.

#### Scenario: A new Zero leader resumes above the old leases
- **WHEN** the Zero leader is killed after leasing UIDs and timestamps
- **THEN** the new leader issues UIDs and timestamps above every one the old leader issued

#### Scenario: A Zero leader without quorum stops serving
- **WHEN** a Zero leader is partitioned from both other Zero replicas
- **THEN** within 5 s it stops serving leases and the watermark stream

#### Scenario: The watermark survives a leader change
- **WHEN** the Zero leader changes while commits are in flight
- **THEN** the new leader's done watermark never moves below the old one's, and Alphas resubscribe within about 1 s

### Requirement: Zero leadership is read without the Raft loop
Zero's leadership check SHALL read an atomic the Raft loop publishes, together with the time of the last quorum
contact.

#### Scenario: A lease request does not wait on the Raft loop
- **WHEN** Zero serves 100,000 timestamp requests
- **THEN** none of its leadership checks waits on the Raft loop
