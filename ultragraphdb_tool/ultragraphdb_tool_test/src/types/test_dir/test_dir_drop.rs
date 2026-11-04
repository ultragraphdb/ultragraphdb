/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::test_dir::TestDir;

impl Drop for TestDir {
    fn drop(&mut self) {
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&self.0));
    }
}
