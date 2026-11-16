#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Client endpoint: connections authenticate against the metadata group's principals
  openspec/changes/stage-5-8-system-group/specs/client-endpoint/spec.md, "Connections authenticate against the
  metadata group's principals": the parts the two halves of the exchange decide without a connection. A client's
  exchange runs through `ScramRelay`, the frontend's half, and the system group's half on `ScramVerifier`, across
  Challenge and Verify. A data request before authentication and the throttle need a connection, and are group 5's.

  Scenario: A wrong password is refused without detail
    When a client authenticates as `ann` with a wrong password, and as an unknown name
    Then both are refused with the same authentication error

  Scenario: The frontend never holds a stored key
    When a client authenticates as `ann` and the frames the frontend receives from the system group are read
    Then they hold `ann`'s salt, iteration count, principal and the exchange's ServerSignature, and neither her stored key nor her server key

  Scenario: An unknown name answers as a known one does
    When a client asks to authenticate as an unknown name, twice, across a restart of the frontend
    Then both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password
