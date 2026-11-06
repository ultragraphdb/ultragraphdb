# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A write commits in one frame
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through a real storage process beside a real frontend.

  Scenario: An insert commits in one request
    When a client sends a DirectWrite inserting `<a> <knows> <b>` through its session
    Then the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply

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

  Scenario: A retried write commits once
    When a client loses the reply to request 7 of its session, a DirectWrite, and sends it again
    Then it receives the same reply and commit version, and the writes are held once

  Scenario: A commit-time operation's node is named by its IRI
    When a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction commits
    Then the Commit reply names `http://ex.org/x` by its text, and holds no system ID
