# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Transactions and one-frame writes through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A read above the watermark is refused
    When a client's Begin names a read version above the visibility watermark of the partition answering it
    Then it is refused at once with an error naming the version and the watermark, and the worker serves the next request

  Scenario: A transaction sees every commit acknowledged before it began
    When a commit setting `<a> <name> "A"` is acknowledged and a read-write transaction then begins and reads `name` of `a`
    Then it reads `"A"`

  Scenario: A read-only transaction does not abort
    When a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the read-only transaction ends
    Then it ends without an error, having read the value from before the commit

  Scenario: Write skew is refused
    When two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to `false`, concurrently, all four facts in one partition
    Then one of them commits and the other aborts with a conflict

  Scenario: Inserts of different objects into one list both commit
    When two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`
    Then both commit, and `a` knows `b` and `c`

  Scenario: A term found absent conflicts with a statement of it
    When a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the same partition writing `<new> <name> "N"` commits after its read version
    Then the reading transaction aborts with a conflict naming an absent term

  Scenario: An absent term is named before the range that read it
    When a transaction reads `<new> <p> ?o` through compute, finding nothing, so it records both the term's absence and the list's range, and a transaction writing `<new> <p> "x"` commits after its read version
    Then the reading transaction aborts with a conflict of kind AbsentTerm

  Scenario: An aborted transaction writes nothing
    When a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
    Then `<a> <knows> <b>` is not held

  Scenario: A write set over the Welcome's bound is refused
    When a transaction's write set grows past the Welcome's `max_write_set` and it commits
    Then the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it wrote is held

  Scenario: A closed connection aborts its transaction at once
    When a read-write transaction creates `<new>` and the connection carrying it closes after 1 s
    Then the host aborts the transaction then, not at its limit, and its entry for `<new>` is released

  Scenario: A reused number on an open transaction is refused
    When a session's last request in partition 1 is 7, and a new transaction bound to partition 1 sends its Commit as request 7
    Then the Commit is refused with code 13, `detail` 7, the transaction aborts, nothing it wrote is held, and request 7's stored reply is not returned

  Scenario: A DirectWrite on a unique predicate is refused
    When a DirectWrite holds a valid insert and an insert of the predicate `id`, which the catalog declares unique
    Then it is refused with an error naming `id` and ConditionalWrite, and neither insert is held

  Scenario: Concurrent identical inserts both succeed
    When two DirectWrites insert `<a> <knows> <b>` concurrently
    Then both are answered with success, whichever executed first, and the statement is held once

  Scenario: An upsert decides in commit order
    When two ConditionalWrites each insert-if-absent a node with `id` `"x"` concurrently
    Then both name the same node, created once

  Scenario: A failed condition is an answer
    When a ConditionalWrite holds a compare-and-set whose expected value does not hold
    Then it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written

  Scenario: A declaration in a write is refused
    When a DirectWrite holds a declaration of a predicate
    Then it is refused with a protocol error, and the catalog is unchanged

  Scenario: An insert commits in one request
    When a client sends a DirectWrite inserting `<a> <knows> <b>` through its session
    Then the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply

  Scenario: A failed compare-and-set writes nothing
    When a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `"open"` to `"closed"` while `status` is `"done"`
    Then the transaction is refused reporting `"done"`, `status` of `a` stays `"done"`, and `<a> <seen> true` is not held

  Scenario: A counter add returns the counter's new value
    When the counter `next-id` holds 5, and two transactions each add 1 to it
    Then both commit, the one committed first receives 6 and the other 7, and `next-id` holds 7

  Scenario: A created node is named by its skolem IRI
    When a transaction in the partition owning the bucket of `"w"` insert-if-absents a node with `epi.id` `"w"`, declared unique, and no node holds it
    Then the reply names the created node by its skolem IRI, says it was created, and holds no system ID
