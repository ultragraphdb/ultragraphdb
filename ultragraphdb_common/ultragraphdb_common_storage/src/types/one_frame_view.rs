/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyView, Command, CommitOperation, ConditionalWriteView, DirectWriteView, FrameView,
    TermTableView, WriteChange,
};

/// A one-frame write's body, a DirectWrite's or a ConditionalWrite's, read alike (`PROTOCOL.md` §§6.6, 6.7).
#[derive(Debug)]
pub(crate) enum OneFrameView<'a> {
    /// A DirectWrite, which runs no condition.
    Direct(DirectWriteView<'a>),
    /// A ConditionalWrite, its operations decided in commit order.
    Conditional(ConditionalWriteView<'a>),
}

impl<'a> OneFrameView<'a> {
    /// The body of `frame`, a DirectWrite or a ConditionalWrite.
    pub(crate) fn of(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        if frame.command() == Command::DirectWrite {
            Ok(Self::Direct(DirectWriteView::from_frame(frame)?))
        } else {
            Ok(Self::Conditional(ConditionalWriteView::from_frame(frame)?))
        }
    }

    pub(crate) fn session(&self) -> u128 {
        u128::from_be_bytes(match self {
            Self::Direct(view) => view.session(),
            Self::Conditional(view) => view.session(),
        })
    }

    pub(crate) fn request(&self) -> u64 {
        match self {
            Self::Direct(view) => view.request(),
            Self::Conditional(view) => view.request(),
        }
    }

    pub(crate) fn tenant(&self) -> u64 {
        match self {
            Self::Direct(view) => view.tenant(),
            Self::Conditional(view) => view.tenant(),
        }
    }

    pub(crate) fn principal(&self) -> u64 {
        match self {
            Self::Direct(view) => view.principal(),
            Self::Conditional(view) => view.principal(),
        }
    }

    /// The epoch its token raises the commit to, epoch 0 for none.
    pub(crate) fn epoch(&self) -> Epoch {
        let token: Option<&SessionToken> = match self {
            Self::Direct(view) => view.token(),
            Self::Conditional(view) => view.token(),
        };
        token.map_or(Epoch::ZERO, SessionToken::epoch)
    }

    pub(crate) fn terms(&self) -> &TermTableView<'a> {
        match self {
            Self::Direct(view) => view.terms(),
            Self::Conditional(view) => view.terms(),
        }
    }

    pub(crate) fn change_count(&self) -> usize {
        match self {
            Self::Direct(view) => view.len(),
            Self::Conditional(view) => view.change_count(),
        }
    }

    pub(crate) fn change(&self, index: usize) -> Option<WriteChange> {
        match self {
            Self::Direct(view) => view.change(index),
            Self::Conditional(view) => view.change(index),
        }
    }

    pub(crate) fn operation_count(&self) -> usize {
        match self {
            Self::Direct(_) => 0,
            Self::Conditional(view) => view.operation_count(),
        }
    }

    pub(crate) fn operation(&self, index: usize) -> Option<CommitOperation> {
        match self {
            Self::Direct(_) => None,
            Self::Conditional(view) => view.operation(index),
        }
    }

    /// Whether it refuses a unique predicate, which only a ConditionalWrite's operation claims.
    pub(crate) fn is_direct(&self) -> bool {
        matches!(self, Self::Direct(_))
    }
}
