/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

/// A session's request in flight (`PROTOCOL.md` §11.5): the role connection whose Check began it, `None` for a check the
/// frontend's endpoint asked in process, and whether the request may record the session in a partition's table, which keeps
/// a full registry from evicting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    connection: Option<u64>,
    writes: bool,
}

/// The marks of the sessions with a request in flight, held per role connection, and the group's quiet time after it
/// begins to listen (`PROTOCOL.md` §11.5). Held in memory alone: a restart loses every mark, which the quiet time stands
/// for. Every time is the group's monotonic clock in microseconds.
///
/// Bounds (Ultra rule 2): a mark is held per session, so at most one per session the group was asked about, and a closed
/// connection is held only while a mark of it is, at most twice `transaction_ms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionMarks {
    marks: BTreeMap<u128, Mark>,
    /// Each closed connection holding a mark, by the instant it closed.
    closed: BTreeMap<u64, u64>,
    /// The number the next connection takes, from 1.
    next: u64,
    /// Twice `transaction_ms`, in microseconds.
    hold_us: u64,
    /// The end of the quiet time, exclusive.
    quiet_until_us: u64,
}

impl SessionMarks {
    /// No mark, for a group whose transactions live at most `transaction_ms`.
    pub(crate) fn new(transaction_ms: u64) -> Self {
        Self {
            marks: BTreeMap::new(),
            closed: BTreeMap::new(),
            next: 1,
            hold_us: transaction_ms.saturating_mul(2_000),
            quiet_until_us: 0,
        }
    }

    /// The group begins to listen at `now_us`: it is quiet until twice `transaction_ms` later, that instant excluded.
    pub(crate) fn listen(&mut self, now_us: u64) {
        self.quiet_until_us = now_us.saturating_add(self.hold_us);
    }

    /// Whether `now_us` lies in the quiet time, the half-open interval from the instant the group began to listen.
    pub(crate) fn is_quiet(&self, now_us: u64) -> bool {
        now_us < self.quiet_until_us
    }

    /// A number for a new connection.
    pub(crate) fn open(&mut self) -> u64 {
        let number = self.next;
        self.next = self.next.saturating_add(1);
        number
    }

    /// `connection` closed at `now_us`: its marks end twice `transaction_ms` later.
    pub(crate) fn close(&mut self, connection: u64, now_us: u64) {
        if self
            .marks
            .values()
            .any(|mark| mark.connection == Some(connection))
        {
            let _first = self.closed.entry(connection).or_insert(now_us);
        }
    }

    /// Ends every mark of a connection closed for twice `transaction_ms` at `now_us`, and forgets each closed connection
    /// holding no mark then.
    pub(crate) fn expire(&mut self, now_us: u64) {
        let hold_us = self.hold_us;
        let gone: Vec<u64> = self
            .closed
            .iter()
            .filter(|(_, closed_us)| now_us >= closed_us.saturating_add(hold_us))
            .map(|(connection, _)| *connection)
            .collect();
        for connection in gone {
            let _closed = self.closed.remove(&connection);
            self.marks
                .retain(|_, mark| mark.connection != Some(connection));
        }
    }

    /// Whether `session` has a request in flight.
    pub(crate) fn is_marked(&self, session: u128) -> bool {
        self.marks.contains_key(&session)
    }

    /// Whether a full registry may not evict `session`: its request in flight may record it in a partition's table.
    pub(crate) fn protects(&self, session: u128) -> bool {
        self.marks.get(&session).is_some_and(|mark| mark.writes)
    }

    /// Marks `session` in flight, against `connection`.
    pub(crate) fn mark(&mut self, session: u128, connection: Option<u64>, writes: bool) {
        let _before = self.marks.insert(session, Mark { connection, writes });
    }

    /// An Ended on `connection` naming `session`: the mark ends when it was begun on `connection` or on a connection that
    /// closed, and is passed over otherwise.
    pub(crate) fn end(&mut self, session: u128, connection: u64) {
        let ends = self.marks.get(&session).is_some_and(|mark| {
            mark.connection == Some(connection)
                || mark
                    .connection
                    .is_some_and(|began| self.closed.contains_key(&began))
        });
        if ends {
            let _ended = self.marks.remove(&session);
        }
    }

    /// The request in flight through `session` that the frontend's endpoint checked in process ended: its mark ends,
    /// whichever connection began it.
    pub(crate) fn end_in_process(&mut self, session: u128) {
        let _ended = self.marks.remove(&session);
    }
}
