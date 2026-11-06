# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through a real compute process over real storage processes; a test plays the frontend.

  Rule: Compute executes quad-pattern reads over storage

    Scenario: A bound subject asks one partition
      When a read binds its subject to `http://ex.org/a` over two partitions
      Then compute asks only the partition of `http://ex.org/a`'s bucket, and returns its rows

    Scenario: A bound literal object fans out
      When `<a>` in partition 1 and `<c>` in partition 2 each hold `name` `"Ann"`, and a read binds its object to `"Ann"` and its predicate to `name`
      Then compute asks every partition of the tenant's run, reads no referrer set, and returns `<a>` and `<c>` merged in key order

    Scenario: A value-index lookup asks every partition
      When partitions 1, 2 and 3 each hold a subject whose `name` the `term` index lists under `ann`, and compute's API looks up `ann`
      Then compute asks the three partitions, and the API returns the three subjects merged in key order

    Scenario: An unbound subject merges every partition
      When a read leaves its subject and object unbound over two partitions that each hold matching rows
      Then compute returns the rows of both partitions merged in key order

    Scenario: A vector object reads the forward lists
      When `<a>` in partition 1 and `<c>` in partition 2 each hold `embedding` `v`, `<d>` holds another vector, and a read binds its object to `v` and its predicate to `embedding`
      Then compute asks every partition of the tenant's run for the forward lists of `embedding`, and returns `<a>` and `<c>` alone

    Scenario: A reifier's skolem IRI reads its statement's properties
      When `<a> <knows> <b>` carries `<since> 2020` under the reifier `r`, and a read binds its subject to `r`
      Then compute reads the statement's record in the statements keyspace of the partition `r`'s bucket names, and returns one row for `<since>` and `2020` with `ABSENT` in the reifier column

    Scenario: A term absent where its list lies answers no row
      When a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2, and partition 1's dictionary holds no entry of `<p>`
      Then the dictionary Scan of partition 1 answers no entry, compute sends no list Scan and nothing to partition 2, and the read answers one batch flagged `LAST` with no row

    Scenario: A term and its list take two Scans
      When a read of `<a> <knows> ?o` runs against a partition holding both terms
      Then compute's first Scan holds the terms' dictionary ranges, and its Scan of the list, keyed by their IDs, comes after that Scan's answer

    Scenario: A read reads one version of a partition
      When a read-only transaction at latest level takes three Scans of its partition, and a commit lands between the first and the second
      Then the second and third Scans set `at` 1 and `read_version` the first Entries' `version`, and the read returns none of the commit

    Scenario: A list Scan waits for every partition's close
      When a snapshot read asks partitions 1 and 2, partition 1 has closed the read's epoch and partition 2 has not
      Then compute sends each a dictionary Scan at that epoch and no list Scan until both have answered, so no list Scan of partition 1 is refused as stalled while partition 2 waits

  Rule: Compute runs each read on one core

    Scenario: A fan-out stays on its read's core
      When a read fans out over three partitions
      Then its Scans, its merge and its Result frames all run on the core its ComputeRead arrived on

  Rule: A reverse read asks the object's referrers

    Scenario: A reverse read gathers every referrer
      When `<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch
      Then compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`

    Scenario: A partition the referrer set does not name is not asked
      When `<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs
      Then compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4

    Scenario: A lagging referrer delays the read, never truncates it
      When `R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes
      Then the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's

  Rule: Access paths follow the bound positions

    Scenario: A bound object reads the reverse keyspace
      When a read binds its object to `http://ex.org/b` and its predicate to `http://ex.org/knows`
      Then it reads the reverse lists of `http://ex.org/b` under `http://ex.org/knows` in the partitions `R(b)` names, and returns each subject

    Scenario: An unbound graph reads every graph
      When `<a> <p> <b>` is held in the default graph and in `<g1>`, and a read leaves the graph unbound
      Then it returns both statements, each with its graph

    Scenario: A statement without a statement ID is named by its quad
      When `<a> <knows> <b>` is held without a statement ID and a read returns it
      Then the row names the statement by `<a>`, `<knows>`, `<b>` and its graph alone

    Scenario: Parallel edges are told apart by their reifiers
      When `<a> <knows> <b>` is held without a statement ID, a parallel edge of it is inserted, and a read returns both
      Then two rows return, each carrying its own statement's reifier IRI, and neither carries a statement ID

    Scenario: A reverse read names each parallel edge's reifier
      When `<a> <knows> <b>` is held as two parallel edges, and a read binds its object to `<b>` and its predicate to `knows`
      Then two rows return for `<a>`, each with its statement's reifier from the reverse list's object record, and compute sends no Scan of a forward list

    Scenario: An unbound graph is walked by Scans of limit 1
      When `<a> <p> <b>` is held in `<g1>` and `<g2>`, and a read of `<a> <p> ?o` leaves the graph unbound
      Then compute finds each graph by a Scan of limit 1 from the keyspace's tenant and then from just past the graph found, cancels each after its first frame, and returns both statements

  Rule: Resume keys continue their own read

    Scenario: A read resumes where it stopped
      When a read with limit 2 matches 5 rows and is resumed twice
      Then the three answers return rows 1–2, 3–4 and 5, the last with no resume key

    Scenario: A resume key with another pattern is refused
      When a resume key from one read is used with a different pattern
      Then the read is refused with an error naming the resume key

    Scenario: A resume key holds terms, not keys
      When a read with limit 1 over `<a> <knows> ?o` returns `<b>` and its resume key is decoded by the server
      Then the key holds the pattern's hash, its `scope` and the text `http://ex.org/b`, and no system ID or store key

    Scenario: The empty result
      When a read matches nothing
      Then it returns no rows and no resume key

    Scenario: A resume key of another transaction is refused
      When a resume key from one read-write transaction's read is used with the same pattern in another read-write transaction
      Then the read is refused with Error 1, the message naming the resume key

    Scenario: A reader at a named version resumes after a reconnect
      When a read-only transaction at snapshot level reads with limit 2, its connection is lost, and a new Begin names the version the first Begin answered and resumes with the key
      Then the read returns rows 3 onward at that version, neither repeating nor skipping a row

    Scenario: A resumed version no longer retained is refused
      When a reader resumes with a key at a named version the partition has discarded
      Then the read is refused with code 3

    Scenario: A limit stopping inside one object's statements resumes there
      When `<a> <knows> <b>` is held under three statement IDs, and a read with limit 2 is resumed
      Then the first answer returns the first two statements by ascending ID, the second the third, and the partition reads none of the list's prefix

    Scenario: A limit stopping after an object with no statement ID skips it whole
      When `<a> <knows> <b>` and `<a> <knows> <c>` are held with no statement ID, and a read with limit 1 is resumed
      Then the resumed Scan's `after` names `<b>`, compute drops `<b>` from what it answers, and the read returns `<c>`

    Scenario: A limit stopping inside a statement's record resumes there
      When a statement's record holds three properties, and a read of its reifier with limit 2 is resumed
      Then the second answer returns the third property alone

    Scenario: Only the last batch holds a key
      When a read with limit 10,000 stops at its limit after several batches
      Then section 6 of every batch but the last is empty, and the last holds the key

    Scenario: An unlimited read that fails restarts
      When a read with no limit is refused with code 24 after three batches, and its caller runs it again
      Then no batch held a resume key, and the second run returns from the first row

  Rule: A read's transaction names its level

    Scenario: A read after a commit sees it through its token
      When a client commits a write, receives a session token, and begins a transaction with that token whose read asks for the write
      Then the read returns the write

    Scenario: Two reads of one transaction share its snapshot
      When a transaction begins at snapshot level, reads `<a>`'s statements, another transaction commits a new statement of `<a>`, and the first transaction reads `<a>`'s statements again
      Then both reads return the same rows

    Scenario: A monotonic-fast read does not wait for a lagging partition
      When partition 2 has closed epoch 9 and partition 1 epoch 12, and a monotonic-fast read with its subject unbound runs over both
      Then it answers at once, reading both partitions at epoch 9, the lower of their closed epochs

    Scenario: Monotonic-fast reads never go back
      When a monotonic-fast transaction's first read answers at epoch 9, and its second read touches only a partition whose latest closed epoch is 12
      Then the second read answers at epoch 12 without waiting

    Scenario: A monotonic-fast read never answers below the token
      When a read-only monotonic-fast transaction carries a token of partition 1 at epoch 20, and its read touches partition 1, closed at 21, and partition 2, closed at 18
      Then the read waits until partition 2 has closed epoch 20, then answers at epoch 20 and returns the token's commit

    Scenario: A latest-state read across partitions is refused
      When a transaction at latest-state level reads with its subject unbound over two partitions
      Then the read is refused with an error naming the level and the two partitions

  Rule: A read's batches fit the client's frame

    Scenario: A batch stops at the client's frame
      When a client whose Hello names `max_frame` 524,288 reads more rows than one such frame holds
      Then every Result frame it receives is at most 524,288 bytes, and every row arrives

    Scenario: A row too large for the client's frame is refused
      When a client whose Hello names `max_frame` 524,288 reads a row holding a literal of 600,000 bytes
      Then the read is refused with code 6, `detail` 524,288

    Scenario: A resume key that does not fit goes alone
      When a read stops at its limit, and its rows fill the last batch so that its resume key does not fit
      Then the stream's last batch holds `rows` 0 and the key, spending one credit, and the batch before it is not flagged `LAST`

    Scenario: A read's stream starts with the frontend's credit
      When a client's Welcome names `initial_credit` 4, compute's Welcome to the frontend names 16, and a read's answer takes more frames
      Then compute sends 4 frames, and one more for each frame of the client's Credit the frontend relays

  Rule: Compute relays what a partition refuses, and refuses a frame it cannot trust

    Scenario: A partition's refusal is relayed with its detail
      When a partition refuses a Scan with Error 1 of `detail` 1
      Then compute relays the Error with its `detail`

    Scenario: A frame compute refuses closes the connection
      When a partition's answer holds a frame compute refuses, while another read waits on that connection
      Then compute closes the connection, answers the read with code 17 and sends it no Scan again

    Scenario: A partition waiting its turn in a merge fails no read
      When a partition's Scan stalls behind the merge, once between two keys and once inside a list
      Then the read reaches `LAST` with each row once, and the Scan went on from where it stopped

    Scenario: A partition that stops answering fails the read in time
      When a partition takes a Scan and sends nothing more, its connection open
      Then compute answers the read with code 17 naming the partition, within twice its `scan_stall_ms`, and dials it again

    Scenario: A Scan that stalled before giving a key refuses the read
      When a partition's Scan stalls before giving a key
      Then the read is refused with code 24, `detail` that partition

    Scenario: A corrupt stored record fails its own read alone
      When a statement's stored record does not read, and a client's connection then reads another list of that partition
      Then the first read is refused with code 17, and the second is answered over the same connection to the partition

    Scenario: A later latest-state reverse read of another home is refused
      When a read at latest level bound to one partition reverses over an object another partition homes and nobody holds
      Then the read is refused with code 25

    Scenario: A reverse read at a named version finds an edge whose member stands above it
      When an edge's commit version in its partition lies below its member's version in the home's own order, and a reverse read names the edge's version
      Then the read returns the edge

    Scenario: A resumed statement read passes a property removed since
      When a read of a reifier at latest level with limit 2 is resumed, the last property it returned having been removed
      Then the resumed read returns the third property alone

    Scenario: A limit asks only its own rows' names
      When a read with limit 2 reads a list of many objects held by hash
      Then it asks the partition's dictionary for the names of those rows alone

    Scenario: A client holding no credit holds a frame's worth of names
      When a client of no further credit reads a list whose objects' texts come to many frames
      Then the partition sent no more than a few frames of them

    Scenario: Concurrent first reads of a core share one connection per partition
      When two lookups start together on a core that holds no connection
      Then each partition took one connection

  Rule: A read outlasting the retention keeps its version

    Scenario: A read past the retention keeps its version
      When a read of two graphs stalls on its client's credit past `read_retain_ms`, and goes on
      Then the read reaches `LAST`, and no Scan of it is refused with code 3

    Scenario: A pin refused holds up no other
      When a lookup outlasts its retention over three partitions, and one partition refuses its pin
      Then the other partitions' pins go on taking frames
