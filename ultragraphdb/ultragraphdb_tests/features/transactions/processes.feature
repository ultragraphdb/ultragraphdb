# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Sessions under a whole cluster of their own
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A Commit reply past the floor of `max_frame` is refused
    When a transaction's commit-time operations leave a Commit reply whose frame would pass 524,288 bytes
    Then the commit is refused when it is decided, with code 6, `detail` 524,288, taking no commit version and writing nothing

  Scenario: An evicted session is told so
    When a session is evicted from the full registry and its client sends its next request
    Then the frontend refuses the request with an error saying the session was evicted, and no partition sees it

  Scenario: A session with a commit in flight is not evicted
    When the registry holds `sessions_max` sessions, the oldest has a Commit at its partition unanswered, its client's connection closes, and another session is registered
    Then the registry evicts the next oldest session instead, and the oldest only once its partition has answered the Commit

  Scenario: A full registry of commits in flight refuses a Register as busy
    When the registry holds `sessions_max` sessions, each with a commit in flight, and a client registers another
    Then the Register is refused with code 14, nothing is evicted, and a Register after a commit is answered is served

  Scenario: A retry of a session evicted meanwhile is unknown
    When a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile
    Then the frontend answers code 12, and the client reports the outcome as unknown

  Scenario: A one-frame write's resend is decided by the table
    When a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again
    Then the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version

  Scenario: A Recall reaching no answer stays in doubt
    When a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall
    Then the frontend answers code 17, and the client resends after a delay
