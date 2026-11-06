#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Exactly-once commits through sessions
  The client's side: one counter per session, a request kept in doubt until its resend resolves it. The test server plays
  the frontend's answers; the frontend's and the partitions' sides are theirs.

  Scenario: A retried commit commits once
    When a client commits request 7 of its session, loses the reply, and sends request 7 again
    Then it receives the same reply and commit version, and the transaction's writes are held once

  Scenario: A commit answered code 17 is resolved by its resend
    When a Commit through a session is answered code 17, and the partition had committed it
    Then the client keeps the request number, sends the Commit again on a new stream, receives the stored reply and only then moves its counter

  Scenario: A Recall reaching no answer stays in doubt
    When a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall
    Then the frontend answers code 17, and the client resends after a delay

  Scenario: A retry of a session evicted meanwhile is unknown
    When a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile
    Then the frontend answers code 12, and the client reports the outcome as unknown

  Scenario: A one-frame write's resend is decided by the table
    When a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again
    Then the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version
