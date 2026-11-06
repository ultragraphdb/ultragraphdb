## ADDED Requirements

### Requirement: Compute executes quad-pattern reads over storage
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, executed by compute, which
asks the partitions holding the pattern's entries for their rows through storage's read kernels and returns the rows in
the store's key order. A pattern whose bound subject is a reifier's skolem IRI SHALL read the statement's record in the
statements keyspace of the partition its bucket names, one row per property matching the pattern, with `ABSENT` in the
reifier column. A pattern with any other bound subject SHALL ask that subject's partition alone; a pattern with an
unbound subject and a bound IRI or blank-node object SHALL be a reverse read ("A reverse read asks the object's
referrers"); a pattern with an unbound subject and a bound vector object SHALL ask every partition holding a bucket of
the tenant's run for the forward lists of the predicate, or of every predicate when it is unbound, compute keeping the
objects equal to the vector's encoding, since no reverse key names a vector; any other pattern, a bound literal object
included, SHALL ask every partition holding a bucket of the tenant's run and merge their rows in key order, batched per
role address, through a merge scan when the frontier is large, since a literal has no home and no referrer set. A
value-index lookup and a lookup by reverse count SHALL be compute's own reads for the query core, called through
compute's API and reaching the same partitions; no client request names them in this stage. A read of a read-write
transaction SHALL reach only its partition (partitions, "A read-write transaction stays within one partition in this
stage"). Compute SHALL resolve a bound term in the partition whose lists it reads, an absent entry there meaning that
partition holds no statement of the term and answers no row; only where one ID must key several partitions, a reverse
read's object and a resume key's terms, SHALL it read the term's home. A term and a list keyed by its ID SHALL take two
Scans, the term's first. A read SHALL read one version of a partition: its first Scan there chooses it, and each later
Scan of that partition SHALL set `at` 1, `read_version` that Scan's Entries' `version` and `epoch` 0. Compute SHALL send
a read's list Scans only once each partition it asks has closed the read's epoch, which a dictionary Scan at that epoch
shows, a read with no term to resolve there asking any one-key dictionary range for it, so a list Scan never parks on
another partition's close.

#### Scenario: A bound subject asks one partition
- **WHEN** a read binds its subject to `http://ex.org/a` over two partitions
- **THEN** compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

#### Scenario: A bound literal object fans out
- **WHEN** `<a>` in partition 1 and `<c>` in partition 2 each hold `name` `"Ann"`, and a read binds its object to
  `"Ann"` and its predicate to `name`
- **THEN** compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>` and `<c>`
  merged in key order

#### Scenario: A value-index lookup asks every partition
- **WHEN** partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and compute's API
  looks up `ann`
- **THEN** compute asks the three partitions, and the API returns the three subjects merged in key order

#### Scenario: An unbound subject merges every partition
- **WHEN** a read leaves its subject and object unbound over two partitions that each hold matching rows
- **THEN** compute returns the rows of both partitions merged in key order

#### Scenario: A vector object reads the forward lists
- **WHEN** `<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a
  read binds its object to `v` and its predicate to `embedding`
- **THEN** compute asks every partition of the tenant's run for the forward lists of `embedding`, and returns `<a>` and
  `<c>` alone

#### Scenario: A reifier's skolem IRI reads its statement's properties
- **WHEN** `<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`
- **THEN** compute reads the statement's record in the statements keyspace of the partition `r`'s bucket names, and
  returns one row for `<since>` and `2020` with `ABSENT` in the reifier column

#### Scenario: A term absent where its list lies answers no row
- **WHEN** a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2,
  and partition 1's dictionary holds no entry of `<p>`
- **THEN** the dictionary Scan of partition 1 answers no entry, compute sends no list Scan and nothing to partition 2,
  and the read answers one batch flagged `LAST` with no row

#### Scenario: A term and its list take two Scans
- **WHEN** a read of `<a> <knows> ?o` runs against a partition holding both terms
- **THEN** compute's first Scan holds the terms' dictionary ranges, and its Scan of the list, keyed by their IDs, comes
  after that Scan's answer

#### Scenario: A read reads one version of a partition
- **WHEN** a read-only transaction at latest level takes three Scans of its partition, and a commit lands between the
  first and the second
- **THEN** the second and third Scans set `at` 1 and `read_version` the first Entries' `version`, and the read returns
  none of the commit

#### Scenario: A list Scan waits for every partition's close
- **WHEN** a snapshot read asks partitions 1 and 2, partition 1 has closed the read's epoch and partition 2 has not
- **THEN** compute sends each a dictionary Scan at that epoch and no list Scan until both have answered, so no list Scan
  of partition 1 is refused as stalled while partition 2 waits

### Requirement: Compute runs each read on one core
Compute SHALL run one runtime per core, each holding its own connections to the frontend and the partitions, and SHALL run
a read wholly on the core that received it, sharing no mutable state between cores.

#### Scenario: A fan-out stays on its read's core
- **WHEN** a read fans out over three partitions
- **THEN** its Scans, its merge and its Result frames all run on the core its ComputeRead arrived on

### Requirement: Storage's read kernel answers what compute scans
Storage SHALL answer a Scan with the entries of its key ranges at the version the Scan names, each key once with its
value visible at that version, in key order, each range at most the Scan's limit: from a keyspace of lists, the statements,
the dictionary or the referrer sets, and no other. All of a Scan's ranges SHALL name one tenant: the partition SHALL compare
each first key's bytes 1 to 8 with the first range's and refuse a difference with Error 1 (Protocol), the message naming the
range, before any frame. A range with an `after` SHALL read its first key's list from that object on in the list's order, the
object itself included when the list holds it, and the keys beyond whole, reading none of the list's prefix; an `after` that
is not one whole object encoding, or on a range outside a keyspace of lists, SHALL be refused with Error 1, the message naming
the range. One Entries frame SHALL answer a run of one-key ranges of the dictionary, from `range` through `through`, each with
`end` 1, a range finding nothing holding no entry. Every frame of a Scan SHALL name the version the Scan chose. A store failing
under a range SHALL refuse the Scan with code 17 (Unavailable), the message naming the key, so a failed read never reads as
absence. An answer at a closed epoch SHALL wait until the partition has closed that epoch. A
Scan whose epoch has not closed by its deadline, or with no frame able to leave for `scan_stall_ms`, SHALL be refused naming
the partition. The partition SHALL build a stream's next frame once the one before has left, so a stalled stream holds at
most one frame in its memory and the list that frame stopped inside, which it SHALL read once however many frames it takes.
A turn of the partition SHALL build at most one frame per Scan, round-robin, within a budget of bytes per turn chosen so that a
commit waiting on the turn waits less than one sync of its log, and a frame holding a record or value SHALL stop at what its turn may
yet build. No turn of the partition SHALL read more than a bounded number of keys holding nothing. A Scan SHALL read one version, chosen
once. Only a stream ending in `LAST` SHALL be an answer. A Scan naming a transaction SHALL end with that transaction.
A foreign term SHALL read by the partition's copy of its entries, or by its home's answer when it holds none; an unheld skolem
IRI SHALL answer no entry wherever it is homed, a home's outcome 7 saying so, and the transaction goes on; any other refusal
from the home, a failed store's outcome 17 among them, SHALL end the transaction. Compute SHALL
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
  dictionary with no past key, of the referrer sets with a first key too short, or of the referrer sets past their tenant
- **THEN** each is refused with a protocol error naming what is wrong, and no frame precedes the refusal

#### Scenario: A record too large for a frame is refused
- **WHEN** the dictionary or the statements hold a value too large for the receiver's `max_frame`
- **THEN** a Scan of it is refused with code 6 (TooLarge), `detail` that `max_frame`

#### Scenario: A stalled Scan leaves another running
- **WHEN** one Scan of a connection holds no credit and another takes its credit as it reads
- **THEN** the second reaches `LAST` while the first takes no more frames

#### Scenario: A large list's frames stay within a turn
- **WHEN** a Scan reads a list holding more bytes than a turn builds, the receiver's `max_frame` larger than that
- **THEN** every Entries frame holds no more than a turn builds, and the list arrives whole

#### Scenario: A commit goes on beside Scans taking every turn's budget
- **WHEN** many Scans of one partition read large lists with credit to spare, and a client commits a write
- **THEN** the commit is answered while the Scans go on, and each Scan reaches `LAST`

#### Scenario: A range of emptied lists takes many turns
- **WHEN** a range holds more emptied lists than a turn reads, and a list holding an object after them
- **THEN** the answer holds that list alone, and the range ends with `end` 1

#### Scenario: A Scan of a transaction ends with it
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
- **WHEN** compute scans the key of a statement's record at its subject's partition, at a version
- **THEN** the entry's value is the record's stored bytes at that version

#### Scenario: A Scan of a transaction sees its statement's own property
- **WHEN** a transaction sets `<since> 2020` through the reifier `r`, the write is answered, and a Scan naming the
  transaction reads the statement's record
- **THEN** the record holds `<since> 2020` over what the Bind's version holds, a property the transaction removed is
  absent from it, and its span is recorded as a range scanned

#### Scenario: A Scan's ranges name one tenant
- **WHEN** a Scan's second range holds a first key of another tenant than its first range's
- **THEN** it is refused with Error 1, the message naming the range, and no frame precedes the refusal

#### Scenario: A run of dictionary lookups answers in one frame
- **WHEN** a Scan holds 50 one-key ranges of the dictionary, 3 of whose keys are absent
- **THEN** one Entries frame answers them, `range` 0, `through` 49, `end` 1, holding 47 entries in the ranges' order

#### Scenario: A resumed range reads its list from `after`
- **WHEN** a list holds `<a>`, `<m>` and `<z>`, and one range names `after` `<m>` and another `after` `<n>`, which the
  list does not hold
- **THEN** the first answers `<m>` and `<z>`, the second `<z>`, and neither reads the record of `<a>`

#### Scenario: A malformed `after` is refused
- **WHEN** a range's `after` is not one whole object encoding, and another range of the dictionary holds an `after`
- **THEN** each is refused with Error 1, the message naming the range, and no frame precedes the refusal

#### Scenario: A failed store under a range is no absence
- **WHEN** the partition's store fails reading a key of a Scan's range
- **THEN** the Scan is refused with code 17, the message naming the key, and no frame says the range ended

#### Scenario: A home's failed store ends the transaction
- **WHEN** a transaction names in a Scan a skolem IRI whose home's store fails under the registration, which answers
  outcome 17
- **THEN** the Scan's stream and the Bind stream each carry code 17, and the range answers no entry

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
commit writes. A span the read set cannot hold SHALL end the transaction, and no entry of it SHALL be sent. A span of the statements
is a range scanned as a list's is, and every statement record a commit writes SHALL count against it. A read SHALL count as
answered at its `LAST`, at an Error, or once the frontend has sent its Cancel, whether the client's or a Commit's; a read the
client cancels while it is held SHALL be dropped unsent. A Commit and an Abort SHALL wait for no read: taking a Commit, the
frontend SHALL cancel the transaction's reads still open, drop unsent those it still holds, send the writes held behind them in
order on the Bind stream and then send the Commit, so every write the client sent before its Commit is in what commits; taking
an Abort, it SHALL cancel the reads and drop the held writes. No frame of a cancelled read SHALL reach the client, and a span
its Scan records after the Cancel only widens what the commit validates. A partition SHALL answer each Write and Operation on
the Bind stream with Held once it holds the request, which is the request's answer for this order and which the frontend relays
to no client. A read under way when its transaction ended SHALL be refused at the partition, and compute's Error 1 of `detail`
1 may arrive first: the frontend SHALL hold it until the Bind stream ends, then answer the read with the Error the stream gave,
and send nothing further when it has already answered the read.

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

#### Scenario: A limit stopping before a graph leaves that graph unread
- **WHEN** T1 reads `<a> <name> ?o` with limit 1 and its graph unbound, `<a>` holding it in two graphs, T2 commits an insert
  into the graph T1's read did not reach, and T1 commits a write
- **THEN** T1 commits

#### Scenario: Resuming records the rest
- **WHEN** T1 reads with limit 1, resumes to the range's end, T2 commits an insert into the resumed part, and T1 commits a
  write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A read past the read set's room ends its transaction
- **WHEN** a transaction's reads leave no room for a span
- **THEN** the read is refused with `TooLarge`, no entry of that span reaches the client, and the transaction's next
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

#### Scenario: A span recorded after a Cancel only widens the commit's check
- **WHEN** a Commit cancels a read whose Scan records one more span before the Cancel reaches the partition, and another
  transaction wrote into that span
- **THEN** the commit aborts with a conflict of kind Range, and no frame of that span reached the client

#### Scenario: A statement's record read conflicts with a property written
- **WHEN** T1 reads the properties of the reifier `r` through compute, T2 sets a property through `r` and commits, and
  T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

#### Scenario: A read under way when its transaction ended takes the stream's Error
- **WHEN** a read of a transaction is under way, the transaction passes its limit, and compute's Error 1 of `detail` 1
  reaches the frontend before the Bind stream's code 4
- **THEN** the frontend holds that Error until the Bind stream ends, and the client's read is refused with code 4

#### Scenario: A read already answered takes nothing further
- **WHEN** a read of a transaction has reached its `LAST`, and the transaction then ends with an Error
- **THEN** the frontend sends nothing further on that read's stream

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
position SHALL scan the forward keyspace of each predicate. An unbound graph SHALL read every graph, walked per partition
and keyspace: a Scan from the keyspace's tenant, or from just past the last graph found, with limit 1, names the next graph by
its first key, compute cancels that Scan after its first frame and then reads the pattern's keys in that graph, and in a
transaction these Scans record their spans as any other. An object held once, with no statement ID, SHALL be one row with
`ABSENT` in the reifier column; one held under statement IDs SHALL be one row per ID, in ascending order, each with that
statement's reifier, in a forward and a reverse list alike, so a reverse read takes no further Scan for them. A row SHALL carry
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
- **WHEN** `<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both
- **THEN** two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID

#### Scenario: A reverse read names each parallel edge's reifier
- **WHEN** `<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to
  `knows`
- **THEN** two rows return for `<a>`, each with its statement's reifier from the reverse list's object record, and
  compute sends no Scan of a forward list

#### Scenario: An unbound graph is walked by Scans of limit 1
- **WHEN** `<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound
- **THEN** compute finds each graph by a Scan of limit 1 from the keyspace's tenant and then from just past the graph
  found, cancels each after its first frame, and returns both statements

#### Scenario: A graph appearing in a gap conflicts
- **WHEN** T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a graph the
  walk passed over, and T1 commits a write
- **THEN** T1 aborts with a conflict of kind Range

### Requirement: Resume keys continue their own read
A read stopping at its limit where rows may remain SHALL return a resume key in its stream's last batch, and no other
batch SHALL hold one; compute and the partition SHALL read no further to tell, so a resumed read may answer an empty last batch
with no key. The key SHALL continue that read alone, after its last row: an XXH3-checksummed token compute
alone writes and decodes, holding the hash of the read's pattern, its `scope` and the last row's terms in their text
form, never a store key. `scope` SHALL bind the key to its transaction: for a read-write transaction, XXH3-64 of its
identity; for a read-only one, the version it reads at, 0 at levels 1 and 2, so a read-only transaction at snapshot
level naming the version the first Begin answered may resume after a lost connection. Compute SHALL refuse with Error 1
(Protocol), the message naming the resume key, a key failing its checksum, that does not decode, or whose pattern hash
or `scope` is not the request's, checked in that order; a named version no longer retained SHALL answer code 3 (TooOld).
Compute SHALL resume inside a list by a range's `after` at the last row's object, dropping from that object's record the
statements at or below the last row's statement ID, and the whole object when the last row has no reifier; in a
statement's record it SHALL drop the properties up to and including the last row's. A read with no limit that fails
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
- **THEN** the key holds the pattern's hash, its `scope` and the text `http://ex.org/b`, and no system ID or store key

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

#### Scenario: A resumed version no longer retained is refused
- **WHEN** a reader resumes with a key at a named version the partition has discarded
- **THEN** the read is refused with code 3

#### Scenario: A limit stopping inside one object's statements resumes there
- **WHEN** `<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed
- **THEN** the first answer returns the first two statements by ascending ID, the second the third, and the partition
  reads none of the list's prefix

#### Scenario: A limit stopping after an object with no statement ID skips it whole
- **WHEN** `<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed
- **THEN** the resumed Scan's `after` names `<b>`, compute drops `<b>` from what it answers, and the read returns `<c>`

#### Scenario: A limit stopping inside a statement's record resumes there
- **WHEN** a statement's record holds three properties, and a read of its reifier with limit 2 is resumed
- **THEN** the second answer returns the third property alone

#### Scenario: Only the last batch holds a key
- **WHEN** a read with limit 10,000 stops at its limit after several batches
- **THEN** section 6 of every batch but the last is empty, and the last holds the key

#### Scenario: An unlimited read that fails restarts
- **WHEN** a read with no limit is refused with code 24 after three batches, and its caller runs it again
- **THEN** no batch held a resume key, and the second run returns from the first row

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
and the highest epoch any read of the transaction has answered at when this read is sent, waiting only for a partition that
has not closed the floor; a reverse
read reads `R(o)` at the home's latest closed epoch `e`, waiting until `e` is at or above the floor, resolves its groups
through the placement map at `e`, and has every member answer at `e`, waiting until each has closed `e`. A transaction's
monotonic-fast reads therefore never go back and always see its token's commit, but need not share one snapshot. A
read-only transaction carrying a token SHALL wait, within its deadline, until the token's epoch has closed. A snapshot
read waiting on the open epoch SHALL make the frontend, which hosts the sequencer, ask it for an early close (sequencer,
"The sequencer closes epochs at its own wall-clock time"). The frontend SHALL hold a latest-level transaction's later read until
its first read's first Result names the partition, or that read ends; a first read refused leaves the next read first.

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

#### Scenario: A later latest-state read waits for the first read's partition
- **WHEN** a transaction at latest level sends two reads beside each other, the first binding it to partition 1
- **THEN** the frontend sends the second only after the first's first Result names partition 1, with `partition` 1, and
  compute refuses it with code 25 when it reaches another partition

#### Scenario: A first read refused leaves the next read first
- **WHEN** a transaction at latest level sends two reads, and the first is refused before any Result
- **THEN** the second goes out as the transaction's first read and binds it

#### Scenario: Concurrent monotonic-fast reads take the highest epoch answered
- **WHEN** a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after
  both
- **THEN** the third read's floor is 12

### Requirement: A read's batches fit the client's frame
The frontend SHALL carry in each ComputeRead the `max_frame` of the client's Hello, 524,288 to 1,048,576, and `credit`,
the `initial_credit` of its Welcome to that client, at least 1, and SHALL relay each Credit and Cancel a client sends on
a read's stream to that read's ComputeRead stream. Compute SHALL build every Result frame within that `max_frame` and
start the stream with that credit in place of its own Welcome's. A row whose terms do not fit an otherwise empty batch
SHALL be refused with code 6 (TooLarge), `detail` `max_frame`. A resume key that does not fit beside its rows SHALL go
alone in a last batch of `rows` 0, which spends credit as any batch does; a key too large for that batch, or for a Read
within the Welcome's `max_frame`, SHALL refuse the read with code 6. The frontend SHALL refuse with code 6, `detail`
that `max_frame`, an Execute whose ComputeRead would not fit a frame within compute's Welcome, and no compute process
sees it.

#### Scenario: A batch stops at the client's frame
- **WHEN** a client whose Hello names `max_frame` 524,288 reads more rows than one such frame holds
- **THEN** every Result frame it receives is at most 524,288 bytes, and every row arrives

#### Scenario: A row too large for the client's frame is refused
- **WHEN** a client whose Hello names `max_frame` 524,288 reads a row holding a literal of 600,000 bytes
- **THEN** the read is refused with code 6, `detail` 524,288

#### Scenario: A resume key that does not fit goes alone
- **WHEN** a read stops at its limit, and its rows fill the last batch so that its resume key does not fit
- **THEN** the stream's last batch holds `rows` 0 and the key, spending one credit, and the batch before it is not
  flagged `LAST`

#### Scenario: A read's stream starts with the frontend's credit
- **WHEN** a client's Welcome names `initial_credit` 4, compute's Welcome to the frontend names 16, and a read's answer
  takes more frames
- **THEN** compute sends 4 frames, and one more for each frame of the client's Credit the frontend relays

#### Scenario: An Execute too large for compute's frame is refused
- **WHEN** an Execute's parameters, bound into the pattern, make a ComputeRead larger than a frame within compute's
  Welcome
- **THEN** the frontend refuses it with code 6, `detail` that `max_frame`, and no compute process receives it

### Requirement: A partition retains what a read may still read
A partition SHALL retain every version at or above its discard version, which it SHALL raise only to the version bound
of an epoch closed more than `read_retain_ms` ago, default 60,000 ms, and never past its visibility watermark or a version
an open Scan or a read-write transaction holds. A read below the discard version SHALL be refused with code 3 (TooOld). A read-only
transaction's deadline bounds only its reads' waits. An open Scan SHALL keep its version for as long as it stays open; between
Scans a version SHALL stay only until a later epoch has been closed for more than `read_retain_ms`, so compute SHALL keep a Scan
of each partition open across a read that may outlast that, and a next Scan past it SHALL be refused with code 3. Compute SHALL open
that Scan at the read's first frame from the partition when the read's version was chosen before it began, one the client or the
transaction named, since such a read may begin late in the retention, and `read_retain_ms / 4` after a read at latest level or
monotonic-fast began, since a short read then opens no Scan for it.

#### Scenario: A reader coming back within the retention keeps its version
- **WHEN** a read-only transaction at snapshot level reads at epoch 20, epochs close for 30 s with `read_retain_ms`
  60,000, and it reads again
- **THEN** the second read answers at epoch 20's version bound

#### Scenario: A late read keeps its version between Scans
- **WHEN** a read at snapshot level has its dictionary Scan answered, and its list Scan arrives past `read_retain_ms` while
  later epochs close
- **THEN** the read reaches `LAST` with its row, and no Scan of it is refused with code 3

#### Scenario: A read at latest level holds no version at first
- **WHEN** a read at latest level and a read at snapshot level each answer within a quarter of `read_retain_ms`
- **THEN** the read at snapshot level opened one Scan holding its version, and the read at latest level none

#### Scenario: A reader coming back past the retention is refused
- **WHEN** a read-only transaction at snapshot level reads at epoch 20, the partition raises its discard version past
  that version bound once a later epoch has been closed for more than `read_retain_ms`, and it reads again
- **THEN** the read is refused with code 3

#### Scenario: An open Scan holds its version past the retention
- **WHEN** a Scan keeps taking frames for longer than `read_retain_ms` while later epochs close
- **THEN** the discard version stays at or below the Scan's version, and the Scan reaches `LAST` at that version

#### Scenario: A next Scan past the retention is refused after a long Scan
- **WHEN** a Scan at epoch 20's version bound stays open for 90 s with `read_retain_ms` 60,000 while later epochs close, it
  ends, the partition raises its discard version past that bound, and a next Scan at that version arrives
- **THEN** the next Scan is refused with code 3

#### Scenario: A read-write transaction holds its version
- **WHEN** a read-write transaction bound at a version is open while epochs close for longer than `read_retain_ms`
- **THEN** the discard version stays at or below the Bind's version until the transaction ends
