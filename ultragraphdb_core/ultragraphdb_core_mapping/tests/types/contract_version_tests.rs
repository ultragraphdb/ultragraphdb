/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::{CONTRACT_VERSION, ContractVersion, MappingErrorEnum};

#[test]
fn test_the_version_round_trips_and_another_major_version_is_refused() {
    assert_eq!((CONTRACT_VERSION.major(), CONTRACT_VERSION.minor()), (1, 0));
    let v = ContractVersion::new(2, 7);
    assert_eq!(v.to_bytes(), [0, 2, 0, 7]);
    assert_eq!(ContractVersion::from_bytes(&v.to_bytes()), Ok(v));
    assert!(ContractVersion::from_bytes(&[1, 2, 3]).is_err());
    assert_eq!(
        CONTRACT_VERSION.check_stored(ContractVersion::new(1, 9)),
        Ok(())
    );
    let error = CONTRACT_VERSION.check_stored(v).unwrap_err();
    assert!(matches!(
        error.0,
        MappingErrorEnum::ContractVersion {
            stored: (2, 7),
            implemented: (1, 0)
        }
    ));
    assert_eq!(
        error.to_string(),
        "The store was written under mapping contract version 2.7; this build implements version 1.0"
    );
}
