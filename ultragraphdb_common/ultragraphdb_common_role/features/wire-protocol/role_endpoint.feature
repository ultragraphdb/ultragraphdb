#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: what a role connection announces
  A role connection carries what the frontend already bounded on its clients' connections, streams and frames, so a role
  endpoint announces the protocol's largest frame and its most streams, and a role takes the largest frame in its Hello.

  Scenario: A role endpoint announces its limits
    When a role connects to a role endpoint
    Then the Welcome names `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and the role's Hello named `max_frame` 1,048,576
