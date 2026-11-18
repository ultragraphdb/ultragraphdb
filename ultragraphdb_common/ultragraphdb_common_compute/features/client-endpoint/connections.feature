# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A client connection
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: Connections past the bound are refused
    When the frontend allows 2 connections and a third client connects
    Then the third connection is refused with the limit error, and the first two keep working

  Scenario: A blocked commit does not block other connections
    When one connection's commit waits on its owner check
    Then reads and writes of other connections on that core go on

  Scenario: A wrong password is refused without detail
    When a client authenticates as `ann` with a wrong password, and as an unknown name
    Then both are refused with the same authentication error

  Scenario: A data request before authentication is refused
    When a client sends a Read before authenticating
    Then the request is refused with the authentication error, and the connection stays open for authentication

  Scenario: An unknown name answers as a known one does
    When a client asks to authenticate as an unknown name, twice, across a restart of the frontend
    Then both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password
