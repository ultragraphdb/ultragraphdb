/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::{
    CONTRACT_VERSION, ContractVersion, MappingError, MappingErrorEnum,
};

#[test]
fn test_the_version_round_trips_and_another_major_version_is_refused() {
    assert_eq!((CONTRACT_VERSION.major(), CONTRACT_VERSION.minor()), (1, 0));
    let v = ContractVersion::new(2, 7);
    assert_eq!(v.to_bytes(), [0, 2, 0, 7]);
    assert_eq!(ContractVersion::from_bytes(&v.to_bytes()), Ok(v));
    assert_eq!(
        ContractVersion::from_bytes(&[1, 2, 3]),
        Err(MappingError::BadContractVersion(&[1, 2, 3]))
    );
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

#[test]
fn test_the_version_is_two_u16s_big_endian() {
    // Major 0x0102 = 258 and minor 0x0304 = 772: the bytes 01 02 03 04, each u16 big-endian.
    let v = ContractVersion::new(258, 772);
    assert_eq!(v.to_bytes(), [0x01, 0x02, 0x03, 0x04]);
    assert_eq!(
        ContractVersion::from_bytes(&[0x01, 0x02, 0x03, 0x04]),
        Ok(ContractVersion::new(258, 772))
    );
    assert_eq!(
        ContractVersion::new(u16::MAX, 0).to_bytes(),
        [0xFF, 0xFF, 0, 0]
    );
}

#[test]
fn test_only_four_bytes_are_a_version() {
    for bytes in [&[][..], &[1], &[0, 1, 0], &[0, 1, 0, 0, 0]] {
        let error = ContractVersion::from_bytes(bytes).unwrap_err();
        assert_eq!(
            error.0,
            MappingErrorEnum::BadContractVersion {
                bytes: bytes.to_vec()
            },
            "{bytes:?}"
        );
    }
}

#[test]
fn test_an_older_major_version_is_refused_as_a_newer_one_is() {
    // The spec refuses "another major version", lower as well as higher; 0 is one below this build's 1.
    assert_eq!(
        CONTRACT_VERSION.check_stored(ContractVersion::new(0, 9)),
        Err(MappingError::ContractVersion((0, 9), (1, 0)))
    );
    // Another minor version of the same major version, older or newer, opens.
    assert_eq!(
        ContractVersion::new(1, 5).check_stored(ContractVersion::new(1, 0)),
        Ok(())
    );
    assert_eq!(
        ContractVersion::new(1, 5).check_stored(ContractVersion::new(1, u16::MAX)),
        Ok(())
    );
}
