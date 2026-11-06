/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ConditionalWrite, DirectWrite, DropCommand};

/// What a commit, a drop or a one-frame write sends again through its session (`PROTOCOL.md` §11.5): a Commit names only
/// its session and request, which the frontend recalls, while a Drop's and a one-frame write's frame holds what it does,
/// so it goes as its first did. Boxed, since a request in doubt is rare and a pending request frequent.
#[derive(Debug, Clone)]
pub(crate) enum Resend {
    Commit,
    Drop(Box<DropCommand>),
    Direct(Box<DirectWrite>),
    Conditional(Box<ConditionalWrite>),
}
