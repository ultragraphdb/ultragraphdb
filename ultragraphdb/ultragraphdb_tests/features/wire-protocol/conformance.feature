# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@core
Feature: The client's conformance suite against the binaries
  The conversation ultragraphdb_client_compio's and ultragraphdb_client_tokio's conformance suites play against the golden frames of ultragraphdb_protocol_wire, here sent to the real frontend, compute and storage. Beyond the spec, which its scenarios bind one at a time.

  Scenario: The golden conversation crosses the binaries
    When the client's conformance conversation goes to the three binaries
    Then each golden request earns what the protocol says of it, and each reply is read as the protocol's values
