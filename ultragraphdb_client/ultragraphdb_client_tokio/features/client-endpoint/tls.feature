#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The client endpoint speaks TLS 1.3 when configured
  The client's side: the shell connects with TLS when it names a trust source, verifying the server's certificate before
  the wire protocol's handshake begins. The test server, behind an acceptor over a TCP loopback, plays the frontend's
  endpoint; the frontend's own start is its crate's.

  Scenario: A TLS client reaches the endpoint
    When the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates
    Then the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed

  Scenario: A clear-text client is refused by a TLS endpoint
    When a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS
    Then the connection fails as a TLS handshake failure, and no frame of the wire protocol is read
