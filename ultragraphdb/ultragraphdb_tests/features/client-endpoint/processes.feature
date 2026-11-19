# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Whole clusters under a client, a role of them misbehaving
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real compute and storage, the client crates' own client sending every request. A role that misbehaves is the real process under a signal; a wait of 35 s runs as one epoch of a cluster whose epochs close at most every second.

  Scenario: Connections past the bound are refused
    When compute allows 2 connections and a third client connects
    Then the third connection is refused with the limit error, and the first two keep working

  Scenario: Compute serves clients on every core
    When 32 clients each commit a write over a connection of their own to a compute node of two partitions
    Then every write is committed, compute logs a runtime per CPU, and more than one of its cores relays the writes

  Scenario: An eviction reaches every core at once
    When a session is evicted while a connection on another core holds it, and that connection commits next
    Then the commit is refused with the eviction's error, and no partition sees it

  Scenario: An unknown name answers as a known one does
    When a client asks to authenticate as an unknown name, twice, across a restart of compute
    Then both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password

  Scenario: A role connection closing under a request answers code 17
    When the connection between compute and a storage process closes under a Write and a Commit
    Then the client receives code 17 for each, the Commit is in doubt, and requests to other partitions still succeed

  Scenario: A read's code 23 answers the transaction's other reads
    When reads A and C of a read-write transaction are open and a Write is held behind them, compute answers A with code 23, and the client then sends read D
    Then A, C and D are each answered code 23, the `detail` of A's, the Write is sent to no partition, and the next request on the transaction's stream is answered code 23 and ends it

  Scenario: A write past `held_bytes` is refused
    When `held_bytes` is 1,024, a read of a transaction is open, and the client sends Writes that the read holds back past 1,024 bytes
    Then the Write past the bound is refused with code 15, `detail` 1,024, on the transaction's stream, and the transaction ends
