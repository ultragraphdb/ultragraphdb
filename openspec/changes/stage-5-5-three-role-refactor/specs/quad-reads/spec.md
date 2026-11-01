## ADDED Requirements

### Requirement: Compute executes quad-pattern reads over storage
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, executed by compute,
which asks the partitions holding the pattern's keys for their rows through storage's read kernels and returns the rows
in the store's key order. A pattern with a bound subject SHALL ask that subject's partition alone; a pattern with an
unbound subject SHALL ask every partition and merge their rows in key order.

#### Scenario: A bound subject asks one partition
- **WHEN** a read binds its subject to `http://ex.org/a` over two partitions
- **THEN** compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

#### Scenario: An unbound subject merges every partition
- **WHEN** a read leaves its subject unbound over two partitions that each hold matching rows
- **THEN** compute returns the rows of both partitions merged in key order

### Requirement: Access paths follow the bound positions
A read SHALL choose its keyspace by its bound positions: the forward keyspace for a bound subject and predicate, the
reverse keyspace for a bound object and predicate, the subject-predicates list for a bound subject alone, and the
object-predicates list for a bound object alone; a pattern binding no position SHALL scan the forward keyspace of each
predicate. An unbound graph SHALL read every graph; statement IDs SHALL be returned only when the read asks for them.

#### Scenario: A bound object reads the reverse keyspace
- **WHEN** a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`
- **THEN** it reads the reverse list of `http://ex.org/b` under `http://ex.org/knows`, and returns each subject

#### Scenario: An unbound graph reads every graph
- **WHEN** `<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound
- **THEN** it returns both statements, each with its graph

### Requirement: Resume keys continue their own read
A read reaching its limit SHALL return a resume key that continues that read alone, after its last row; a resume key
used with another pattern SHALL be refused. A read matching nothing SHALL return no rows and no resume key.

#### Scenario: A read resumes where it stopped
- **WHEN** a read with limit 2 matches 5 rows and is resumed twice
- **THEN** the three answers return rows 1–2, 3–4 and 5, the last with no resume key

#### Scenario: A resume key with another pattern is refused
- **WHEN** a resume key from one read is used with a different pattern
- **THEN** the read is refused with an error naming the resume key

#### Scenario: The empty result
- **WHEN** a read matches nothing
- **THEN** it returns no rows and no resume key

### Requirement: A read names its level
A read SHALL name its level: the latest state of one partition, a snapshot at a closed epoch (the default), or
monotonic-fast. A snapshot read SHALL wait until every partition it asks has closed the epoch and SHALL read at it; a
read carrying a session token SHALL read at an epoch at or above the token's; a latest-state read SHALL be refused when
its pattern reaches more than one partition. Quad-pattern reads are monotonic, so a monotonic-fast read SHALL answer from
the partitions' latest closed epochs without waiting.

#### Scenario: A read after a commit sees it through its token
- **WHEN** a client commits a write, receives a session token, and reads with that token
- **THEN** the read returns the write

#### Scenario: A latest-state read across partitions is refused
- **WHEN** a latest-state read leaves its subject unbound over two partitions
- **THEN** the read is refused with an error naming the level and the two partitions
