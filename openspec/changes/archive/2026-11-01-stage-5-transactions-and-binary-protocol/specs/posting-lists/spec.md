## ADDED Requirements

### Requirement: Posting lists are stored in the repository's own format
A posting list SHALL be stored in a byte format of the repository's own, defined in `ultragraphdb_core_codec` with its
types: a version byte, flags, the commit timestamp, the UID pack's blocks, the postings and the splits, little-endian,
followed by an XXH3-64 checksum of the bytes before it. A stored value whose version is unknown, whose checksum fails or
whose lengths overrun SHALL be refused as corruption naming its key, never decoded in part.

#### Scenario: A list round-trips
- **WHEN** a list with a UID pack, value postings with statement IDs and splits is encoded and decoded
- **THEN** the decoded list equals the original

#### Scenario: A flipped byte is corruption
- **WHEN** one byte of a stored list is flipped and the list is read
- **THEN** the read fails with a corruption error naming the key
