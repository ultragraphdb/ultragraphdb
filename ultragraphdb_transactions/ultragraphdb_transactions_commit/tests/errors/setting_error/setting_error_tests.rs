/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::{SettingError, SettingErrorEnum};

#[test]
fn test_each_reason_names_the_setting_and_its_value() {
    let range = SettingError::close_request_range(0);
    assert_eq!(range.0, SettingErrorEnum::CloseRequestRange { value: 0 });
    assert_eq!(
        range.to_string(),
        "close_request_ms is 0, outside 1 to 60000"
    );
    let below = SettingError::close_request_below_epochs(500, 300);
    assert_eq!(
        below.0,
        SettingErrorEnum::CloseRequestBelowEpochs {
            value: 500,
            epoch_ms_max: 300
        }
    );
    assert_eq!(
        below.to_string(),
        "close_request_ms is 500, below twice epoch_ms_max (300)"
    );
}
