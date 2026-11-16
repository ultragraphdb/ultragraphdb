#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: System group: a client's proof is checked in the system group
  openspec/changes/stage-5-8-system-group/specs/system-group/spec.md, "A client's proof is checked in the system
  group": the parts the two halves of the exchange decide without a connection. `ScramRelay` reads and builds the
  client's messages and holds no key; `ScramVerifier::sign` checks the proof where the verifier is held; an unknown
  name's verifier derives from the group's secret. The replies are Challenge's and Verify's bodies (PROTOCOL.md
  §13.8, §13.9) as the group answers them from those halves; the session serving them is group 3's.

  Scenario: A proof that holds is answered with its signature
    When a Verify carries `ann`'s name, the exchange's AuthMessage and the proof `ann`'s password gives
    Then the reply holds `verified` 1, `ann`'s principal and a ServerSignature of 32 bytes

  Scenario: An unknown name and a wrong proof answer alike
    When one Verify carries a wrong proof for `ann` and another any proof for the unknown name `nobody`
    Then both replies hold `verified` 0, `principal` 0 and no ServerSignature

  Scenario: No frame carries a stored key
    When a Challenge and a Verify for `ann` are answered
    Then neither reply holds `ann`'s stored key or server key, and no other command the endpoint serves returns them
