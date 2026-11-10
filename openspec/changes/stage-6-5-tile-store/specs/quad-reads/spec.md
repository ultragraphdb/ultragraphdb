## MODIFIED Requirements

### Requirement: Compute executes quad-pattern reads over storage
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, executed by compute, which
asks the partitions holding the pattern's entries for their rows through storage's two read kernels, Scan and Frontier
("Storage's read kernel answers what compute scans"), and returns the rows partition by partition, in ascending order
of the partitions the read asks as the placement map in effect at its epoch names them, and within a partition in its
families' order; no order SHALL hold by term or by key (`PROTOCOL.md` section 9.1; `STORAGE.md` sections 7.4 and 18.3;
T38, T46). A pattern whose bound subject is a reifier's skolem IRI SHALL read the statement's record (`STORAGE.md`
section 9.3, kind 18) at the partition its bucket names, one row per property matching the pattern, with `ABSENT` in the
reifier column. A pattern with any other bound subject SHALL ask that subject's partition alone; a pattern with an
unbound subject and a bound IRI or blank-node object SHALL be a reverse read ("A reverse read asks the object's
referrers"); a pattern with an unbound subject and a bound vector object SHALL ask every partition holding a bucket of
the tenant's run for the predicate's forward families, or every predicate's when it is unbound, compute keeping the
objects equal to the vector's encoding, since no reverse key names a vector; any other pattern, a bound literal object
included, SHALL ask every partition holding a bucket of the tenant's run, batched per role address, and answer them one
after another, since a literal has no home and no referrer set. A value-index lookup and a lookup by reverse count SHALL
be compute's own reads for the query core, called through compute's API and reaching the same partitions, a reverse
count the sum of the partitions' counts; no client request names them in this stage. A read of a read-write transaction
SHALL reach only its partition (partitions, "A read-write transaction stays within one partition in this stage").
Compute SHALL resolve a bound term in the partition whose rows it reads, an absent entry there meaning that partition
holds no statement of the term and answers no row; only where one ID must key several partitions, a reverse read's
object and a resume key's terms, SHALL it read the term's home. A term and the rows keyed by its ID SHALL take two
reads, a Scan of the term's dictionary record first. Compute SHALL read in the lane and at the version `PROTOCOL.md`
section 13.12 gives each read: a read-write transaction's in lane X at the Bind's version, a read-only transaction's in
lane S. A read SHALL read one version of a partition: its first Scan there chooses it, each later Scan of that
partition SHALL set `at` 1, `read_version` that Scan's Entries' `version` and `epoch` 0, and each lane S Frontier of the
read SHALL name that version, or the version the client named, so a lane S Frontier never waits for an epoch. Compute
SHALL send a read's reads of rows only once each partition it asks has closed the read's epoch, which a Scan of
dictionary records at that epoch shows, a read with no term to resolve there asking a range of space 1 of the dictionary
for it, so a read of rows never parks on another partition's close.

#### Scenario: A bound subject asks one partition
- **WHEN** a read binds its subject to `http://ex.org/a` over two partitions
- **THEN** compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

#### Scenario: A bound literal object fans out
- **WHEN** `<a>` in partition 1 and `<c>` in partition 2 each hold `name` `"Ann"`, and a read binds its object to
  `"Ann"` and its predicate to `name`
- **THEN** compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>`, partition 1's
  row, before `<c>`, partition 2's

#### Scenario: A value-index lookup asks every partition
- **WHEN** partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and compute's API
  looks up `ann`
- **THEN** compute asks the three partitions, and the API returns the three subjects, partition 1's first and
  partition 3's last

#### Scenario: An unbound subject answers partition by partition
- **WHEN** a read leaves its subject and object unbound over two partitions that each hold matching rows
- **THEN** compute returns every row of partition 1 and then every row of partition 2

#### Scenario: A vector object reads the forward families
- **WHEN** `<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a
  read binds its object to `v` and its predicate to `embedding`
- **THEN** compute asks every partition of the tenant's run for the forward families of `embedding`, and returns `<a>`
  and `<c>` alone

#### Scenario: A reifier's skolem IRI reads its statement's properties
- **WHEN** `<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`
- **THEN** compute reads the statement's record at the partition `r`'s bucket names, and returns one row for `<since>`
  and `2020` with `ABSENT` in the reifier column

#### Scenario: A term absent where its rows lie answers no row
- **WHEN** a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2,
  and partition 1's dictionary holds no record of `<p>`
- **THEN** the dictionary Scan of partition 1 answers no entry, compute sends no read of rows and nothing to partition
  2, and the read answers one batch flagged `LAST` with no row

#### Scenario: A term and its rows take two reads
- **WHEN** a read of `<a> <knows> ?o` runs against a partition holding both terms
- **THEN** compute's first Scan holds the terms' dictionary records, and its Frontier of `<a>`'s forward rows, keyed by
  `<a>`'s ID, comes after that Scan's answer

#### Scenario: A read reads one version of a partition
- **WHEN** a read-only transaction at latest level takes three Scans of its partition, and a commit lands between the
  first and the second
- **THEN** the second and third Scans set `at` 1 and `read_version` the first Entries' `version`, and the read returns
  none of the commit

#### Scenario: A snapshot read's Frontier names its Scan's version
- **WHEN** a read-only transaction at snapshot level reads `<a> <knows> ?o`
- **THEN** compute's Frontier of `<a>`'s row is of lane S, its `read_version` the `version` of the Entries answering the
  read's first Scan of the partition

#### Scenario: A read of rows waits for every partition's close
- **WHEN** a snapshot read asks partitions 1 and 2, partition 1 has closed the read's epoch and partition 2 has not
- **THEN** compute sends each a dictionary Scan at that epoch and no read of rows until both have answered, so no read
  of rows of partition 1 is refused as stalled while partition 2 waits

### Requirement: Storage's read kernel answers what compute scans
Storage SHALL answer reads by two kernels, as `PROTOCOL.md` sections 13.13 and 13.21 state, each reading only the
partitions the role address serves: a Scan, by ranges of space 1 a record by its logical key and by ranges of space 2
one predicate's family of a kind, or every predicate's, in one graph or in every graph; a Frontier, named rows of one
relation, kind and direction, of one predicate or every predicate and in one graph or every graph, in lane L, S or X;
which kernel a pattern takes is `PROTOCOL.md` section 13.12's table ("Access paths follow the bound positions"). What
each kernel reads and in which order, the version it reads at and its refusals of a version, code 3 (TooOld) and code
27 (AheadOfWatermark), of a record kind and of a row named twice, its deadline and stall bounds with code 24
(TimedOut), a failed store's code 17 (Unavailable), and what lane X records are storage-lanes' rules ("A Scan reads
records by key and families whole", "A Scan chooses its version once and waits within its deadline", "The logged fold
horizon is the only retention", "A stalled Scan or Frontier releases its version", "A Frontier answers named rows in
request order across groups and classes", "What lane X records is the protocol's rule"). No key, range or resume key
SHALL name a slot, and every refusal of a request's fields SHALL precede its first frame. A row or record larger than a
frame SHALL continue across frames, read once however many frames it takes. A turn of the partition SHALL build at
most one frame per request, round-robin, within a budget of bytes per turn chosen so that a commit waiting on the turn
waits less than one sync of its log, and SHALL read at most 256 rows holding nothing. The partition SHALL build a
stream's next frame once the one before has left, so a stalled stream holds at most one frame in its memory and the
row that frame stopped inside. A range of space 2 or a Frontier that stops SHALL give its resume key (storage-lanes,
"Resume keys are logical and name no slot"), and a request naming it SHALL answer strictly after it. Only a stream
ending in `LAST` SHALL be an answer. A request naming a
transaction SHALL end with that transaction. A foreign term SHALL read by the partition's copy of its records, or by
its home's answer when it holds none; an unheld skolem IRI SHALL answer no entry wherever it is homed, a home's outcome
7 saying so, and the transaction goes on; any other refusal from the home, a failed store's outcome 17 among them,
SHALL end the transaction. Compute SHALL decode what the entries hold and SHALL name every term by its text in what it
returns.

#### Scenario: A dictionary record and a referrer set are read by Scan
- **WHEN** compute scans the term to ID record of `<b>` and `R(b)` at `b`'s home, by ranges of space 1 of kinds 1 and 7
- **THEN** the answer holds `b`'s ID and its referrer set's members at that version

#### Scenario: A hub's row crosses frames
- **WHEN** a reverse row holds more entries than one frame carries, and compute reads it
- **THEN** the answer takes several frames, each ending between two entries, and compute reads every entry once, in the
  row's order

#### Scenario: A record kind outside the kernel's is refused
- **WHEN** a Scan names a range of space 1 of the marks
- **THEN** it is refused with a protocol error naming the range's kind, and no frame precedes the refusal

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
- **THEN** partition 1 takes the ID from partition 2, the range answers it, and a later commit into what the read
  recorded aborts the transaction

#### Scenario: A Scan that ends its transaction says so on the Bind stream
- **WHEN** a Scan's span has no room in the transaction's read set
- **THEN** the Scan's stream and the Bind stream each carry code 6 (TooLarge), and the transaction's next request is refused

#### Scenario: Closed answers each partition's latest closed epoch
- **WHEN** compute sends Closed naming the partition at its role address
- **THEN** the reply holds the partition's latest closed epoch

#### Scenario: A request of a partition the address does not serve is refused
- **WHEN** a Scan, a Frontier and a Recall name a partition the role address does not serve
- **THEN** each is refused with a protocol error naming the partition

#### Scenario: Every range is checked before any frame
- **WHEN** Scans hold a valid range and then one of space 1 and kind 6, one of space 2 and kind 4 with no tokenizer, one
  of space 1 and kind 2 whose key is not 16 bytes, and one of space 2 whose graph is no valid system ID
- **THEN** each is refused with a protocol error naming what is wrong, and no frame precedes the refusal

#### Scenario: A record larger than a frame continues
- **WHEN** a statement's record holds more bytes than the receiver's `max_frame` and compute scans it
- **THEN** the answer takes several frames, each but the last with `continues` 1, and the record's value is their bytes
  in order

#### Scenario: A stalled Scan leaves another running
- **WHEN** one Scan of a connection holds no credit and another takes its credit as it reads
- **THEN** the second reaches `LAST` while the first takes no more frames

#### Scenario: A large row's frames stay within a turn
- **WHEN** a Scan reads a row holding more bytes than a turn builds, the receiver's `max_frame` larger than that
- **THEN** every Entries frame holds no more than a turn builds, and the row arrives whole

#### Scenario: A commit goes on beside Scans taking every turn's budget
- **WHEN** many Scans of one partition read large rows with credit to spare, and a client commits a write
- **THEN** the commit is answered while the Scans go on, and each Scan reaches `LAST`

#### Scenario: A family of emptied rows takes many turns
- **WHEN** a range of space 2 reads a family holding more emptied rows than a turn reads, and a row holding an entry
  after them
- **THEN** the answer holds that row alone, and the range ends with `end` 1

#### Scenario: A request of a transaction ends with it
- **WHEN** a Scan of a transaction waits for credit, and the transaction aborts
- **THEN** the Scan is refused at once with Error 1, `detail` 1, as one the partition does not hold

#### Scenario: Scans of a transaction wait on its home together
- **WHEN** two Scans of a transaction overlap, each naming an IRI of a home that has not answered
- **THEN** neither is refused, and each range answers the ID its home gave

#### Scenario: A Scan arriving while its transaction's write waits on a home is refused
- **WHEN** a transaction's Write waits on a home's answer, and a Scan of the transaction arrives
- **THEN** the Scan is refused with a protocol error naming the write

#### Scenario: A Commit behind a cancelled read's wait takes its turn
- **WHEN** a Scan of a transaction waits on a home's answer, the frontend cancels the read and sends the Commit
- **THEN** the partition answers the Commit once the home's reply is in, and the Commit is not refused

#### Scenario: An unheld skolem IRI answers no entry wherever it is homed
- **WHEN** a transaction names in a Scan a skolem IRI no dictionary holds, homed on another partition
- **THEN** the range answers no entry with `end` 1, and the transaction goes on

#### Scenario: A home's refusal ends the transaction on both streams
- **WHEN** a transaction names in a Scan an IRI its home refuses, and the IRI is no skolem IRI
- **THEN** the Scan's stream and the Bind stream each carry the home's refusal

#### Scenario: A statement's record is read by Scan
- **WHEN** compute scans the record of a statement's ID, a range of space 1 and kind 18, at its subject's partition, at
  a version
- **THEN** the entry's value is the record's value at that version, the quad and its properties

#### Scenario: A Scan of a transaction sees its statement's own property
- **WHEN** a transaction sets `<since> 2020` through the reifier `r`, the write is answered, and a Scan naming the
  transaction reads the statement's record
- **THEN** the record holds `<since> 2020` over what the Bind's version holds, a property the transaction removed is
  absent from it, and the statement's conflict key is recorded as a statement tested

#### Scenario: A run of dictionary lookups answers in one frame
- **WHEN** a Scan holds 50 ranges of space 1 of the dictionary, 3 of whose keys are absent
- **THEN** one Entries frame answers them, `range` 0, `through` 49, `end` 1, holding 47 entries in the ranges' order

#### Scenario: A resumed range answers strictly after its resume key
- **WHEN** a range of space 2 stops at its `limit` after the row of `<m>`, and a new Scan at the same version names the
  range with the resume key its last Entries frame gave
- **THEN** the new Scan answers the rows after `<m>`'s and none before it

#### Scenario: A malformed resume key is refused
- **WHEN** a range of space 2 names a resume key that does not decode as `STORAGE.md` section 18.3 gives it, and a
  Frontier names one of another family
- **THEN** each is refused with Error 1, the message naming the range, and no frame precedes the refusal

#### Scenario: A Frontier answers its rows in the request's order
- **WHEN** a Frontier of kind 1 names the rows of `<c>`, `<a>` and `<b>`, in that order
- **THEN** its Neighbours frames answer `<c>`'s row, then `<a>`'s, then `<b>`'s, each row's entries in its family's
  order

#### Scenario: A failed store under a range is no absence
- **WHEN** the partition's store fails reading a tile of a Scan's range
- **THEN** the Scan is refused with code 17, the message naming the range, and no frame says the range ended

#### Scenario: A home's failed store ends the transaction
- **WHEN** a transaction names in a Scan a skolem IRI whose home's store fails under the registration, which answers
  outcome 17
- **THEN** the Scan's stream and the Bind stream each carry code 17, and the range answers no entry

### Requirement: A read through compute is a read of its transaction
A read of a read-write transaction SHALL reach the partition as Scans and Frontiers naming the transaction, in lane X,
and SHALL see that transaction's own writes: every write the client sent before the read was answered before the
frontend sent the read, and the partition SHALL read the transaction's version with its uncommitted writes over it.
Before a frame leaves the partition, what it answers SHALL be recorded in the transaction's reads as `PROTOCOL.md`
sections 13.13 and 13.21 state: a range of space 2 records its family's whole range of conflict keys when the partition
takes it, whether or not a frame follows, the graph's whole range for every predicate and the tenant's for every graph;
a Frontier's frame records the conflict-key range of each row it reaches, entries or none, and a frame ending inside a
row its family's range in that row's bucket group; a statement's record found records the statement's conflict key, and
one not found the statement's ID as an absent term; a claim records its conflict key, found or absent; an entry or record
found of storage class I is never recorded, and an absence always is. Commit SHALL validate what these record against
every conflict key a commit writes (transactions, "What a transaction's reads record and what conflicts with them";
`PROTOCOL.md` section 12.2). What the read set cannot hold SHALL end the transaction, and no entry of it SHALL be sent. A
read SHALL count as answered at its `LAST`, at an Error, or once the frontend has sent its Cancel, whether the client's
or a Commit's; a read the client cancels while it is held SHALL be dropped unsent. A Commit and an Abort SHALL wait for
no read: taking a Commit, the frontend SHALL cancel the transaction's reads still open, drop unsent those it still
holds, send the writes held behind them in order on the Bind stream and then send the Commit, so every write the client
sent before its Commit is in what commits; taking an Abort, it SHALL cancel the reads and drop the held writes. No frame
of a cancelled read SHALL reach the client, and what its request records after the Cancel only widens what the commit
validates. A partition SHALL answer each Write and Operation on the Bind stream with Held once it holds the request,
which is the request's answer for this order and which the frontend relays to no client. A read under way when its
transaction ended SHALL be refused at the partition, and compute's Error 1 of `detail` 1 may arrive first: the frontend
SHALL hold it until the Bind stream ends, then answer the read with the Error the stream gave, and send nothing further
when it has already answered the read.

#### Scenario: A read sees its transaction's own write
- **WHEN** a transaction writes `<a> <knows> <b>`, the write is answered, and the same transaction reads `<a> <knows> ?o`
  before committing
- **THEN** the read returns `<b>`, and a read by any other transaction does not

#### Scenario: A read sees its transaction's new term and its emptied row
- **WHEN** a transaction writes `<new> <p> "x"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both
- **THEN** the read returns `"x"` for `<new>` with `<new>` named by its text, and nothing for `<a>`

#### Scenario: A family read whole holds one range in the transaction
- **WHEN** a transaction reads `?s <knows> ?o` through compute over a family of 10,000 rows
- **THEN** its read set holds the family's one range and no row's key, and it holds none of those rows in memory
  afterwards

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

#### Scenario: A row read conflicts with an insert into it through compute
- **WHEN** transaction T1 reads `<a> <knows> ?o` through compute, finding nothing, transaction T2 commits `<a> <knows> <b>`,
  and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A reverse row conflicts with an edge whose reverse entry lands in it
- **WHEN** transaction T1 reads `?s <knows> <b>` through compute, transaction T2 commits `<c> <knows> <b>` in the same
  partition, and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A family read whole conflicts past its limit
- **WHEN** T1 reads `?s <knows> ?o` with limit 1 through compute, the family holding `<a>`'s and `<z>`'s rows, T2
  commits an insert into `<z>`'s row, which T1's read did not reach, and T1 commits a write without resuming
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A read past the read set's room ends its transaction
- **WHEN** a transaction's reads leave no room for what a frame would record
- **THEN** the read is refused with `TooLarge`, no entry of that frame reaches the client, and the transaction's next
  request is refused

#### Scenario: A Commit takes the writes held behind a read
- **WHEN** a read of a transaction is open, a Write the client sent after it is held behind it, and the client sends
  Commit
- **THEN** the frontend cancels the read, sends the Write on the Bind stream, then the Commit; the Write is in what
  commits, and no further frame of the read reaches the client

#### Scenario: An Abort drops what a read held back
- **WHEN** a read of a transaction is open, a Write is held behind it, and the client sends Abort
- **THEN** the frontend cancels the read, the partition never receives the Write, and nothing of the transaction is held

#### Scenario: A read the client cancels releases the write behind it
- **WHEN** a read of a transaction is open, a Write is held behind it, and the client cancels the read
- **THEN** the frontend sends the read's Cancel and then the Write, without waiting for the read's `LAST`

#### Scenario: A read cancelled while it is held is dropped unsent
- **WHEN** a Write of a transaction is unanswered, a read behind it is held, and the client cancels the read
- **THEN** no ComputeRead is sent for it, and the transaction goes on

#### Scenario: What a request records after a Cancel only widens the commit's check
- **WHEN** a Commit cancels a read whose Frontier records one more row before the Cancel reaches the partition, and
  another transaction wrote into that row
- **THEN** the commit aborts with a conflict of kind Range, and no frame of that row reached the client

#### Scenario: A statement's record read conflicts with a property written
- **WHEN** T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and
  T1 commits a write
- **THEN** T1 aborts with a conflict of kind Statement

#### Scenario: A read under way when its transaction ended takes the stream's Error
- **WHEN** a read of a transaction is under way, the transaction passes its limit, and compute's Error 1 of `detail` 1
  reaches the frontend before the Bind stream's code 4
- **THEN** the frontend holds that Error until the Bind stream ends, and the client's read is refused with code 4

#### Scenario: A read already answered takes nothing further
- **WHEN** a read of a transaction has reached its `LAST`, and the transaction then ends with an Error
- **THEN** the frontend sends nothing further on that read's stream

### Requirement: A reverse read asks the object's referrers
A reverse read of an entity `o` at epoch `e` SHALL first read `o`'s referrer set at `e` (partitions, "An entity's
referrer set names where its inbound edges may be") at `o`'s home, once the home has closed `e`, or, at a named version
of epoch `e`, its set at the home's latest applied version, waiting for nothing, since a set only grows (`PROTOCOL.md`
section 11.12); it SHALL then resolve the set's bucket groups to partitions through the placement map in effect at `e`
and ask each of those partitions, batched per role address, for its reverse rows of `o` at `e` by a Frontier of kind 3,
each answering once it has closed `e`. A partition the set does not name SHALL not be asked. A snapshot read SHALL
return every matching row or wait, never a partial answer; a read still waiting at its transaction's deadline SHALL fail
with a timeout naming the partition it waited on. Correctness SHALL depend on no cache of inbound entries at the home.

#### Scenario: A reverse read gathers every referrer
- **WHEN** `<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read
  binds its object to `<b>` and its predicate to `knows` at a closed epoch
- **THEN** compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and then `<c>`

#### Scenario: A partition the referrer set does not name is not asked
- **WHEN** `<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a
  reverse read of `<b>` runs
- **THEN** compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse rows, and sends nothing to
  partition 4

#### Scenario: A lagging referrer delays the read, never truncates it
- **WHEN** `R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the
  read's epoch, and the transaction's deadline of 50 ms passes
- **THEN** the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's

### Requirement: Access paths follow the bound positions
A read SHALL choose its kernel and its families by its bound positions, by the first row of `PROTOCOL.md` section
13.12's table that fits: a bound subject SHALL read the subject's forward rows by Frontiers of kinds 1 and 2, an
attribute's column by kind 6 and a version run by kind 5; a bound IRI or blank-node object with an unbound subject
SHALL read the object's referrer set by a Scan of space 1, kind 7, at its home and then the object's reverse rows by a
Frontier of kind 3 at the set's members, the row named by the object's term encoding ("A reverse read asks the object's
referrers"); any other bound literal object, a vector aside, SHALL read the reverse rows by a Frontier of kind 3 at
every partition; a pattern binding neither SHALL read the predicate's forward families whole by Scan ranges of space 2
of kinds 1 and 2. An unbound predicate SHALL name every predicate (`predicate` 16 bytes of `0xFF`) and an unbound graph
every graph (`graph` 16 bytes of `0xFF`) in that kernel, so an unbound graph with a bound subject is read by a Frontier
naming every graph, never by a Scan of a whole family; rows name their graphs, the graph's system ID first, then the
predicate's, and in a transaction such a read records the tenant's whole range of conflict keys (`PROTOCOL.md` sections
13.12, 13.21 and 13.22; T48). An object held once, with no statement ID, SHALL be one row with `ABSENT` in the reifier column; one
held under statement IDs SHALL be one row per ID, in ascending order, each with that statement's reifier, in a forward
and a reverse row alike, so a reverse read takes no further read for them. A row SHALL carry its statement's subject,
predicate, object and graph, which name a statement without a statement ID, and for a statement with one, its reifier's
skolem IRI (term-dictionary, "IRIs and blank nodes get system IDs"), never the statement ID.

#### Scenario: A bound object reads the reverse rows
- **WHEN** a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`
- **THEN** it reads the reverse rows of `http://ex.org/b` under `http://ex.org/knows` by Frontiers of kind 3 in the
  partitions `R(b)` names, and returns each subject

#### Scenario: A bound subject alone reads every predicate
- **WHEN** a read binds its subject to `<a>` and nothing else, `<a>` holding `knows` and `name`
- **THEN** compute reads `<a>`'s forward rows by Frontiers naming every predicate, and returns each statement of `<a>`
  with its predicate

#### Scenario: An unbound graph reads every graph
- **WHEN** `<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound
- **THEN** it returns both statements, each with its graph

#### Scenario: A statement without a statement ID is named by its quad
- **WHEN** `<a> <knows> <b>` is held without a statement ID and a read returns it
- **THEN** the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone

#### Scenario: Parallel edges are told apart by their reifiers
- **WHEN** `<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both
- **THEN** two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID

#### Scenario: A reverse read names each parallel edge's reifier
- **WHEN** `<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to
  `knows`
- **THEN** two rows return for `<a>`, each with its statement's reifier from the reverse row's entries, and compute
  reads no forward row

#### Scenario: An unbound graph with a bound subject is read by a Frontier naming every graph
- **WHEN** `<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound
- **THEN** compute reads `<a>`'s forward row of `p` by a Frontier whose `graph` is 16 bytes of `0xFF`, and returns both
  statements, each with its graph

#### Scenario: A graph appearing later conflicts
- **WHEN** T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a new graph,
  and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

### Requirement: Resume keys continue their own read
A read stopping at its limit where rows may remain SHALL return a resume key in its stream's last batch, and no other
batch SHALL hold one; compute and the partition SHALL read no further to tell, so a resumed read may answer an empty last batch
with no key. The key SHALL continue that read alone, after its last row: a token compute alone writes and decodes, laid
out as `PROTOCOL.md` section 11.8 states under an XXH3-64 checksum, holding the hash of the read's pattern, its `scope`,
the partition that answered the last row, the last row's terms in their text form and the value ordering its family,
never a store key, a slot or a system ID. `scope` SHALL bind the key to its transaction: for a read-write transaction,
XXH3-64 of its identity; for a read-only one, the version it reads at, 0 at levels 1 and 2, so a read-only transaction
at snapshot level naming the version the first Begin answered may resume after a lost connection. Compute SHALL refuse
with Error 1 (Protocol), the message naming the resume key, a key failing its checksum, that does not decode, or whose
pattern hash or `scope` is not the request's, checked in that order; a named version below a partition's latest logged
fold horizon SHALL answer code 3 (TooOld). Compute SHALL pass over the partitions before the key's `partition`, resolve
the last row's terms in that partition and build from them and the ordering value the store's resume key (`STORAGE.md`
section 18.3), which continues strictly after that row, its statement included, and read every later partition from its
start; in a statement's record it SHALL drop the properties up to and including the last row's, in the order of
predicate ID, then value encoding, so a property removed since is passed all the same. A read with no limit that fails
SHALL restart. A read matching nothing SHALL return no rows and no resume key.

#### Scenario: A read resumes where it stopped
- **WHEN** a read with limit 2 matches 5 rows and is resumed twice
- **THEN** the three answers return rows 1–2, 3–4 and 5, the last with no resume key

#### Scenario: A resumed read may find nothing
- **WHEN** a read with limit 1 and its graph unbound matches one row and is resumed
- **THEN** the first answer holds the row and a resume key, and the second answers one batch flagged `LAST` with no row and no
  key

#### Scenario: A resume key with another pattern is refused
- **WHEN** a resume key from one read is used with a different pattern
- **THEN** the read is refused with an error naming the resume key

#### Scenario: A resume key holds terms, not keys
- **WHEN** a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server
- **THEN** the key holds the pattern's hash, its `scope`, the partition that answered and the text `http://ex.org/b`, and
  no system ID, slot or store key

#### Scenario: The empty result
- **WHEN** a read matches nothing
- **THEN** it returns no rows and no resume key

#### Scenario: A resume key of another transaction is refused
- **WHEN** a resume key from one read-write transaction's read is used with the same pattern in another read-write
  transaction
- **THEN** the read is refused with Error 1, the message naming the resume key

#### Scenario: A reader at a named version resumes after a reconnect
- **WHEN** a read-only transaction at snapshot level reads with limit 2, its connection is lost, and a new Begin names
  the version the first Begin answered and resumes with the key
- **THEN** the read returns rows 3 onward at that version, neither repeating nor skipping a row

#### Scenario: A resumed version below the fold horizon is refused
- **WHEN** a reader resumes with a key at a named version below the partition's latest logged fold horizon
- **THEN** the read is refused with code 3

#### Scenario: A resumed read continues in its partition
- **WHEN** a read with its subject unbound and limit 2 matches three rows of partition 1 and two of partition 2, and is
  resumed
- **THEN** the second answer holds partition 1's third row and partition 2's first, and asks partition 1 for nothing
  at or before the key's last row

#### Scenario: A limit stopping inside one object's statements resumes there
- **WHEN** `<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed
- **THEN** the first answer returns the first two statements by ascending ID, the second the third, and the partition
  answers nothing of the row before the second statement

#### Scenario: A limit stopping after an object with no statement ID skips it whole
- **WHEN** `<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed
- **THEN** the store's resume key names `<b>` as its neighbour and 16 zero bytes as its statement, and the read returns
  `<c>`

#### Scenario: A limit stopping inside a statement's record resumes there
- **WHEN** a statement's record holds three properties, and a read of its reifier with limit 2 is resumed
- **THEN** the second answer returns the third property alone

#### Scenario: Only the last batch holds a key
- **WHEN** a read with limit 10,000 stops at its limit after several batches
- **THEN** section 6 of every batch but the last is empty, and the last holds the key

#### Scenario: An unlimited read that fails restarts
- **WHEN** a read with no limit is refused with code 24 after three batches, and its caller runs it again
- **THEN** no batch held a resume key, and the second run returns from the first row

### Requirement: A partition retains what a read may still read
A partition SHALL answer every version at or above its latest logged fold horizon `H`, which its primary stamps on each
round and every replica and replay folds at (`STORAGE.md` section 14.2; `PROTOCOL.md` section 11.9), and SHALL refuse a
read below it with code 3 (TooOld). The primary SHALL keep `H` at or below the read version of its oldest open
read-write transaction, the oldest version a lane S read holds, the start version of every index
build in progress, and the version bound of the latest epoch closed `retention_ms` (`STORAGE.md` section 3.1, default
30,000 ms) before the close preceding the round's epoch, by the sequencer's logged close times, so `H` is a function of
the log. A Scan's or Frontier's version SHALL hold `H` on the primary alone: on a follower it holds nothing, and a
request there whose version a logged horizon passes SHALL be refused with code 3 for the rest of the request, its frames
already sent no answer. A read-only transaction's deadline bounds only its reads' waits. An open Scan SHALL keep its
version for as long as it stays open; between Scans a version SHALL stay only until its epoch has been closed for
`retention_ms` by the sequencer's logged times, so compute SHALL keep a Scan of each partition open across a read that
may outlast that, and a next Scan past it SHALL be refused with code 3. Compute SHALL open that Scan at the read's first
frame from the partition when the read's version was chosen before it began, one the client or the transaction named,
since such a read may begin late in the retention, and `retention_ms / 4` after a read at latest level or
monotonic-fast began, since a short read then opens no Scan for it.

#### Scenario: A reader coming back within the retention keeps its version
- **WHEN** a read-only transaction at snapshot level reads at epoch 20, epochs close for 15 s with `retention_ms`
  30,000, and it reads again
- **THEN** the second read answers at epoch 20's version bound

#### Scenario: A late read keeps its version between Scans
- **WHEN** a read at snapshot level has its dictionary Scan answered, and its read of rows arrives past `retention_ms`
  while later epochs close
- **THEN** the read reaches `LAST` with its row, and no request of it is refused with code 3

#### Scenario: A read at latest level holds no version at first
- **WHEN** a read at latest level and a read at snapshot level each answer within a quarter of `retention_ms`
- **THEN** the read at snapshot level opened one Scan holding its version, and the read at latest level none

#### Scenario: A reader coming back past the retention is refused
- **WHEN** a read-only transaction at snapshot level reads at epoch 20, rounds then log a fold horizon past that version
  bound once a later epoch has been closed for `retention_ms`, and it reads again
- **THEN** the read is refused with code 3

#### Scenario: An open Scan holds its version past the retention
- **WHEN** a Scan keeps taking frames on the primary for longer than `retention_ms` while later epochs close
- **THEN** every round's logged fold horizon stays at or below the Scan's version, and the Scan reaches `LAST` at that
  version

#### Scenario: A next Scan past the retention is refused after a long Scan
- **WHEN** a Scan at epoch 20's version bound stays open for 45 s with `retention_ms` 30,000 while later epochs close, it
  ends, a round then logs a fold horizon past that bound, and a next Scan at that version arrives
- **THEN** the next Scan is refused with code 3

#### Scenario: A read-write transaction holds its version
- **WHEN** a read-write transaction bound at a version is open while epochs close and rounds are logged
- **THEN** every round's logged fold horizon stays at or below the Bind's version until the transaction ends
