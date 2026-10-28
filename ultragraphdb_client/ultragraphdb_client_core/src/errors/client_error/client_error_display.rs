/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::client_error::{ClientError, ClientErrorEnum};

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ClientErrorEnum::Transport { error } => write!(f, "connection failed: {error}"),
            ClientErrorEnum::Protocol { error } => {
                write!(f, "the server sent a frame refused: {error}")
            }
            ClientErrorEnum::Refused {
                code,
                detail,
                message,
                ..
            } => {
                write!(f, "refused ({code:?}, detail {detail}): {message}")
            }
            ClientErrorEnum::Closed { code, message } => {
                write!(f, "the server closed the connection ({code:?}): {message}")
            }
            ClientErrorEnum::TimedOut => write!(f, "no answer within the timeout"),
            ClientErrorEnum::ConnectionLost => write!(f, "the request's connection was lost"),
            ClientErrorEnum::NotConnected => write!(f, "no connection is ready"),
            ClientErrorEnum::Request { error } => write!(f, "the request cannot be sent: {error}"),
            ClientErrorEnum::ReplyMismatch {
                field,
                expected,
                found,
            } => {
                write!(
                    f,
                    "a reply names {field} {found} where {expected} was asked"
                )
            }
            ClientErrorEnum::RetriesExhausted {
                session,
                request,
                attempts,
            } => write!(
                f,
                "commit {request} of session {session} unanswered after {attempts} attempts: in doubt"
            ),
            ClientErrorEnum::InDoubt { session, request } => {
                write!(f, "session {session} has commit {request} in doubt")
            }
            ClientErrorEnum::NotInDoubt { session } => {
                write!(f, "session {session} has no commit in doubt")
            }
            ClientErrorEnum::SessionBusy { session } => {
                write!(f, "session {session} already has a commit in flight")
            }
            ClientErrorEnum::UnknownSession { session } => write!(f, "no session {session}"),
            ClientErrorEnum::ReadOnly { stream } => {
                write!(f, "the transaction on stream {stream} is read-only")
            }
            ClientErrorEnum::StreamFinished { stream } => write!(f, "stream {stream} is not open"),
            ClientErrorEnum::StreamBusy { stream } => {
                write!(f, "stream {stream} still waits for its answer")
            }
            ClientErrorEnum::StreamLimit { limit } => {
                write!(
                    f,
                    "the connection holds {limit} streams, as many as allowed"
                )
            }
        }
    }
}
