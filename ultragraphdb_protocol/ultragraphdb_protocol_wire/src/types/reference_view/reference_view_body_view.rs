/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::traits::body_view::BodyView;
use crate::types::command::Command;
use crate::types::frame_view::FrameView;
use crate::types::reference::Reference;
use crate::types::reference_registration::{REGISTRATION_SIZE, ReferenceRegistration};
use crate::types::reference_view::ReferenceView;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::constants::MAX_EPOCH;
use crate::utils::fields::boolean;
use crate::utils::sections::{Sections, as_u32, expect_command, records};
use crate::utils::term_kind::term_kind;

impl<'a> BodyView<'a> for ReferenceView<'a> {
    const COMMAND: Command = Command::Reference;

    type Body = Reference;

    fn from_frame(frame: &FrameView<'a>) -> Result<Self, ProtocolError> {
        let command = Self::COMMAND.number();
        let header = expect_command(frame, Self::COMMAND)?;
        header.unused_zero(&[(0, 1), (8, 16)])?;
        let reply = boolean(command, "reply", header.u8_at(0))?;
        let tenant = header.u64_at(8);
        let sections = Sections::<3>::read(command, frame.body(), 0)?;
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
            check_registration(command, reply, at, index, record, terms.len())?;
        }
        Ok(Self {
            tenant,
            reply,
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
        .in_tenant(self.tenant))
    }
}

/// Registration `index`'s bytes `record`, at body offset `at`, checked: its zero bytes
/// ([`ProtocolError::padding_not_zero`]), then its fields as a request's or, with `reply`, a reply's, over a term
/// table of `terms` entries ([`ProtocolError::field_value`] `"registration"`, the index).
fn check_registration(
    command: u16,
    reply: bool,
    at: usize,
    index: usize,
    record: &[u8],
    terms: usize,
) -> Result<(), ProtocolError> {
    if let Some(zero) = record
        .get(14..16)
        .and_then(|zero| zero.iter().position(|b| *b != 0))
    {
        return Err(ProtocolError::padding_not_zero(command, at + 14 + zero));
    }
    let refused = ProtocolError::field_value(command, "registration", index as u64);
    let bytes: [u8; REGISTRATION_SIZE] = record.try_into().map_err(|_| refused.clone())?;
    let registration = ReferenceRegistration::decode(&bytes).ok_or_else(|| refused.clone())?;
    if fields_hold(reply, &registration, terms) {
        Ok(())
    } else {
        Err(refused)
    }
}

/// Whether `registration`'s fields hold their rules over a term table of `terms` entries, as a request's or, with
/// `reply`, as a reply's.
fn fields_hold(reply: bool, registration: &ReferenceRegistration, terms: usize) -> bool {
    let names_iri = registration.iri().is_some();
    let one_way = names_iri == registration.entity().is_none();
    let common = registration.partition() >= 1
        && registration.iri().is_none_or(|iri| (iri as usize) < terms)
        && registration.close() <= MAX_EPOCH
        && (registration.group().is_some() || registration.generation() == 0);
    let directed = match (reply, registration.outcome()) {
        (false, outcome) => outcome == 0 && registration.close() == 0 && one_way,
        (true, 0) => {
            registration.entity().is_some()
                && (registration.group().is_none() || registration.generation() >= 1)
        }
        (true, outcome) => {
            matches!(outcome, 7 | 26)
                && registration.generation() == 0
                && registration.close() == 0
                && one_way
        }
    };
    common && directed
}
