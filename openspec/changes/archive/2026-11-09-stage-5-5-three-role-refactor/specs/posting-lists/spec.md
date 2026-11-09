## MODIFIED Requirements

### Requirement: Posting lists are stored in the repository's own format
A posting list SHALL be stored in a byte format of the repository's own, defined in `ultragraphdb_core_codec` with its
types: a version byte, flags, the commit version, the packed ID list's blocks, the postings and the splits,
little-endian but for each 128-bit value, which is written as its 16 big-endian bytes, followed by an XXH3-64 checksum
of the bytes before it. IDs SHALL be 128-bit system IDs, and the version byte SHALL be the one this stage's format
introduces. A stored value whose version is unknown, whose checksum fails or whose lengths overrun SHALL be refused as
corruption naming its key, never decoded in part.

#### Scenario: A list round-trips
- **WHEN** a list with a packed ID list, value postings with statement IDs and splits is encoded and decoded
- **THEN** the decoded list equals the original

#### Scenario: A flipped byte is corruption
- **WHEN** one byte of a stored list is flipped and the list is read
- **THEN** the read fails with a corruption error naming the key

#### Scenario: A list of the previous format is refused
- **WHEN** a value written with stage 5's version byte is read
- **THEN** the read fails with a corruption error naming the key and the unknown version
