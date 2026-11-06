# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads: storage's read kernel
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through a real storage process beside a real
  frontend, decoded as compute will.

  Rule: Storage's read kernel answers what compute scans

    Scenario: A dictionary and a referrer set are read by Scan
      When compute scans the dictionary entry of `<b>` and `R(b)` at `b`'s home
      Then the answer holds `b`'s ID and its referrer set's members at that version

    Scenario: A hub's list crosses frames
      When a reverse list holds more objects than one frame carries, and compute scans it
      Then the answer takes several frames, each ending between two objects, and compute reads every object once, in the list's order

    Scenario: A large list's frames stay within a turn
      When a Scan reads a list holding more bytes than a turn builds, the receiver's `max_frame` larger than that
      Then every Entries frame holds no more than a turn builds, and the list arrives whole

    Scenario: A commit goes on beside Scans taking every turn's budget
      When many Scans of one partition read large lists with credit to spare, and a client commits a write
      Then the commit is answered while the Scans go on, and each Scan reaches `LAST`

    Scenario: A keyspace outside the kernel's is refused
      When a Scan names a range of the marks keyspace
      Then it is refused with a protocol error naming the keyspace

    Scenario: A range of the dictionary holds one key
      When a Scan names a range of the dictionary from one key to another
      Then it is refused with a protocol error naming the range

    Scenario: A Scan waits for the epoch it names
      When a Scan names an epoch the partition has not closed, and the partition then closes it
      Then the answer comes after the close, at that epoch

    Scenario: A Scan past its deadline names the partition
      When a Scan names an epoch the partition does not close before the Scan's deadline
      Then it is refused with code 24 (TimedOut), naming the partition, and returns no entry

    Scenario: A reader granting no credit stops the answer
      When a Scan's answer takes more frames than the stream's credit, and the reader grants no more
      Then it receives as many frames as its credit, and the Scan is refused with code 24 (TimedOut), naming the partition, once `scan_stall_ms` has passed

    Scenario: A reader granting credit reads the rest
      When a Scan's answer takes more frames than the stream's credit, and the reader grants credit as it reads
      Then it receives every range in order, the last frame carrying `LAST`

    Scenario: A version above the watermark is refused at once
      When a Scan with `at` 4 names a version above the partition's visibility watermark
      Then it is refused with code 27 (AheadOfWatermark), `detail` the watermark

    Scenario: A named version in a closed epoch reads on an idle partition
      When a partition has closed an epoch, its visibility watermark lies below that epoch's version bound, and a Scan with `at` 4 names that bound
      Then it reads what the partition applied, and is not refused

    Scenario: A Scan reads one version across a close
      When a Scan with `at` 3 has two ranges, and the partition closes another epoch between their frames
      Then both ranges read the version chosen first, and every frame names its epoch

    Scenario: A Scan that ends its transaction says so on the Bind stream
      When a Scan's span has no room in the transaction's read set
      Then the Scan's stream and the Bind stream each carry code 6 (TooLarge), and the transaction's next request is refused

    Scenario: Closed answers each partition's latest closed epoch
      When compute sends Closed naming the partition at its role address
      Then the reply holds the partition's latest closed epoch

    Scenario: A range reads one tenant of one keyspace
      When Scans name a first key too short for a tenant, a past key beyond the tenant, and the first key past it
      Then the first two are refused with a protocol error naming the range, and the third reads to the tenant's end

    Scenario: A range holding exactly its limit ended
      When a Scan's limit equals what a range holds, and is one below what another holds
      Then the first range ends with `end` 1, a one-key range of the dictionary too, and the other with `end` 2

    Scenario: A Scan of a transaction sees its own write and holds what it read
      When a transaction writes `<a> <p> <b>`, a Scan naming it reads the list, a second transaction commits into that list, and the first commits
      Then the Scan's frames hold `<b>`, a Scan naming no transaction does not, and the first commit aborts with a conflict of kind Range

    Scenario: A Scan of a transaction names what is the transaction's
      When Scans name a transaction with a wrong read version, a wrong principal, another tenant, a referrer set, a key not its term's, or an identity it does not hold
      Then each is refused with a protocol error naming what is wrong, and the transaction goes on

    Scenario: A second Bind of an identity the partition holds is refused
      When a Bind names a transaction the partition holds
      Then it is refused with a protocol error naming the transaction

    Scenario: A Scan's foreign term comes from its home in another process
      When a transaction on partition 1 names in a Scan an IRI a partition of another process owns
      Then the range answers the ID that process's dictionary holds

    Scenario: A Scan of a partition the address does not serve is refused
      When a Scan and a Recall name a partition the role address does not serve
      Then each is refused with a protocol error naming the partition

    Scenario: Every range is checked before any frame
      When Scans hold a valid range and then one of the marks keyspace, of a split part, of an unknown keyspace, of the dictionary with no past key, of the referrer sets with a first key too short, or of the referrer sets past their tenant
      Then each is refused with a protocol error naming what is wrong, and no frame precedes the refusal

    Scenario: A record too large for a frame is refused
      When the dictionary or the statements hold a value too large for the receiver's `max_frame`
      Then a Scan of it is refused with code 6 (TooLarge), `detail` that `max_frame`

    Scenario: A stalled Scan leaves another running
      When one Scan of a connection holds no credit and another takes its credit as it reads
      Then the second reaches `LAST` while the first takes no more frames

    Scenario: A range of emptied lists takes many turns
      When a range holds more emptied lists than a turn reads, and a list holding an object after them
      Then the answer holds that list alone, and the range ends with `end` 1

    Scenario: A Scan of a transaction ends with it
      When a Scan of a transaction waits for credit, and the transaction aborts
      Then the Scan is refused at once with Error 1, `detail` 1, as one the partition does not hold

    Scenario: Scans of a transaction wait on its home together
      When two Scans of a transaction overlap, each naming an IRI of a home that has not answered
      Then neither is refused, and each range answers the ID its home gave

    Scenario: A Scan arriving while its transaction's write waits on a home is refused
      When a transaction's Write waits on a home's answer, and a Scan of the transaction arrives
      Then the Scan is refused with a protocol error naming the write

    Scenario: A failed store under a range is no absence
      When the partition's store fails reading a key of a Scan's range
      Then the Scan is refused with code 17, the message naming the key, and no frame says the range ended

    Scenario: A Commit behind a cancelled read's wait takes its turn
      When a Scan of a transaction waits on a home's answer, the frontend cancels the read and sends the Commit
      Then the partition answers the Commit once the home's reply is in, and the Commit is not refused

    Scenario: An unheld skolem IRI answers no entry wherever it is homed
      When a transaction names in a Scan a skolem IRI no dictionary holds, homed on another partition
      Then the range answers no entry with `end` 1, and the transaction goes on

    Scenario: A home's refusal ends the transaction on both streams
      When a transaction names in a Scan an IRI its home refuses, and the IRI is no skolem IRI
      Then the Scan's stream and the Bind stream each carry the home's refusal

    Scenario: A statement's record is read by Scan
      When compute scans the key of a statement's record at its subject's partition, at a version
      Then the entry's value is the record's stored bytes at that version

    Scenario: A Scan of a transaction sees its statement's own property
      When a transaction sets `<since> 2020` through the reifier `r`, the write is answered, and a Scan naming the transaction reads the statement's record
      Then the record holds `<since> 2020` over what the Bind's version holds, a property the transaction removed is absent from it, and its span is recorded as a range scanned

    Scenario: A Scan's ranges name one tenant
      When a Scan's second range holds a first key of another tenant than its first range's
      Then it is refused with Error 1, the message naming the range, and no frame precedes the refusal

    Scenario: A run of dictionary lookups answers in one frame
      When a Scan holds 50 one-key ranges of the dictionary, 3 of whose keys are absent
      Then one Entries frame answers them, `range` 0, `through` 49, `end` 1, holding 47 entries in the ranges' order

    Scenario: A resumed range reads its list from `after`
      When a list holds `<a>`, `<m>` and `<z>`, and one range names `after` `<m>` and another `after` `<n>`, which the list does not hold
      Then the first answers `<m>` and `<z>`, the second `<z>`, and neither reads the record of `<a>`

    Scenario: A malformed `after` is refused
      When a range's `after` is not one whole object encoding, and another range of the dictionary holds an `after`
      Then each is refused with Error 1, the message naming the range, and no frame precedes the refusal

    Scenario: A home's failed store ends the transaction
      When a transaction names in a Scan a skolem IRI whose home's store fails under the registration, which answers outcome 17
      Then the Scan's stream and the Bind stream each carry code 17, and the range answers no entry

  Rule: A partition retains what a read may still read

    Scenario: A reader coming back within the retention keeps its version
      When a read-only transaction at snapshot level reads at epoch 20, epochs close for 30 s with `read_retain_ms` 60,000, and it reads again
      Then the second read answers at epoch 20's version bound

    Scenario: A reader coming back past the retention is refused
      When a read-only transaction at snapshot level reads at epoch 20, the partition raises its discard version past that version bound once a later epoch has been closed for more than `read_retain_ms`, and it reads again
      Then the read is refused with code 3

    Scenario: An open Scan holds its version past the retention
      When a Scan keeps taking frames for longer than `read_retain_ms` while later epochs close
      Then the discard version stays at or below the Scan's version, and the Scan reaches `LAST` at that version

    Scenario: A next Scan past the retention is refused after a long Scan
      When a Scan at epoch 20's version bound stays open for 90 s with `read_retain_ms` 60,000 while later epochs close, it ends, the partition raises its discard version past that bound, and a next Scan at that version arrives
      Then the next Scan is refused with code 3

    Scenario: A read-write transaction holds its version
      When a read-write transaction bound at a version is open while epochs close for longer than `read_retain_ms`
      Then the discard version stays at or below the Bind's version until the transaction ends

  Rule: A partition answers a Recall from its session table

    Scenario: A Recall returns the stored reply
      When a session's request committed, and the frontend recalls it
      Then the partition answers with a Commit frame holding what the commit's reply held

    Scenario: A Recall of what the table does not hold
      When the frontend recalls a request the table holds no reply for, then a request below the session's last
      Then the first is refused with code 18 (NotCommitted), and the second with code 13 (RequestOutOfOrder), `detail` the last
