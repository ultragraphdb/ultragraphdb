#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the system group's commands
  openspec/changes/stage-5-8-system-group/specs/wire-protocol/spec.md, "The system group's commands are bodies of the
  protocol": Session (534), Check (535), Ended (536), Declare (538) and Defaults (539), Challenge (519) and Verify
  (520), the stage 5.8 fields of Subscribe and Catalog, and number 540, kept and refused. These are the scenarios whose
  decision this crate makes: each body's bytes, read and built, its checks in its Order line's order. Which role may
  send what is the receiving session's to enforce, through the view; the system group's sessions serve the commands
  in their own features.

  Scenario: A Check's sessions ascend
    When a Check holds two checks whose sessions are equal, or the second below the first
    Then it is refused with `FieldValue` naming `check` and the second check's index

  Scenario: A Check request holds no outcome
    When a Check request holds an outcome other than 0, or a reply an outcome of 0 or 7
    Then each is refused with `FieldValue` naming `check` and the check's index

  Scenario: A Subscribe from the latest close names no epoch
    When a Subscribe holds `latest` 1 with `from` 5, or `latest` 1 on a connection whose role is `storage`
    Then each is refused with `FieldValue` naming `latest`

  Scenario: A Catalog request may name an epoch only without a version
    When a Catalog request names `version` 3 and `epoch` 9
    Then it is refused with `FieldValue` naming `catalog`

  Scenario: An Ended names at least one session
    When an Ended holds an empty section 0, or a section of 24 bytes
    Then the first is refused with `CountMismatch` naming `sessions`, the second with `SectionLength`

  Scenario: A Session names a session
    When a Session's `session` is all zero
    Then it is refused with `FieldValue` naming `session`

  Scenario: A Declare of a Drop holds no declaration
    When a Declare of kind 2 holds a record in section 0, or names `drop` 1 with no term
    Then the first is refused with `CountMismatch` naming `declarations`, the second with `CountMismatch` naming `drop terms`

  Scenario: A Declare of declarations holds at least one
    When a Declare of kind 1 holds no record in section 0
    Then it is refused with `CountMismatch` naming `declarations`

  Scenario: Defaults continues across frames in order
    When a tenant's default-entry predicates fill two Defaults frames
    Then the first carries no `LAST` and the second does, both hold one command header, and the predicates ascend by their texts' bytes across both

  Scenario: Each new body has a golden frame
    When the golden frames of Session, Check, Ended, Declare, Defaults, Challenge and Verify are decoded and built again
    Then each decodes to the fields its generator names and builds to the same bytes

  Scenario: Command 540 is not served
    When a role connection sends command 540
    Then it is refused as an unknown command
