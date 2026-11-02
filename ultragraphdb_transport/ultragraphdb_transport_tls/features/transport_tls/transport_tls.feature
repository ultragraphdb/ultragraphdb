# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Transport TLS
  The certificates a connection is verified against, and connections verified against them
  (openspec/changes/transport-tls/specs/transport-tls/spec.md).

  Scenario: The bundle SSL_CERT_FILE names is the system bundle
    When SSL_CERT_FILE names a bundle that holds a test CA
    Then the system trust store holds that one certificate and names that file as its source

  Scenario: A certificate listed twice is trusted once
    When a bundle lists one CA, another CA, and the first again
    Then the trust store holds two certificates

  Scenario: A source without a usable certificate is refused
    When a bundle holds only a block that is no certificate
    Then reading it is refused as an empty trust store

  Scenario: A certificate a trusted CA issued for the host verifies
    When a client trusting the test CA connects to a server presenting the certificate it issued for `localhost`
    Then the handshake verifies and a line sent comes back from the server

  Scenario: A certificate from an untrusted CA is refused
    When a client trusting another CA connects to that server
    Then the connection is refused as a failed handshake naming an unknown issuer

  Scenario: A certificate issued for another name is refused
    When a client trusting the test CA connects to that server for `collector.example`
    Then the connection is refused as a failed handshake naming a certificate not valid for the name
