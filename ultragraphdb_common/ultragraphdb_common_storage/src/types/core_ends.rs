/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::channel::oneshot;
use ultragraphdb_error_role::RoleError;

/// How the threads of a role's partitions ended: one receiver for each not yet heard from. A receiver gives its outcome once, and
/// answers as a thread gone without a word afterwards, so one heard from is let go.
#[derive(Debug, Default)]
pub struct CoreEnds {
    waiting: Vec<oneshot::Receiver<Result<(), RoleError>>>,
}

impl CoreEnds {
    /// Waits on one more thread, which reports through `ended`.
    pub fn add(&mut self, ended: oneshot::Receiver<Result<(), RoleError>>) {
        self.waiting.push(ended);
    }

    /// Whether a thread has ended cleanly since the last look.
    ///
    /// # Errors
    ///
    /// The first failure among the threads that have ended; [`RoleError::software`] for a thread gone without a word, which
    /// panicked.
    pub fn look(&mut self) -> Result<bool, RoleError> {
        let mut clean = false;
        let mut failure = None;
        self.waiting.retain_mut(|one| match one.try_recv() {
            Ok(None) => true,
            Ok(Some(Ok(()))) => {
                clean = true;
                false
            }
            Ok(Some(Err(error))) => {
                let _first = failure.get_or_insert(error);
                false
            }
            Err(_) => {
                let _first = failure.get_or_insert_with(silent);
                false
            }
        });
        failure.map_or(Ok(clean), Err)
    }

    /// Waits for every thread still running to end, each past its flush. A dropped future lets go of the receivers left,
    /// and the threads run on to their ends unheard.
    ///
    /// # Errors
    ///
    /// The first failure among them, once all have ended.
    pub async fn finish(self) -> Result<(), RoleError> {
        let mut first = Ok(());
        for one in self.waiting {
            let outcome = one.await.unwrap_or_else(|_| Err(silent()));
            if first.is_ok() {
                first = outcome;
            }
        }
        first
    }
}

fn silent() -> RoleError {
    RoleError::software("a partition's thread ended without a word")
}
