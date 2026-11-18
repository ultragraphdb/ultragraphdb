# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Compute keeps a transaction's order
  openspec/changes/stage-5-9-compute-role/specs/quad-reads/spec.md

  Scenario: A write waits for a read sent before it
    When a client sends a read of its transaction, and a write on another stream before the read is answered
    Then compute sends the write only after the read is answered, and the read returns none of it

  Scenario: A read waits for a write sent before it
    When a client sends a write and then a read of its transaction on another stream, without waiting for the write's reply
    Then compute sends the read's first Scan only after the write is answered, and the read returns the write

  Scenario: A Commit takes the writes held behind a read
    When a read of a transaction is open, a Write the client sent after it is held behind it, and the client sends Commit
    Then compute cancels the read, sends the Write on the Bind stream, then the Commit; the Write is in what commits, and no further frame of the read reaches the client

  Scenario: An Abort drops what a read held back
    When a read of a transaction is open, a Write is held behind it, and the client sends Abort
    Then compute cancels the read, the partition never receives the Write, and nothing of the transaction is held

  Scenario: A read the client cancels releases the write behind it
    When a read of a transaction is open, a Write is held behind it, and the client cancels the read
    Then compute sends the read's Cancel and then the Write, without waiting for the read's `LAST`

  Scenario: A read cancelled while it is held is dropped unsent
    When a Write of a transaction is unanswered, a read behind it is held, and the client cancels the read
    Then no Scan is sent for it, and the transaction goes on

  Scenario: A read under way when its transaction ended takes the stream's Error
    When a read of a transaction is under way, the transaction passes its limit, and the Scan's Error 1 of `detail` 1 arrives before the Bind stream's code 4
    Then compute holds that Error until the Bind stream ends, and the client's read is refused with code 4

  Scenario: A read already answered takes nothing further
    When a read of a transaction has reached its `LAST`, and the transaction then ends with an Error
    Then compute sends nothing further on that read's stream

  Scenario: A later latest-state read waits for the first read's partition
    When a transaction at latest level sends two reads beside each other, the first binding it to partition 1
    Then compute starts the second only after the first's first Result names partition 1, bound to partition 1, and refuses it with code 25 when it reaches another partition

  Scenario: A first read refused leaves the next read first
    When a transaction at latest level sends two reads, and the first is refused before any Result
    Then the second goes out as the transaction's first read and binds it

  Scenario: Concurrent monotonic-fast reads take the highest epoch answered
    When a monotonic-fast transaction's read answered at epoch 12 and another at epoch 9, and a third is sent after both
    Then the third read's floor is 12

  Scenario: An Execute whose bound pattern passes a frame is refused
    When an Execute's parameters, bound into its statement's pattern, make the Read it is read as larger than a frame of 1,048,576 bytes
    Then compute refuses it with code 6, `detail` 1,048,576, and no partition receives a Scan of it

  Scenario: An executed statement reads through compute
    When a statement binding a predicate and taking its object as a parameter is prepared, and executed in a snapshot transaction
    Then the partition receives the read's Scans of the statement's pattern with the parameter bound, and the client receives its batch
