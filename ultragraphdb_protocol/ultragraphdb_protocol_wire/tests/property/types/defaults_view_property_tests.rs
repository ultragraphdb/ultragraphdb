/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Defaults (`PROTOCOL.md` §13.28): `tenant` u64 at 0, `position` u64 at 8, and a term table of the predicates holding
//! a record. Order: unused bytes 16–127 zero; sections; the term table; in a request, no entry; in a reply, each entry
//! of kind 1 and above the one before by its text's bytes.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, Defaults, DefaultsView};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, count, field, frame, iri_table, iris,
    nonzero_byte, outcome, read_back, reserved, section_length, sections, term_table, viewed,
    within,
};

/// The Defaults frame of `tenant`, `position` and the term table `table`, laid out by hand, `unused` a command-header
/// byte set outside the fields.
pub fn laid_out(
    tenant: u64,
    position: u64,
    unused: Option<(usize, u8)>,
    table: &[Vec<u8>; 2],
    flags: u8,
) -> Option<Vec<u8>> {
    let mut header = command_header(&[(0, &tenant.to_le_bytes()), (8, &position.to_le_bytes())]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    frame(Command::Defaults, header, &sections(table), flags)
}

/// The predicates a view reads from `bytes`, with its fields.
fn read(bytes: &[u8]) -> Result<(u64, u64, Vec<String>), ProtocolErrorEnum> {
    viewed::<DefaultsView, _>(bytes, |view| {
        let predicates = (0..view.len())
            .filter_map(|index| view.predicate(index))
            .map(str::to_string)
            .collect();
        (view.tenant(), view.position(), predicates)
    })
}

/// L1.8: a Defaults request, and a reply of 0 to 32 drawn IRIs in the order of their bytes, is the bytes laid out by
/// hand, reads back to its fields and builds again to the same bytes.
#[hegel::test]
fn test_a_defaults_round_trips(tc: TestCase) {
    let tenant = tc.draw(gs::integers::<u64>());
    let reply = tc.draw(gs::booleans());
    let (position, predicates) = if reply {
        (
            tc.draw(gs::integers::<u64>().min_value(1)),
            iris(&tc, 0, 32),
        )
    } else {
        (0, Vec::new())
    };
    let expected = laid_out(tenant, position, None, &iri_table(&predicates), 0);
    let body = if reply {
        Defaults::reply(tenant, position, predicates.clone())
    } else {
        Defaults::request(tenant)
    };
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(read(&bytes), Ok((tenant, position, predicates)));
    let again = read_back::<DefaultsView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// `predicates` as term-table entries of kind 1.
fn entries(predicates: &[String]) -> Vec<(u8, &str, &str)> {
    predicates.iter().map(|iri| (1, iri.as_str(), "")).collect()
}

/// L1.18: a Defaults with one drawn rule broken is refused with that rule's refusal: an unused byte, an entry in a
/// request, a reply's entry that is no IRI, and an entry at or below the one before.
#[hegel::test]
fn test_a_defaults_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let predicates = iris(&tc, 2, 12);
    let mut table = entries(&predicates);
    let index = within(&tc, 1, predicates.len() - 1);
    let (position, unused, refusal) = match within(&tc, 0, 4) {
        0 => {
            let at = within(&tc, 16, 127);
            (1, Some((at, nonzero_byte(&tc))), reserved(at))
        }
        1 => (0, None, count(539, "defaults", predicates.len() as u64, 0)),
        2 => {
            let (kind, second) = tc.draw(gs::sampled_from(vec![(2u8, ""), (3, ""), (4, "en")]));
            let at = within(&tc, 0, predicates.len() - 1);
            table[at] = (kind, table[at].1, second);
            (1, None, field(539, "default", kind.into()))
        }
        3 => {
            table.swap(index - 1, index);
            (1, None, field(539, "default order", index as u64))
        }
        _ => {
            // An entry equal to the one before is the term table's own refusal, checked first (§4.4, step 9).
            table[index] = table[index - 1];
            let refusal = ProtocolErrorEnum::DuplicateTerm {
                command: 539,
                section: 0,
                at: index,
                earlier: index - 1,
            };
            (1, None, refusal)
        }
    };
    let laid = laid_out(3, position, unused, &term_table(&table), 0);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<DefaultsView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}

/// L1.22: with two rules of the Order line broken in a reply, the refusal is that of the earlier one: an unused byte,
/// the term table's records a whole number of 24 bytes (§4.4, step 0), an entry that is no IRI, and a later entry out
/// of order.
#[hegel::test]
fn test_a_defaults_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let predicates = iris(&tc, 4, 12);
    let mut table = entries(&predicates);
    let first = within(&tc, 0, 2);
    let second = within(&tc, first + 1, 3);
    let broken = |rule: usize| rule == first || rule == second;
    let at = within(&tc, 16, 127);
    let last = predicates.len() - 1;
    let other = within(&tc, 0, last - 2);
    if broken(2) {
        table[other] = (2, table[other].1, "");
    }
    if broken(3) {
        table.swap(last - 1, last);
    }
    let mut sections = term_table(&table);
    if broken(1) {
        sections[0].push(0);
    }
    let refusals = [
        reserved(at),
        section_length(539, 0, 24 * predicates.len() + 1, 24),
        field(539, "default", 2),
        field(539, "default order", last as u64),
    ];
    let unused = broken(0).then(|| (at, nonzero_byte(&tc)));
    let laid = laid_out(3, 1, unused, &sections, 0);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<DefaultsView>(&laid.unwrap_or_default()),
        Err(refusals[first].clone())
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same predicates laid out by hand, with the same
/// refusal: a predicate in a body of `position` 0, a predicate repeated, and two out of order.
#[hegel::test]
fn test_a_defaults_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let mut predicates = iris(&tc, 2, 12);
    let index = within(&tc, 1, predicates.len() - 1);
    let broken = within(&tc, 0, 3);
    let position = if broken == 1 {
        0
    } else {
        tc.draw(gs::integers::<u64>().min_value(1))
    };
    match broken {
        2 => predicates[index] = predicates[index - 1].clone(),
        3 => predicates.swap(index - 1, index),
        _ => (),
    }
    let expected = laid_out(3, position, None, &iri_table(&predicates), 0);
    let body = Defaults::reply(3, position, predicates);
    assert_builds_as_laid_out::<_, DefaultsView>(&body, &expected);
    assert_eq!(built(&body).is_err(), broken != 0);
}
