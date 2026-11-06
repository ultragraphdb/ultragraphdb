# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The registry's side of a session
  The frontend's owner check, which refuses a request before any partition sees it.

  Scenario: A session belongs to its principal
    When principal `alice` registers a session, and a connection authenticated as `bob` commits through it
    Then the commit is refused with code 21, `detail` 2, and `alice`'s next request through the session is served
