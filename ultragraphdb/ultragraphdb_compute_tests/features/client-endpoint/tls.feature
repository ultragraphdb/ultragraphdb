# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The client endpoint speaks TLS 1.3 when configured
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A TLS client reaches the endpoint
    When the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates
    Then the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed

  Scenario: A clear-text client is refused by a TLS endpoint
    When a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS
    Then the connection fails as a TLS handshake failure, and no frame of the wire protocol is read

  Scenario: An endpoint with no TLS decision does not start
    When the frontend starts with neither a certificate nor `plaintext = true` in its `[endpoint]` table
    Then it exits with code 78 and an error naming `tls_cert_file`, `tls_key_file` and `plaintext`
