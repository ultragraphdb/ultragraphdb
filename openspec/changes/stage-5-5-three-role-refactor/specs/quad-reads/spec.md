## ADDED Requirements

### Requirement: Compute executes quad-pattern reads over storage
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, executed by compute, which
asks the partitions holding the pattern's entries for their rows through storage's read kernels and returns the rows in
the store's key order. A pattern with a bound subject SHALL ask that subject's partition alone; a pattern with an
unbound subject and a bound IRI or blank-node object SHALL be a reverse read ("A reverse read asks the object's
referrers"); any other pattern, a bound literal object and a value-index lookup included, SHALL ask every partition
holding a bucket of the namespace's run and merge their rows in key order, batched per role address, through a merge scan when
the frontier is large, since a literal has no home and no referrer set. A read of a read-write transaction SHALL reach
only its partition (partitions, "A read-write transaction stays within one partition in this stage").

#### Scenario: A bound subject asks one partition
- **WHEN** a read binds its subject to `http://ex.org/a` over two partitions
- **THEN** compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

#### Scenario: A bound literal object fans out
- **WHEN** `<a>` in partition 1 and `<c>` in partition 2 each hold `name` `"Ann"`, and a read binds its object to
  `"Ann"` and its predicate to `name`
- **THEN** compute asks every partition of the namespace's run, reads no referrer set, and returns `<a>` and `<c>`
  merged in key order

#### Scenario: A value-index lookup asks every partition
- **WHEN** partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and a read looks
  up `ann`
- **THEN** compute asks the three partitions and returns the three subjects merged in key order

#### Scenario: An unbound subject merges every partition
- **WHEN** a read leaves its subject and object unbound over two partitions that each hold matching rows
- **THEN** compute returns the rows of both partitions merged in key order

### Requirement: Compute runs each read on one core
Compute SHALL run one runtime per core, each holding its own connections to the frontend and the partitions, and SHALL run
a read wholly on the core that received it, sharing no mutable state between cores.

#### Scenario: A fan-out stays on its read's core
- **WHEN** a read fans out over three partitions
- **THEN** its Scans, its merge and its Result frames all run on the core its ComputeRead arrived on

### Requirement: Storage's read kernel answers what compute scans
Storage SHALL answer a Scan with the entries of its key ranges at the version the Scan names, each key once with its
value visible at that version, in key order, each range at most the Scan's limit: from a keyspace of lists, the dictionary
or the referrer sets, and no other. An answer at a closed epoch SHALL wait until the partition has closed that epoch. A
Scan whose epoch has not closed by its deadline, or with no frame able to leave for `scan_stall_ms`, SHALL be refused naming
the partition. The partition SHALL build a stream's next frame once the one before has left, so a stalled stream holds at
most one frame in its memory and the list that frame stopped inside, which it SHALL read once however many frames it takes.
No turn of the partition SHALL read more than a bounded number of keys holding nothing. A Scan SHALL read one version, chosen
once. Only a stream ending in `LAST` SHALL be an answer. A Scan naming a transaction SHALL end with that transaction.
A foreign term SHALL read by the partition's copy of its entries, or by its home's answer when it holds none; an unheld skolem
IRI SHALL answer no entry wherever it is homed, and any other refusal from the home SHALL end the transaction. Compute SHALL
decode what the entries hold and SHALL name every term by its text in what it returns.

#### Scenario: A dictionary and a referrer set are read by Scan
- **WHEN** compute scans the dictionary entry of `<b>` and `R(b)` at `b`'s home
- **THEN** the answer holds `b`'s ID and its referrer set's members at that version

#### Scenario: A hub's list crosses frames
- **WHEN** a reverse list holds more objects than one frame carries, and compute scans it
- **THEN** the answer takes several frames, each ending between two objects, and compute reads every object once, in the
  list's order

#### Scenario: A keyspace outside the kernel's is refused
- **WHEN** a Scan names a range of the marks keyspace
- **THEN** it is refused with a protocol error naming the keyspace

#### Scenario: A range of the dictionary holds one key
- **WHEN** a Scan names a range of the dictionary from one key to another
- **THEN** it is refused with a protocol error naming the range

#### Scenario: A Scan waits for the epoch it names
- **WHEN** a Scan names an epoch the partition has not closed, and the partition then closes it
- **THEN** the answer comes after the close, at that epoch

#### Scenario: A Scan past its deadline names the partition
- **WHEN** a Scan names an epoch the partition does not close before the Scan's deadline
- **THEN** it is refused with code 24 (TimedOut), naming the partition, and returns no entry

#### Scenario: A reader granting no credit stops the answer
- **WHEN** a Scan's answer takes more frames than the stream's credit, and the reader grants no more
- **THEN** it receives as many frames as its credit, and the Scan is refused with code 24 (TimedOut), naming the partition,
  once `scan_stall_ms` has passed

#### Scenario: A reader granting credit reads the rest
- **WHEN** a Scan's answer takes more frames than the stream's credit, and the reader grants credit as it reads
- **THEN** it receives every range in order, the last frame carrying `LAST`

#### Scenario: A version above the watermark is refused at once
- **WHEN** a Scan with `at` 4 names a version above the partition's visibility watermark
- **THEN** it is refused with code 27 (AheadOfWatermark), `detail` the watermark

#### Scenario: A named version in a closed epoch reads on an idle partition
- **WHEN** a partition has closed an epoch, its visibility watermark lies below that epoch's version bound, and a Scan with
  `at` 4 names that bound
- **THEN** it reads what the partition applied, and is not refused

#### Scenario: A Scan reads one version across a close
- **WHEN** a Scan with `at` 3 has two ranges, and the partition closes another epoch between their frames
- **THEN** both ranges read the version chosen first, and every frame names its epoch

#### Scenario: A term another partition owns is read through its copy
- **WHEN** a transaction on partition 1 reads a statement of a predicate partition 2 owns, partition 1 holding the copy
- **THEN** the Scan's range answers the predicate's ID, and the read returns the statement

#### Scenario: A term another partition owns and no copy holds comes from its home
- **WHEN** a transaction on partition 1 reads a predicate partition 2 owns, partition 1 holding no copy
- **THEN** partition 1 takes the ID from partition 2, the range answers it, and a later commit into the list the read scanned
  aborts the transaction

#### Scenario: A Scan that ends its transaction says so on the Bind stream
- **WHEN** a Scan's span has no room in the transaction's read set
- **THEN** the Scan's stream and the Bind stream each carry code 6 (TooLarge), and the transaction's next request is refused

#### Scenario: Closed answers each partition's latest closed epoch
- **WHEN** compute sends Closed naming the partition at its role address
- **THEN** the reply holds the partition's latest closed epoch

#### Scenario: A Scan of a partition the address does not serve is refused
- **WHEN** a Scan and a Recall name a partition the role address does not serve
- **THEN** each is refused with a protocol error naming the partition

#### Scenario: Every range is checked before any frame
- **WHEN** Scans hold a valid range and then one of the marks keyspace, of a split part, of an unknown keyspace, of the
  dictionary with no past key, of the referrer sets with a first key too short, or of the referrer sets past their namespace
- **THEN** each is refused with a protocol error naming what is wrong, and no frame precedes the refusal

#### Scenario: A record too large for a frame is refused
- **WHEN** the dictionary holds a value too large for the receiver's `max_frame`
- **THEN** a Scan of it is refused with code 6 (TooLarge), `detail` that `max_frame`

#### Scenario: A stalled Scan leaves another running
- **WHEN** one Scan of a connection holds no credit and another takes its credit as it reads
- **THEN** the second reaches `LAST` while the first takes no more frames

#### Scenario: A range of emptied lists takes many turns
- **WHEN** a range holds more emptied lists than a turn reads, and a list holding an object after them
- **THEN** the answer holds that list alone, and the range ends with `end` 1

#### Scenario: A Scan of a transaction ends with it
- **WHEN** a Scan of a transaction waits for credit, and the transaction aborts
- **THEN** the Scan is refused at once with a protocol error, as one the partition does not hold

#### Scenario: Scans of a transaction wait on its home together
- **WHEN** two Scans of a transaction overlap, each naming an IRI of a home that has not answered
- **THEN** neither is refused, and each range answers the ID its home gave

#### Scenario: A Scan arriving while its transaction's write waits on a home is refused
- **WHEN** a transaction's Write waits on a home's answer, and a Scan of the transaction arrives
- **THEN** the Scan is refused with a protocol error naming the write

#### Scenario: An unheld skolem IRI answers no entry wherever it is homed
- **WHEN** a transaction names in a Scan a skolem IRI no dictionary holds, homed on another partition
- **THEN** the range answers no entry with `end` 1, and the transaction goes on

#### Scenario: A home's refusal ends the transaction on both streams
- **WHEN** a transaction names in a Scan an IRI its home refuses, and the IRI is no skolem IRI
- **THEN** the Scan's stream and the Bind stream each carry the home's refusal

### Requirement: A partition answers a Recall from its session table
A partition SHALL answer a Recall with the session's stored reply when its table holds the request as the session's last,
and SHALL otherwise say why it holds none: the request in its commit step, a later request as the session's last, or neither.

#### Scenario: A Recall returns the stored reply
- **WHEN** a session's request committed, and the frontend recalls it
- **THEN** the partition answers with a Commit frame holding what the commit's reply held

#### Scenario: A Recall of what the table does not hold
- **WHEN** the frontend recalls a request the table holds no reply for, then a request below the session's last
- **THEN** the first is refused with code 18 (NotCommitted), and the second with code 13 (RequestOutOfOrder), `detail` the last

### Requirement: A read through compute is a read of its transaction
A read of a read-write transaction SHALL reach the partition as a Scan naming the transaction, and SHALL see that
transaction's own writes: every write the client sent before the read was answered before the frontend sent the read,
and the partition SHALL read the transaction's version with its uncommitted writes over it. Before an entry leaves the
partition, the span holding it SHALL be recorded in the transaction's reads: to the end of a range that ended, found
nothing included, and to just past the last entry of a range stopped at its limit. Commit SHALL validate those spans as
ranges scanned (transactions, "What a transaction's reads record and what conflicts with them"), against every list a
commit writes. A span the read set cannot hold SHALL end the transaction, and no entry of it SHALL be sent.

#### Scenario: A read sees its transaction's own write
- **WHEN** a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o`
  before committing
- **THEN** the read returns `<b>`, and a read by any other transaction does not

#### Scenario: A read sees its transaction's new term and its emptied list
- **WHEN** a transaction writes `<new> <p> "x"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both
- **THEN** the read returns `"x"` for `<new>` with `<new>` named by its text, and nothing for `<a>`

#### Scenario: A scan through compute holds no list in the transaction
- **WHEN** a transaction scans 10,000 lists through compute
- **THEN** its read set holds one range and no list key, and it holds none of those lists in memory afterwards

#### Scenario: A term nobody holds conflicts with its later statement through compute
- **WHEN** transaction T1 reads `<nobody> <p> ?o` through compute, finding nothing, transaction T2 commits
  `<nobody> <p> "x"`, and T1 commits a write
- **THEN** T1 aborts with a conflict of kind AbsentTerm

#### Scenario: A write waits for a read sent before it
- **WHEN** a client sends a read of its transaction, and a write on another stream before the read is answered
- **THEN** the frontend sends the write only after the read is answered, and the read returns none of it

#### Scenario: A read waits for a write sent before it
- **WHEN** a client sends a write and then a read of its transaction on another stream, without waiting for the write's
  reply
- **THEN** the frontend sends the read to compute only after the write is answered, and the read returns the write

#### Scenario: A scanned range conflicts with an insert into it through compute
- **WHEN** transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`,
  and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A reverse range conflicts with an edge whose reverse entry lands in it
- **WHEN** transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same
  partition, and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A range stopped at its limit conflicts only with what it returned
- **WHEN** T1 reads with limit 1 a range holding `<a>` and `<z>`, T2 commits an insert past `<a>` that T1's read did not
  reach, and T1 commits without resuming
- **THEN** T1 commits

#### Scenario: Resuming records the rest
- **WHEN** T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a
  write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A read past the read set's room ends its transaction
- **WHEN** a transaction's reads leave no room for a span
- **THEN** the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next
  request is refused

### Requirement: A reverse read asks the object's referrers
A reverse read of an entity `o` at epoch `e` SHALL first read `o`'s referrer set at `e` (partitions, "An entity's
referrer set names where its inbound edges may be") at `o`'s home, once the home has closed `e`; it SHALL then resolve
the set's bucket groups to partitions through the placement map in effect at `e` and ask each of those partitions,
batched per role address, for its reverse entries of `o` at `e`, each answering once it has closed `e`. A partition the set does
not name SHALL not be asked. A snapshot read SHALL return every matching row or wait, never a partial answer; a read
still waiting at its transaction's deadline SHALL fail with a timeout naming the partition it waited on. Correctness
SHALL depend on no cache of inbound entries at the home.

#### Scenario: A reverse read gathers every referrer
- **WHEN** `<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read
  binds its object to `<b>` and its predicate to `knows` at a closed epoch
- **THEN** compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`

#### Scenario: A partition the referrer set does not name is not asked
- **WHEN** `<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a
  reverse read of `<b>` runs
- **THEN** compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to
  partition 4

#### Scenario: A lagging referrer delays the read, never truncates it
- **WHEN** `R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the
  read's epoch, and the transaction's deadline of 50 ms passes
- **THEN** the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's

### Requirement: Access paths follow the bound positions
A read SHALL choose its keyspace by its bound positions: the forward keyspace for a bound subject and predicate, the
reverse keyspace of each asked partition for a bound object and predicate, the subject-predicates list for a bound
subject alone, and the object-predicates list of each asked partition for a bound object alone; a pattern binding no
position SHALL scan the forward keyspace of each predicate. An unbound graph SHALL read every graph. A row SHALL carry
its statement's subject, predicate, object and graph, which name a statement without a statement ID, and for a statement
with one, its reifier's skolem IRI (term-dictionary, "IRIs and blank nodes get system IDs"), never the statement ID.

#### Scenario: A bound object reads the reverse keyspace
- **WHEN** a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`
- **THEN** it reads the reverse lists of `http://ex.org/b` under `http://ex.org/knows` in the partitions `R(b)` names,
  and returns each subject

#### Scenario: An unbound graph reads every graph
- **WHEN** `<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound
- **THEN** it returns both statements, each with its graph

#### Scenario: A statement without a statement ID is named by its quad
- **WHEN** `<a> <knows> <b>` is held without a statement ID and a read returns it
- **THEN** the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone

#### Scenario: Parallel edges are told apart by their reifiers
- **WHEN** `<a> <knows> <b>` is held once without a statement ID and once as a parallel edge with one, and a read
  returns both
- **THEN** one row carries no reifier and the other carries the parallel edge's reifier IRI, and neither carries a
  statement ID

### Requirement: Resume keys continue their own read
A read reaching its limit SHALL return a resume key that continues that read alone, after its last row: an
XXH3-checksummed token holding the hash of the read's pattern and the last row's terms in their text form, never a
store key. A resume key used with another pattern, or failing its checksum, SHALL be refused. A read matching nothing
SHALL return no rows and no resume key.

#### Scenario: A read resumes where it stopped
- **WHEN** a read with limit 2 matches 5 rows and is resumed twice
- **THEN** the three answers return rows 1–2, 3–4 and 5, the last with no resume key

#### Scenario: A resume key with another pattern is refused
- **WHEN** a resume key from one read is used with a different pattern
- **THEN** the read is refused with an error naming the resume key

#### Scenario: A resume key holds terms, not keys
- **WHEN** a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server
- **THEN** the key holds the pattern's hash and the text `http://ex.org/b`, and no system ID or store key

#### Scenario: The empty result
- **WHEN** a read matches nothing
- **THEN** it returns no rows and no resume key

### Requirement: A read's transaction names its level
A transaction SHALL name on Begin the level, the session token, if any, and the deadline its reads share. A read-write
transaction SHALL read its partition's latest state whatever the level, and a token SHALL make its commit carry an epoch
at or above the token's (transactions, "Commits answer a session token"). A read-only transaction SHALL read at its
level: the latest state of one partition, binding to the partition of its first read, a later read reaching another
partition refused; a snapshot at a closed epoch (the default), the latest epoch the sequencer had closed when the
transaction began or its session token's epoch if that is higher, one snapshot for all its reads, each waiting until
every partition it asks has closed that epoch (sequencer, "Partitions learn each closed epoch and stamp the open one");
or monotonic-fast, which quad-pattern reads may take since they are monotonic, each read answering at one epoch: the
lowest closed epoch among the partitions it touches, but never below its floor, the larger of the session token's epoch
and the epoch of the transaction's previous read, waiting only for a partition that has not closed the floor; a reverse
read reads `R(o)` at the home's latest closed epoch `e`, waiting until `e` is at or above the floor, resolves its groups
through the placement map at `e`, and has every member answer at `e`, waiting until each has closed `e`. A transaction's
monotonic-fast reads therefore never go back and always see its token's commit, but need not share one snapshot. A
read-only transaction carrying a token SHALL wait, within its deadline, until the token's epoch has closed. A snapshot
read waiting on the open epoch SHALL make the frontend, which hosts the sequencer, ask it for an early close (sequencer,
"The sequencer closes epochs at its own wall-clock time").

#### Scenario: A read after a commit sees it through its token
- **WHEN** a client commits a write, receives a session token, and begins a transaction with that token whose read
  asks for the write
- **THEN** the read returns the write

#### Scenario: Two reads of one transaction share its snapshot
- **WHEN** a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new
  statement of `<a>`, and the first transaction reads `<a>`'s statements again
- **THEN** both reads return the same rows

#### Scenario: A monotonic-fast read does not wait for a lagging partition
- **WHEN** partition 2 has closed epoch 9 and partition 1 epoch 12, and a monotonic-fast read with its subject unbound
  runs over both
- **THEN** it answers at once, reading both partitions at epoch 9, the lower of their closed epochs

#### Scenario: Monotonic-fast reads never go back
- **WHEN** a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition
  whose latest closed epoch is 12
- **THEN** the second read answers at epoch 12 without waiting

#### Scenario: A monotonic-fast read never answers below the token
- **WHEN** a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches
  partition 1, closed at 21, and partition 2, closed at 18
- **THEN** the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit

#### Scenario: A latest-state read across partitions is refused
- **WHEN** a transaction at latest-state level reads with its subject unbound over two partitions
- **THEN** the read is refused with an error naming the level and the two partitions
