#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Telemetry
  A binary installs the subscriber once, from a configuration it passes: a level directive, plain or JSON lines, and,
  when an OTLP endpoint is given, spans exported through a batch processor that the guard flushes when dropped.

  Scenario: JSON lines carry the event's fields
    When telemetry is installed for JSON output at level `info` and an `info` event with a field `key` is emitted
    Then one JSON line holds the level, the target, the message and `key`, and a `debug` event emits nothing

  Scenario: Spans reach the exporter with their parent
    When telemetry is installed with an exporter and a span is entered inside another
    Then the exporter receives both spans, the inner one naming the outer as its parent
