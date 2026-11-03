# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Server TLS and the TLS session
  A server accepts TLS 1.3 with its configured certificate, and one TLS session drives over any carrier
  (openspec/changes/stage-5-5-three-role-refactor/specs/transport-tls/spec.md).

  Scenario: A client trusting the CA completes the handshake
    When a client trusting the test CA connects to an acceptor holding the certificate the CA issued for `localhost`
    Then the handshake completes with TLS 1.3 and a line sent comes back from the server

  Scenario: A TLS 1.2 client is refused
    When a client offering TLS 1.2 alone connects to the acceptor
    Then the handshake fails as a protocol-version refusal, and the caller reads no byte

  Scenario: A mismatched key is refused at construction
    When an acceptor is built from the test CA's server certificate and another key
    Then construction fails with `KeyMismatch`

  Scenario: The same exchange over two carriers
    When a client and a server complete a handshake and exchange one line through the session over an in-memory pipe, and again over a TCP socket
    Then both exchanges succeed, and the session's code is the same in both
