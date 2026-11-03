#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The write-ahead log
  The ordered-log interface over the node's write-ahead log: rounds as checksummed, chained frames in preallocated
  segments, synced before they are applied, each append answered by a durability ticket, recovered by a scan that
  tells a torn tail from interior damage, a ping-pong superblock, checkpoints that trim, and a storage seam with a
  simulated implementation. These are the scenarios whose decision the log makes. Where a scenario names the store,
  the store here applies what the log hands it and nothing else; the commit step binds the same scenarios again
  against the real store. "A store ahead of the log refuses to start" and "A store behind its checkpoint refuses to
  start" run there alone: they are decided by the store's applied position, which the log does not hold, and "A crash
  loses no acknowledged round" runs on fjall in the kv store's scenarios.

  Scenario: An append out of order is refused
    When the log holds rounds 1 to 5 and round 7 is appended
    Then the append is refused with an error naming position 6, and nothing is written

  Scenario: A sealed log refuses appends
    When the log is sealed at position 9 and a round is appended
    Then the append fails, and `seal` reported 9

  Scenario: A ticket completes once its round is durable
    When a round is appended and its ticket is polled before and after the sync that covers it
    Then the ticket is pending before the sync and complete after it, and `committed` reaches the round's position only then

  Scenario: A round never reaches the store before the log
    When the node loses power at any step of a round
    Then the store after recovery holds no write of a round the log does not hold

  Scenario: Preallocated zeros are not read as rounds
    When the log is opened with a segment holding three rounds followed by zeros
    Then the scan finds three rounds and its tail after the third

  Scenario: Rounds of an older incarnation are not read
    When a reused segment holds rounds of a previous incarnation after the current rounds
    Then the scan stops after the current incarnation's rounds

  Scenario: Rounds lost from the store are replayed
    When rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost
    Then after recovery the store holds rounds 1 to 10

  Scenario: A torn tail is truncated
    When the last round was torn by a crash during its write, before its sync completed, and no complete round follows it
    Then the log is truncated before it, the truncation is reported, and the round was never acknowledged

  Scenario: Interior corruption refuses to start
    When a byte of an acknowledged round's body is flipped and later rounds follow it
    Then the node refuses to start with an error naming the round, the chunk and its transactions

  Scenario: A corrupt copy is not a fresh deployment
    When one superblock copy is corrupted
    Then the node refuses to start with an error naming the copy, and does not create a new log

  Scenario: A checkpoint recycles old segments
    When rounds fill four segments and a checkpoint is taken at the end of the third
    Then the first three segments are kept as spares holding only zeros, and recovery replays from the checkpoint

  Scenario: The flag does not remove the sync
    When the flag is `true` and a round is committed
    Then the round is synced before it is acknowledged

  Scenario: A reported fault loses no acknowledged round
    When the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged
    Then after recovery every acknowledged round is present

  Scenario: A silent torn write is caught
    When the simulated storage tears the last round's write without reporting it, and the node restarts
    Then recovery detects the round by its checksums, truncates it as a torn tail and reports the truncation; it never reads the round as valid
