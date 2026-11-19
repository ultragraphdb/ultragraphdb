# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The endpoint asks the system group and keeps a view of what the closes announce
  openspec/changes/stage-5-8-system-group/specs/client-endpoint/spec.md, transactions/spec.md and wire-protocol/spec.md:
  each endpoint core's link to a system group a test plays.

  Scenario: One turn's commits share one Check frame
    When eight connections of one endpoint core each queue the check of a Commit through a session of their own before the core's sending task runs next
    Then the core sends one Check frame holding eight checks, in ascending order of their sessions, and routes each Commit by its own outcome

  Scenario: Two requests of one session in one turn take two frames
    When a Commit and a resend through one session arrive at one core within one turn
    Then the turn's Check frame holds one check of that session, the second request's check goes in the next frame, and the core answers neither by itself

  Scenario: A request that ends during an outage is ended on the next connection
    When a core's connection to the system group closes while a Commit of a session is at its partition, the partition answers the Commit, and the core dials again
    Then the client receives the Commit's reply, the core sends an Ended naming the session on the new connection, and the session's next Commit passes its check

  Scenario: A resend on another core is told the original is on its way
    When a Commit of a session is at its partition, its reply not yet sent, and the client resends it on a connection to another endpoint core
    Then the resend is answered code 14, and no partition is asked a Recall

  Scenario: A Begin asks no other process
    When an authenticated client sends a Begin naming tenant 0
    Then compute answers with the read version of the latest close its view holds, and sends no frame to the system group for it

  Scenario: A core that starts late begins at the latest close
    When an endpoint core subscribes with `latest` after the sequencer has closed 40 epochs and trimmed the first 30
    Then the first close on its stream is that of epoch 40, and its view holds epoch 40 before it answers its first Begin

  Scenario: A token ahead of the core's view waits and is not refused
    When a client's commit returned a token of epoch 9 while the core's stream has delivered the close of epoch 7, and the client sends a Begin naming the token on that core
    Then the Begin is answered once the close of epoch 8 arrives, and is refused with code 24 only if its deadline is reached with that close not arrived

  Scenario: A token carries the floor to another connection
    When a client reads at epoch 9 on one connection, commits there, and begins a transaction naming the commit's token on a connection to a core whose view holds epoch 8
    Then the second transaction's snapshot is at or above the token's epoch

  Scenario: A stale view decides nothing
    When one core commits a declaring transaction that makes version 4 of `t1`, and before the next close another core commits a declaring transaction built on version 3
    Then the second Commit is refused with code 2, kind 5, the message naming versions 3 and 4, and the client's retry after the close builds on version 4

  Scenario: Describe never reports a version being checked
    When a tightening of `nick` is being checked, and a client describes the tenant on a core that did not send the Declare
    Then the Describe reports the version before it

  Scenario: A subscription opened again reads what its skipped closes announced
    When a core's view holds the close of epoch 7 and version 3 of `t1`, its subscription is opened again with `latest`, and the first close on the new stream is that of epoch 12, version 4 of `t1` and a placement version having been announced by the closes of epochs 8 to 11
    Then the core reads Placement and, with Catalog, the current version of `t1` before it answers its next Begin, and its view then holds epoch 12, version 4 of `t1` and the placement in effect

  Scenario: A source past its failures is throttled, then banned
    When one address fails to authenticate past the throttle's threshold, and then past the ban's
    Then its attempts past the first threshold wait a growing delay, its connections past the second are closed as accepted, another address still authenticates, and once the ban's time has passed the address authenticates again

  Scenario: A retry checked in the quiet time stays undecided
    When a retried Commit is recalled, every partition answers 18, and the second Check answers 14, the system group having begun to listen less than twice `transaction_ms` before
    Then compute answers code 14, and the client resends after a delay

  Scenario: A Describe continues across frames
    When a tenant's Describe answer takes two frames within the client's `max_frame`
    Then compute sends two Describe frames with one command header under the client's credit, the first without `LAST` and the second with it, and the client reads the declarations one frame would hold

  Scenario: A Describe past the catalog's bound is refused
    When a tenant's Describe answer's sections hold more than `catalog_bytes_max` bytes together
    Then it is refused with code 6, `detail` `catalog_bytes_max`
