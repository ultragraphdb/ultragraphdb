/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_bus::NetworkFaults;

#[test]
fn test_faults_keep_their_rates() {
    let none = NetworkFaults::none();
    assert_eq!(
        (
            none.drop_rate(),
            none.duplicate_rate(),
            none.delay_rate(),
            none.split_rate(),
            none.max_delay()
        ),
        (0, 0, 0, 0, 0)
    );
    assert_eq!(none, NetworkFaults::default());
    let faults = NetworkFaults::none()
        .with_drop(1)
        .with_duplicate(2)
        .with_delay(3, 4)
        .with_split(5);
    assert_eq!(
        (
            faults.drop_rate(),
            faults.duplicate_rate(),
            faults.delay_rate(),
            faults.split_rate(),
            faults.max_delay()
        ),
        (1, 2, 3, 5, 4)
    );
}
