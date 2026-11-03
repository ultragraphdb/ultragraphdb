# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The sequencer
  The sequencer's state machine over its ordered log (openspec/changes/stage-5-5-three-role-refactor/specs/sequencer/spec.md):
  every scenario of every requirement, verbatim. The scenarios of "Partitions learn each closed epoch and stamp the open
  one" run here from the sequencer's side, against a model of a partition the sequencer's deliveries drive; groups 4
  and 7 run them against the partition's own code. The exit code a refused setting stops the frontend with is group
  11's (role-binaries); here the setting's refusal is checked.

  Rule: The sequencer closes epochs at its own wall-clock time

    Scenario: Epochs close in order
      When the sequencer runs for 100 ms with `epoch_ms_min` 5 and `epoch_ms_max` 10 on a stepped clock
      Then it closes between 10 and 20 epochs, numbered consecutively from 1

    Scenario: Epoch lengths out of their bounds stop the start
      When the frontend starts once with `epoch_ms_min` 0, once with `epoch_ms_min` 1,001, once with `epoch_ms_min` 5 and `epoch_ms_max` 4, and once with `epoch_ms_max` 10,001
      Then each start exits with code 78 and an error naming the setting and its bound: 1, 1,000, `epoch_ms_min` and 10,000

    Scenario: A partition's request closes the epoch early
      When `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a partition asks for a close
      Then the sequencer closes epoch 7 once 2 ms have passed since epoch 6 closed, not at 10 ms

    Scenario: A waiting read closes the epoch early
      When `epoch_ms_min` is 2 and `epoch_ms_max` 10, epoch 6 closed 1 ms ago, and a snapshot read waits on the open epoch 7
      Then the frontend asks its sequencer for a close, and epoch 7 closes once 2 ms have passed since epoch 6 closed

    Scenario: A clock stepping back does not lower close times
      When the clock steps back by one second between two closes
      Then the later close's recorded time equals the earlier close's time, not one second less

    Scenario: The last epoch number is refused
      When the sequencer's last closed epoch is 2^42 − 1 and it is asked to close the next
      Then the close is refused with an error naming the 42-bit limit

  Rule: A time maps to the epoch closed at or before it

    Scenario: A time between two closes
      When epoch 4 closed at 1,000 µs and epoch 5 at 2,000 µs, and the time 1,500 µs is mapped
      Then the result is epoch 4

    Scenario: A time before the first close is refused
      When the first close was at 1,000 µs and the time 500 µs is mapped
      Then the mapping fails with an error naming 500 µs and 1,000 µs

  Rule: Partitions learn each closed epoch and stamp the open one

    Scenario: A partition stamps the open epoch
      When epochs 1 to 7 are delivered to a partition and it commits a transaction
      Then the commit's version carries epoch 8

    Scenario: A partition stamps no epoch ahead of its closes
      When epochs 1 to 7 are delivered to a partition, the sequencer has closed epoch 8, and its close has not yet reached the partition
      Then the partition's commits still carry epoch 8, and its first commit after the close of 8 arrives carries epoch 9

    Scenario: A partition closes an epoch once its rounds are applied
      When a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered
      Then the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits until then

    Scenario: A reconnecting partition catches up
      When a partition loses its connection after epoch 3 and reconnects after epoch 9 closed
      Then it receives epochs 4 to 9 in order before any later close

  Rule: Closes announce the versions taking effect after them

    Scenario: A version reaches every partition before its epoch
      When a catalog version is pending while epoch 38 is the latest close, and the sequencer closes epoch 39
      Then the close of epoch 39 assigns it effective epoch 40, its durable record holds the assignment, and every partition holds the version before it stamps a commit at epoch 40

    Scenario: A close assigns one placement version
      When placement versions 1 and 2 are pending while epoch 38 is the latest close, and the sequencer closes epochs 39 and 40
      Then the close of 39 assigns version 1 effective epoch 40, and the close of 40 assigns version 2 effective epoch 41

    Scenario: A crash before the close leaves the version pending
      When a catalog version is committed as pending while epoch 38 is the latest close, and the frontend is killed before it closes epoch 39
      Then after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40

  Rule: The sequencer orders multi-partition transactions in its log

    Scenario: Transactions keep their order and epoch
      When transactions A and B are accepted in epoch 6 and C in epoch 7, and the log is read from epoch 6
      Then it returns A and B with epoch 6 and C with epoch 7, in that order

    Scenario: A transaction is placed at or above its reply epoch
      When the latest close is 12 and a multi-partition transaction naming reply epoch 12 is accepted
      Then the log places it in epoch 13

    Scenario: A transaction naming no partition is refused
      When a multi-partition transaction naming no partition is offered
      Then it is refused with an error naming the empty partition list, and the log is unchanged

  Rule: The sequencer is a log-backed state machine with one replica

    Scenario: Recovery rebuilds the sequencer
      When the sequencer closes epochs 1 to 12, accepts three transactions, and is killed and restarted
      Then its last closed epoch is 12, and reading its log returns the three transactions in their order

    Scenario: An unacknowledged close is not delivered
      When the sequencer is killed after appending a close and before the append is durable
      Then no partition received that close before the crash, and after restart the close is either durable and delivered or absent
