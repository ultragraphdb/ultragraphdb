/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use compio::runtime::Runtime;
use ultragraphdb_common_config::{CONFIG_LOG_CLUSTER, ConfigStore, store_owner};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

/// The owner ends once its handle is gone.
#[test]
fn test_the_owner_ends_with_its_handle() -> TestResult {
    Runtime::new()?.block_on(async {
        let config = WalConfig::new(CONFIG_LOG_CLUSTER, false);
        let (log, _) = Wal::open(SimStorage::new(1), PathBuf::from("/wal"), config).await?;
        let (handle, owner) = store_owner(ConfigStore::recover(log).await?);
        drop(handle);
        owner.await;
        Ok(())
    })
}
