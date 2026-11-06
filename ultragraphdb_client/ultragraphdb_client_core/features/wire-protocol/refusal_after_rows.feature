#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A client takes a refusal after rows as the read's failure
  Only a stream ending in `LAST` is an answer, and a retryable error says what runs again.

  Scenario: A refusal after rows fails the read
    When a read delivers two batches and is then refused with code 24
    Then the client returns the two batches and then the refusal, sends no Credit or Cancel on the stream, and the transaction stays open

  Scenario: A code's retry names what runs again
    When a commit is refused with code 2, and a read with code 24
    Then the client's error says of the first that the whole transaction runs again on a new Begin, and of the second that the read runs again

  Scenario: Code 17 runs a read again and leaves a commit in doubt
    When a read is refused with code 17, and a Commit with code 17
    Then the client's error says of the first that the read runs again, and of the second that it is in doubt
