# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@both_shells
Feature: A real reply damaged on its way to the client
  The conformance suites flip every bit of every golden reply over a simulated network. Beyond the spec: a link between a real client and a real frontend flips one in the header and one in the body, of a reply the frontend really sent.

  Scenario: A damaged reply is refused by the connection's rule
    When a real reply is damaged on its way to the client
    Then the client refuses it by the connection's rule, never reading it as a value, and a commit so answered stays in doubt
