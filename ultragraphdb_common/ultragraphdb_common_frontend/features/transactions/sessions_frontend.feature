# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Exactly-once commits, the frontend's side
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md

  Scenario: A session belongs to its principal
    When principal `alice` registers a session, and a connection authenticated as `bob` commits through it in a partition where `alice` has not yet committed
    Then the commit is refused with code 21, `detail` 2, nothing is committed, and `alice`'s next request through the session is served

  Scenario: An evicted session is told so
    When a session is evicted from the full registry and its client sends its next request
    Then the frontend refuses the request with an error saying the session was evicted, and no partition sees it

  Scenario: A retry of a request never committed is told so
    When a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7
    Then the request is refused as not committed, `detail` 8, and nothing is committed

  Scenario: A Recall reaching no answer stays in doubt
    When a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall
    Then the frontend answers code 17, and the client resends after a delay

  Scenario: Any other refusal of a Recall stays in doubt
    When a retried Commit is recalled over two partitions, partition 1 answers 18 and partition 2's address refuses the Recall with Error 1, serving another partition
    Then the frontend answers code 17, and the client resends after a delay

  Scenario: A stored reply outranks every other Recall answer
    When a retried Commit is recalled over three partitions, one answers the stored reply, one 14, and one's connection closes
    Then the client receives the stored reply

  Scenario: A retry of a session evicted meanwhile is unknown
    When a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile
    Then the frontend answers code 12, and the client reports the outcome as unknown

  Scenario: A one-frame write's resend is decided by the table
    When a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again
    Then the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version

  Scenario: A token above the open epoch is refused
    When the sequencer's open epoch is 30, and a Begin, a DirectWrite and a ConditionalWrite each carry a token of epoch 31
    Then each is refused with Error 1 on its stream, the message naming epoch 31

  Scenario: A write reaching two partitions is refused
    When a DirectWrite writes subjects of partitions 1 and 2
    Then the frontend refuses it with code 23 before any relay, and neither is held

  Scenario: A declaration in a write is refused
    When a DirectWrite holds a declaration of a predicate
    Then it is refused with a protocol error, and the catalog is unchanged

  Scenario: A ConditionalWrite claiming a value goes to its claim
    When a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of `epi.id` `"x"`, and partition 2 owns the bucket of `"x"`
    Then the frontend relays it to partition 2, and its reply names the node

  Scenario: A resent blank-node write reaches one session table
    When a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client sends it again
    Then both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the partitions holding the tenant's run, and the second is answered the stored reply

  Scenario: A transaction ending before its Commit is relayed lets the session go
    When a Commit through a session waits behind a Write its partition refuses, and the client then commits through that session again
    Then the second Commit is answered, not refused as busy

  Scenario: A session with a commit in flight is not evicted
    When the registry holds `sessions_max` sessions, the oldest has a Commit at its partition unanswered, its client's connection closes, and another session is registered
    Then the registry evicts the next oldest session instead, and the oldest only once its partition has answered the Commit

  Scenario: A full registry of commits in flight refuses a Register as busy
    When the registry holds `sessions_max` sessions, each with a commit in flight, and a client registers another
    Then the Register is refused with code 14, nothing is evicted, and a Register after a commit is answered is served

  Scenario: A Cancel after a commit reaches its partition holds its session from eviction
    When the registry holds `sessions_max` sessions, one with a Commit and one with a one-frame write at its partition unanswered, their client cancels both streams, and registers another session
    Then the Register is refused with code 14, evicting neither, and a Register once the partition has answered both is served
