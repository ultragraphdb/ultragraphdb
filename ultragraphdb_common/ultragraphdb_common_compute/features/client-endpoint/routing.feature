# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Requests are routed to the partitions and compute that serve them
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A write reaches its partition
    When a client writes one statement per partition, each subject an IRI of that partition's buckets
    Then the frontend sends each Write first to the partition owning its subject's bucket, and each commit's reply reaches the client

  Scenario: A stale placement map is corrected
    When a partition refuses a write naming its new owner
    Then the frontend refreshes its map, resends the write to the owner once, and the client receives the owner's reply

  Scenario: A home's code 26 reaches the client
    When a partition answers a client's first Write with code 26 of `detail` 0, a home having refused its registration
    Then the client receives code 26 on the transaction's stream, and the frontend binds no other partition

  Scenario: Later requests wait for the first
    When a client sends two Writes of a new transaction without waiting, and the partition refuses the first with code 26
    Then the second was sent to no partition; the frontend refreshes its map, binds to the owner, and sends the first and then the second, and the client sees no error

  Scenario: A first read moved by a stale map is resent
    When a transaction's first request is a read, compute answers code 23, and the refreshed map moves the read's subject to partition 2
    Then the frontend cancels the Bind stream, binds to partition 2 and resends the read once, and the client receives its rows

  Scenario: A first read the map does not move is refused
    When a transaction's first request is a read, compute answers code 23, and the refreshed map leaves the read's subject where it was
    Then the client's read is refused with code 23, and its next request on the transaction's stream too

  Scenario: A read's code 23 answers the transaction's other reads
    When reads A and C of a read-write transaction are open and a Write is held behind them, compute answers A with code 23, and the client then sends read D
    Then A, C and D are each answered code 23, the `detail` of A's, the Write is sent to no partition, and the next request on the transaction's stream is answered code 23 and ends it

  Scenario: A first request spreading over partitions is refused before any Bind
    When a transaction's first Write names subjects the frontend's map places in partitions 1 and 2
    Then it is refused with code 23 on the transaction's stream, `detail` partitions 1 and 2, no partition receives a Bind, and the transaction ends

  Scenario: A ConditionalWrite whose operation names another partition's subject is refused before any relay
    When a ConditionalWrite inserts at a subject of partition 2 and adds to a counter of a subject of partition 1
    Then it is refused with code 23, `detail` partitions 1 and 2, and no partition receives it

  Scenario: A first read holds a later read only until its first frame
    When a transaction's first read takes six frames at `initial_credit` 4, and after its first batch the client sends a second read and waits on it
    Then the frontend sends the second read to compute on the first read's first frame, and the client receives its rows with the first read still open
