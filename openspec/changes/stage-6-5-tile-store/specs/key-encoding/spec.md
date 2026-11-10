## ADDED Requirements

### Requirement: Conflict keys name logical families
A conflict key SHALL be laid out as `PROTOCOL.md` section 12.2 states: the tenant as a big-endian u64, the graph's
system ID, the predicate's system ID, the conflict family's byte, then the conflict family's key, a family with no graph
or no predicate holding 16 bytes of `0xFF` in its place. The eight conflict families SHALL be forward row (1), reverse
row (2), index token (3), count (4), statement (5), mark (6), claim (7) and session (8), each keyed as that section's
table gives it, and none SHALL name a tile, a family of the store, a slot or a bucket group's placement, so a tile's
split or move changes no conflict (T38; T48, C48). Keys SHALL compare byte by byte, so a tenant's keys, a graph's in a
tenant, a predicate's in a graph and a bucket group's forward rows of a predicate are each one range. A range's past key
SHALL be the least key above every key its prefix begins: the prefix with its last byte below `0xFF` raised by one and
the bytes after it dropped, or empty when every byte is `0xFF`. A drop SHALL cover the conflict ranges section 12.2
gives its kind, a predicate's covering its `@unique` claims. A key whose family byte, at offset 40, is unknown, whose
graph or predicate is `0xFF` × 16 in families 1 to 5, or whose graph or predicate is not `0xFF` × 16 in families 6 and
8, SHALL be refused as `FieldValue` naming `conflict key`.

#### Scenario: A bucket group's forward rows are one range
- **WHEN** the forward-row conflict keys of one predicate are built for subjects of bucket groups 3 and 4, two subjects
  in each
- **THEN** the two keys of group 3 sort before both keys of group 4, and a range from group 3's prefix to its past key
  holds exactly group 3's keys

#### Scenario: A family with no graph sorts after every graph
- **WHEN** a mark's conflict key and a forward row's conflict key of the same tenant are built
- **THEN** the mark's key holds `0xFF` × 16 in place of the graph and the predicate, and sorts after the forward row's

#### Scenario: A past key drops the bytes after the raised byte
- **WHEN** the past key of the prefix `01 02 FF FF` is built, and of a prefix of four bytes `0xFF`
- **THEN** the first is `01 03`, and the second is empty

#### Scenario: A predicate drop covers its claims
- **WHEN** a drop of predicate `p` in graph `g` builds its conflict ranges, and a `@unique` claim of `p` in `g` is built
- **THEN** the claim's conflict key lies inside the drop's range of the tenant, `g` and `p`

#### Scenario: A malformed conflict key is refused
- **WHEN** a write-set record holds a conflict key whose byte at offset 40 is 9, and another of family 6 whose graph is a
  system ID
- **THEN** each is refused with `FieldValue` naming `conflict key`

### Requirement: Record keys are logical
A record's key SHALL be the one its kind gives in `STORAGE.md` section 9.3, made of system IDs, hashes, term encodings,
sortable values and integers, each integer big-endian, so records of one kind compare as their logical values: a term
to ID record's key the XXH3-64 of the term's name bytes as a u64 followed by the name bytes; an ID to term record's, a
referrer set's, a tombstone's and a purge marker's key the system ID; a mark's the entity's system ID and the referring
bucket group as 3 bytes; a key claim's the key space's type prefix, the graph's system ID and the key's
canonical encoding (`SCHEMA.md` section 7.2); a `@unique` claim's the predicate's system ID and the value's comparison
form in a compiled layout or its term encoding in the generic layout; a statement record's the statement's system ID
alone. No record key, conflict key, resume key or answer SHALL name a slot: slots stay inside the replica that gave them
(`STORAGE.md` section 14.6). Parsing a record key SHALL return its kind's fields, and a key its kind cannot parse whole
SHALL be refused with an error naming the key's bytes.

#### Scenario: A term to ID key holds its hash and its name bytes
- **WHEN** the term to ID key of the IRI `http://ex.org/a` is built
- **THEN** it is the XXH3-64 of the byte 1 followed by `http://ex.org/a` as a big-endian u64, then the byte 1, then
  `http://ex.org/a`

#### Scenario: A key claim names its graph
- **WHEN** the key claims of one key space and one key are built in graphs `<g1>` and `<g2>`
- **THEN** the two keys differ in bytes 2 to 17, which hold each graph's system ID, and agree elsewhere

#### Scenario: A temporal `@unique` value claims by its instant
- **WHEN** the `@unique` claim keys of one predicate are built in a compiled layout for `12:00Z` and `13:00+01:00`
- **THEN** both keys are equal

#### Scenario: A statement record is keyed by its ID alone
- **WHEN** the statement record key of a statement held in graph `<g1>` is built
- **THEN** it is the statement's 16-byte system ID, and holds no graph

## MODIFIED Requirements

### Requirement: A truncated key is refused
Parsing a truncated conflict key or record key SHALL fail with an error naming the key's bytes, and SHALL not panic. A
truncated key is any proper prefix of a valid key, other than one that is itself a valid key whose last field may be
empty.

#### Scenario: Every prefix of a key fails cleanly
- **WHEN** every proper prefix of a forward-row, reverse-row, index-token and statement conflict key, and of a term to
  ID, key claim and statement record key, is parsed
- **THEN** each parse fails with an error, and nothing panics

#### Scenario: Every prefix of a 128-bit ID field fails cleanly
- **WHEN** a forward-row conflict key is cut inside its subject's system ID, at each of the ID's 15 inner positions, and
  parsed
- **THEN** each parse fails with an error naming the key's bytes

## REMOVED Requirements

### Requirement: Keys parse back into their parts
**Reason**: The KV keyspaces, their keyspace bytes and their regions are removed with the KV store; a tile is found by
its tile key and family (`STORAGE.md` section 13.1), and no KV key layout remains.
**Migration**: "Record keys are logical" and "Conflict keys name logical families"; the tile key is `tile-format`'s.
Marks and referrer sets are records of kinds 6 and 7.

### Requirement: Split keys address the parts of a split posting list
**Reason**: Posting lists and their split keys are removed; a row spans adjacency blocks of its family, each found by
its fence (`STORAGE.md` sections 7.5 and 13.2).
**Migration**: `tile-format` and `tile-engine`.
