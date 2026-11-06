/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::HeldCatalogs;
use ultragraphdb_traits_catalog::HeldVersion;

#[test]
fn test_up_to_gives_a_tenants_versions_in_effect_by_an_epoch_in_order() {
    let mut held = HeldCatalogs::default();
    for (tenant, version, effective) in [(1, 2, 8), (1, 1, 5), (1, 3, 9), (2, 1, 1)] {
        held.hold(HeldVersion::new(tenant, version, effective, Vec::new()));
    }
    let versions = |tenant, epoch| -> Vec<u64> {
        held.up_to(tenant, epoch)
            .map(HeldVersion::version)
            .collect()
    };
    assert_eq!(versions(1, 8), [1, 2], "in order, the later one left out");
    assert_eq!(versions(1, 4), Vec::<u64>::new());
    assert_eq!(versions(2, 9), [1], "another tenant's are its own");
}
