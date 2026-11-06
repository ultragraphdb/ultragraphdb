#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A commit in flight is in doubt once sent
  The client's side of the outcomes a commit, a drop or a one-frame write leaves: lost if its frame never left, in doubt
  once it did, and resolved through its session by a resend with the same request number.

  Scenario: An unsent commit is lost, a sent one in doubt
    When the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered
    Then the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed

  Scenario: Code 17 on a commit is in doubt
    When a client's Commit through a session is answered code 17
    Then the client answers `InDoubt`, keeps the request number, and sends no other commit through the session until a resend resolves it

  Scenario: A resent one-frame write refused for another reason did not commit
    When a DirectWrite in doubt is resent and refused with code 7
    Then the client reports that it did not commit

  Scenario: A commit through no session has an unknown outcome
    When a transaction committing through no session loses its reply
    Then the client reports the outcome as unknown, and resends nothing
