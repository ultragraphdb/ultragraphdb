/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::SinkExt;
use futures::channel::mpsc::Sender;
use futures::channel::oneshot;

use crate::types::config_command::ConfigCommand;

/// The API's way to the task owning the store. A clone is one more sender of the same channel.
#[derive(Debug, Clone)]
pub struct ConfigHandle {
    pub(crate) commands: Sender<ConfigCommand>,
}

impl ConfigHandle {
    /// Asks the owner what `command`, given its reply's sender, asks: `None` when the owner has stopped. A future dropped
    /// while the command waits for room asks nothing; one dropped at the answer leaves the command to run, a change
    /// committed included, and its answer lost.
    pub(crate) async fn ask<T>(
        &self,
        command: impl FnOnce(oneshot::Sender<T>) -> ConfigCommand,
    ) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        let mut commands = self.commands.clone();
        commands.send(command(reply)).await.ok()?;
        answer.await.ok()
    }
}
