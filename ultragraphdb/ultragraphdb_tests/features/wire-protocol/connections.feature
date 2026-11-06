# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@core
Feature: Connections authenticate through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A client authenticates with SCRAM-SHA-256
    When a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password
    Then the server's final Authenticate names the principal, and the client's requests are served

  Scenario: A wrong password is refused without saying why
    When a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal holds
    Then each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same bytes
