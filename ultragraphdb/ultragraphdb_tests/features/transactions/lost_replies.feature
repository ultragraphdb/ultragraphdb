# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once through a network that loses frames
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. A link between the client and the frontend takes the frame the scenario names, and the reply it took is compared with the resend's.

  @both_shells
  Scenario: A retried commit commits once
    When a client commits request 7 of its session, loses the reply, and sends request 7 again
    Then it receives the same reply and commit version, and the transaction's writes are held once

  Scenario: A session commits through two partitions
    When a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses the reply to request 3 and sends it again on a new connection
    Then all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's stored reply from partition 1

  Scenario: A retry of a request never committed is told so
    When a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7
    Then the request is refused as not committed, `detail` 8, and nothing is committed

  @both_shells
  Scenario: A retried write commits once
    When a client loses the reply to request 7 of its session, a DirectWrite, and sends it again
    Then it receives the same reply and commit version, and the writes are held once

  Scenario: A resent blank-node write reaches one session table
    When a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client sends it again
    Then both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the partitions holding the tenant's run, and the second is answered the stored reply
