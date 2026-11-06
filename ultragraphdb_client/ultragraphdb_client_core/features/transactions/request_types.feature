#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A request declares its type
  The client's side: a write in one frame, under the same request counter and in-doubt rules as a commit.

  Scenario: An insert commits in one request
    When a client sends a DirectWrite inserting `<a> <knows> <b>` through its session
    Then the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open transaction for it at any point after the reply

  Scenario: A failed condition is an answer
    When a ConditionalWrite holds a compare-and-set whose expected value does not hold
    Then it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written

  Scenario: A retried write commits once
    When a client loses the reply to request 7 of its session, a DirectWrite, and sends it again
    Then it receives the same reply and commit version, and the writes are held once

  Scenario: A declaration in a write is refused
    When a DirectWrite holds a declaration of a predicate
    Then it is refused with a protocol error, and the catalog is unchanged
