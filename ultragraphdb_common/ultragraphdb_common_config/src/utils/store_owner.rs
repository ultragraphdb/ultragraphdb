/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc;
use ultragraphdb_traits_log::OrderedLog;

use crate::types::config_command::ConfigCommand;
use crate::types::config_handle::ConfigHandle;
use crate::types::config_store::ConfigStore;

/// How many commands wait for the owner: an administrator's and the roles' starts are few.
const WAITING: usize = 64;

/// A handle to `store` and the task owning it, which runs until every handle is gone. One command runs at a time, so a
/// revision's check of the latest and its append are one step. A caller gone before the answer loses nothing but the answer.
/// The owner awaits each command to its end, so no write of the store is dropped at its sync.
pub fn store_owner<L: OrderedLog>(
    mut store: ConfigStore<L>,
) -> (ConfigHandle, impl Future<Output = ()>) {
    let (commands, mut waiting) = mpsc::channel(WAITING);
    let owner = async move {
        while let Some(command) = waiting.next().await {
            // A caller gone before its answer loses only the answer, so each reply's send drops its `Result`.
            match command {
                ConfigCommand::Targets(reply) => drop(reply.send(store.targets())),
                ConfigCommand::Instance {
                    target,
                    revision,
                    instance,
                    reply,
                } => {
                    let answer = store
                        .instance(&target, revision, &instance)
                        .map(|(number, rendered)| (number, rendered.to_string()));
                    drop(reply.send(answer));
                }
                ConfigCommand::Files {
                    target,
                    revision,
                    reply,
                } => drop(reply.send(store.files(&target, revision).map(<[u8]>::to_vec))),
                ConfigCommand::Download { target, reply } => {
                    let answer = store.active_revision(&target).and_then(|number| {
                        store
                            .files(&target, number)
                            .map(|files| (number, files.to_vec()))
                    });
                    drop(reply.send(answer));
                }
                ConfigCommand::Add {
                    target,
                    base,
                    body,
                    reply,
                } => drop(reply.send(store.add_revision(&target, base, &body).await)),
                ConfigCommand::Activate {
                    target,
                    revision,
                    reply,
                } => drop(reply.send(store.activate(&target, revision).await)),
                ConfigCommand::Retire { target, reply } => {
                    drop(reply.send(store.retire(&target).await));
                }
            }
        }
    };
    (ConfigHandle { commands }, owner)
}
