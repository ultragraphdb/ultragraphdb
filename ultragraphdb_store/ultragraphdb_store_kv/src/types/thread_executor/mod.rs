/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod thread_executor_executor;

/// The production [`ultragraphdb_traits_store::Executor`]: each spawned task on a thread of its own, and the tasks of
/// [`ultragraphdb_traits_store::Executor::run_all`] on scoped threads, one each, joined in order. The simulator steps
/// the same work on its stepped executor instead.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThreadExecutor;

impl ThreadExecutor {
    pub fn new() -> Self {
        Self
    }
}
