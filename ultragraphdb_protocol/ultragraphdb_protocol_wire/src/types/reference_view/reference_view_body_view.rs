/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::as_u32;
use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::reference::Reference;
use crate::types::reference_mode::ReferenceMode;
use crate::types::reference_registration::{REGISTRATION_SIZE, ReferenceRegistration};
use crate::types::reference_status::ReferenceStatus;
use crate::types::reference_view::ReferenceView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::body::{read_sections, records};
use crate::utils::command_header::expect_command;
use crate::utils::constants::MAX_EPOCH;
use crate::utils::fields::boolean;
use crate::utils::term_kind::term_kind;

impl<'a> BodyView<'a> for ReferenceView<'a> {
    const COMMAND: Command = Command::Reference;

    type Body = Reference;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 2), (8, 16)])?;
        let reply = boolean(command, "reply", header.u8_at(0))?;
        let mode = ReferenceMode::from_number(header.u8_at(1))
            .ok_or_else(|| ProtocolError::field_value(command, "mode", header.u8_at(1).into()))?;
        let tenant = header.u64_at(8);
        let sections = read_sections::<3>(command, frame.body(), 0)?;
        let terms = TermTableView::read(
            command,
            (1, 2),
            sections.offset(1),
            sections.get(1),
            sections.get(2),
        )?;
        if let Some(other) = (0..as_u32(terms.len()))
            .filter_map(|index| terms.get(index))
            .find(|term| !matches!(term, RequestTermView::Iri(_)))
        {
            return Err(ProtocolError::field_value(
                command,
                "reference term",
                term_kind(&other).into(),
            ));
        }
        let registrations = sections.get(0);
        let whole = records(command, 0, registrations, REGISTRATION_SIZE)?;
        if registrations.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "registrations",
                0,
                1,
            ));
        }
        for (index, record) in whole.enumerate() {
            let at = sections.offset(0) + REGISTRATION_SIZE * index;
            check_registration(command, (reply, mode), at, index, record, terms.len())?;
        }
        Ok(Self {
            tenant,
            reply,
            mode,
            registrations,
            terms,
        })
    }

    fn to_body(&self) -> Result<Reference, ProtocolError> {
        Ok(Reference::new(
            self.reply,
            self.registrations().collect(),
            self.terms.to_table(),
        )
        .with_mode(self.mode)
        .in_tenant(self.tenant))
    }
}

/// Registration `index`'s bytes `record`, at body offset `at`, checked: its zero byte
/// ([`ProtocolError::padding_not_zero`]), then its fields as a request's or, with `reply`, a reply's, in the batch's
/// mode, over a term table of `terms` entries ([`ProtocolError::field_value`] `"registration"`, the index).
fn check_registration(
    command: u16,
    (reply, mode): (bool, ReferenceMode),
    at: usize,
    index: usize,
    record: &[u8],
    terms: usize,
) -> Result<(), ProtocolError> {
    if record.get(15).is_some_and(|zero| *zero != 0) {
        return Err(ProtocolError::padding_not_zero(command, at + 15));
    }
    // Lossless: a `usize` is at most 64 bits.
    let refused = ProtocolError::field_value(command, "registration", index as u64);
    let bytes: [u8; REGISTRATION_SIZE] = record.try_into().map_err(|_| refused.clone())?;
    let registration = ReferenceRegistration::decode(&bytes).ok_or_else(|| refused.clone())?;
    if fields_hold(reply, mode, &registration, terms) {
        Ok(())
    } else {
        Err(refused)
    }
}

/// Whether `registration`'s fields hold their rules over a term table of `terms` entries, as a request's or, with
/// `reply`, as a reply's, in `mode`.
fn fields_hold(
    reply: bool,
    mode: ReferenceMode,
    registration: &ReferenceRegistration,
    terms: usize,
) -> bool {
    let names_iri = registration.iri().is_some();
    let one_way = names_iri == registration.entity().is_none();
    let common = registration.partition() >= 1
        && registration.iri().is_none_or(|iri| (iri as usize) < terms) // lossless: a `usize` is at least 32 bits
        && registration.close() <= MAX_EPOCH
        && (registration.group().is_some() || registration.generation() == 0);
    let unanswered = registration.status_byte() == 0;
    let directed = match (reply, registration.outcome()) {
        (false, outcome) => outcome == 0 && unanswered && registration.close() == 0 && one_way,
        (true, 0) => accepted_holds(mode, registration),
        (true, outcome) => {
            refusal_of(mode, outcome)
                && unanswered
                && registration.generation() == 0
                && registration.close() == 0
                && one_way
        }
    };
    common && directed
}

/// Whether `outcome` refuses a registration in `mode`: 7, 17, 26 or 30 with mode 0; 17 or 26 get-only.
fn refusal_of(mode: ReferenceMode, outcome: u16) -> bool {
    match mode {
        ReferenceMode::GetOrCreate => matches!(outcome, 7 | 17 | 26 | 30),
        ReferenceMode::GetOnly => matches!(outcome, 17 | 26),
    }
}

/// Whether an accepted `registration`'s fields hold in `mode`: with mode 0 no status, the entity's ID, and a member's
/// generation at least 1; get-only, a status of 1 to 4, an absent entity's ID all zero, a live one's as with mode 0,
/// and a deleted or retracted one's ID with `generation` and `close` 0.
fn accepted_holds(mode: ReferenceMode, registration: &ReferenceRegistration) -> bool {
    let member = registration.group().is_none() || registration.generation() >= 1;
    let unrecorded = registration.generation() == 0 && registration.close() == 0;
    let found = registration.entity().is_some();
    match mode {
        ReferenceMode::GetOrCreate => registration.status_byte() == 0 && found && member,
        ReferenceMode::GetOnly => match ReferenceStatus::from_number(registration.status_byte()) {
            Some(ReferenceStatus::Absent) => !found && unrecorded,
            Some(ReferenceStatus::Live) => found && member,
            Some(ReferenceStatus::Deleted | ReferenceStatus::Retracted) => found && unrecorded,
            None => false,
        },
    }
}
