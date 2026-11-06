# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@core
Feature: The client endpoint through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. The client offers no Install, so the unknown tenant runs through the six requests it sends.

  @over_tls
  Scenario: A TLS client reaches the endpoint
    When the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates
    Then the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed

  @over_tls
  Scenario: A clear-text client is refused by a TLS endpoint
    When a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS
    Then the connection fails as a TLS handshake failure, and no frame of the wire protocol is read

  Scenario: A wrong password is refused without detail
    When a client authenticates as `ann` with a wrong password, and as an unknown name
    Then both are refused with the same authentication error

  Scenario: The gate allows an authenticated principal in this stage
    When an authenticated principal sends a Begin naming tenant 0
    Then the gate passes it, and the Begin is answered

  Scenario: An unknown tenant is refused after the gate
    When an authenticated principal sends a Begin, a Prepare, a Describe, an Install, a Drop, a DirectWrite and a ConditionalWrite, each naming tenant 9, which the metadata group does not hold
    Then each is refused with code 7, `detail` 9, the message naming tenant 9, and no other role sees it

  Scenario: An empty transaction commits at the frontend
    When a client begins a read-write transaction and sends Commit through its session with nothing else
    Then the frontend answers `commit_version` 0, the token zero and no results, and no partition sees it

  Scenario: A vector index is refused
    When a client declares a vector index on `embedding`
    Then the declaration is refused with an error naming `embedding`, and the catalog is unchanged

  Scenario: A Prepare of a later stage's language is refused
    When an authenticated principal prepares a text of a later stage's language
    Then it is refused with code 7 on its stream, the message naming the command and its stage
