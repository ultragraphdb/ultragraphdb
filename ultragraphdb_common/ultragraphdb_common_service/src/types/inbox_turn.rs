/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What one turn of a core may still take of the messages waiting in its inbox: at most the bound it was made with, the rest
/// waiting for the next turn, so a turn ends however fast its senders send (Ultra rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboxTurn {
    left: usize,
}

impl InboxTurn {
    /// A turn taking at most `messages_max` waiting messages.
    #[must_use]
    pub const fn new(messages_max: usize) -> Self {
        Self { left: messages_max }
    }

    /// The next message `receive` gives, while the turn may take one; `None` once it has taken its bound, `receive` then left
    /// uncalled, or once the inbox gives none. An empty or closed inbox ends the turn's taking alike: the core's own wait for its
    /// next message sees a closed one.
    pub fn receive<T, E>(&mut self, receive: impl FnOnce() -> Result<T, E>) -> Option<T> {
        if self.left == 0 {
            return None;
        }
        match receive() {
            Ok(message) => {
                self.left -= 1;
                Some(message)
            }
            Err(_) => None,
        }
    }
}
