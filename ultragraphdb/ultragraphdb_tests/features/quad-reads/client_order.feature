# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@core
Feature: A client's reads and writes in order
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A read sees its transaction's new term and its emptied list
    When a transaction writes `<new> <p> "x"`, creating `<new>`, removes every object of `<a> <p> ?o`, and reads both
    Then the read returns `"x"` for `<new>` with `<new>` named by its text, and nothing for `<a>`

  Scenario: A write waits for a read sent before it
    When a client sends a read of its transaction, and a write on another stream before the read is answered
    Then the frontend sends the write only after the read is answered, and the read returns none of it

  Scenario: A read waits for a write sent before it
    When a client sends a write and then a read of its transaction on another stream, without waiting for the write's reply
    Then the frontend sends the read to compute only after the write is answered, and the read returns the write

  Scenario: A limit stopping before a graph leaves that graph unread
    When T1 reads `<a> <name> ?o` with limit 1 and its graph unbound, `<a>` holding it in two graphs, T2 commits an insert into the graph T1's read did not reach, and T1 commits a write
    Then T1 commits

  Scenario: A Commit takes the writes held behind a read
    When a read of a transaction is open, a Write the client sent after it is held behind it, and the client sends Commit
    Then the frontend cancels the read, sends the Write on the Bind stream, then the Commit; the Write is in what commits, and no further frame of the read reaches the client

  Scenario: An Abort drops what a read held back
    When a read of a transaction is open, a Write is held behind it, and the client sends Abort
    Then the frontend cancels the read, the partition never receives the Write, and nothing of the transaction is held

  Scenario: A read the client cancels releases the write behind it
    When a read of a transaction is open, a Write is held behind it, and the client cancels the read
    Then the frontend sends the read's Cancel and then the Write, without waiting for the read's `LAST`

  Scenario: A read cancelled while it is held is dropped unsent
    When a Write of a transaction is unanswered, a read behind it is held, and the client cancels the read
    Then no ComputeRead is sent for it, and the transaction goes on

  Scenario: A read already answered takes nothing further
    When a read of a transaction has reached its `LAST`, and the transaction then ends with an Error
    Then the frontend sends nothing further on that read's stream

  Scenario: A graph appearing in a gap conflicts
    When T1 reads with its graph unbound through compute, finding `<g1>` alone, T2 commits a statement in a graph the walk passed over, and T1 commits a write
    Then T1 aborts with a conflict of kind Range
