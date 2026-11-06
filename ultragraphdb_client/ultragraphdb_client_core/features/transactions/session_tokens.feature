#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Commits answer a session token
  The client's side: it reads the token off a reply, keeps the highest epoch per session, and sends it on.

  Scenario: A token names its commit
    When a transaction commits in partition 1 at position 40 with a version of epoch 12
    Then its reply carries the token (partition 1, position 40, epoch 12)

  Scenario: A reply through no session carries no token
    When a transaction and a Drop each commit through no session, `session` all zero
    Then each reply's `commit_version` is its version and its token is 24 zero bytes, and a receiver accepts it

  Scenario: A one-frame write carries the session's token
    When a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored reply's is of epoch 9, then sends a ConditionalWrite through the session
    Then the ConditionalWrite carries the token of epoch 12

  Scenario: A zero token replaces nothing
    When a client's session holds a token of epoch 12, and a commit through no session answers the zero token
    Then the session still sends the token of epoch 12
