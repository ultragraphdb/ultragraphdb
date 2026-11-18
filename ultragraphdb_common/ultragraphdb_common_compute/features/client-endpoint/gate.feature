# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Every request naming a tenant passes one gate
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A command of a later stage is refused on its stream
    When an authenticated principal sends an Install naming tenant 0, a Query in an open transaction, and a Prepare of `language` 1
    Then each is refused with code 7 on its stream, the message naming the command and its stage, the Query's transaction stays open, and the connection stays open

  Scenario: The gate allows an authenticated principal in this stage
    When an authenticated principal sends a Begin naming tenant 0
    Then the gate passes it, and the Begin is answered

  Scenario: An unknown tenant is refused after the gate
    When an authenticated principal sends a Begin, a Prepare, a Describe, an Install, a Drop, a DirectWrite and a ConditionalWrite, each naming tenant 9, which the metadata group does not hold
    Then each is refused with code 7, `detail` 9, the message naming tenant 9, and no other role sees it

  Scenario: A principal the gate denies learns nothing of a number
    When the gate, built over an authorizer denying `ann` every action, is asked for tenant 0, which exists, and tenant 9, which does not
    Then each request is refused with code 21, `detail` its action, and the two refusals differ in nothing else

  Scenario: The gate fails closed
    When the gate is asked of an action or a scope it does not know
    Then it denies, and the request is refused with code 21

  Scenario: A denied request reaches no role
    When the gate denies a DirectWrite
    Then it is refused with code 21, `detail` 2, and no partition sees it
