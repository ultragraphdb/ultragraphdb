/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Declare (`PROTOCOL.md` §13.27): `kind` u8 at 0, `drop` u8 at 1, `tenant` at 8, `built_on` at 16, `principal` at 24,
//! `version` at 32, `commit_version` at 40, and the six sections of a Write (§7.2). Order: unused bytes 2–7 and 48–127
//! zero; `kind`; `drop`; `built_on`; `commit_version`; sections as section 7.2 states; for `kind` 1 at least one
//! record, each of kind 8 to 10; for `kind` 2 no record, the term table's count, the term an IRI; in a `kind` 1 reply
//! `version` and `commit_version` at least 1.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    Command, Declare, DeclareView, Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate,
    DeclaredRelation, DropKind,
};

use crate::utils::layout::{
    EPOCH_MAX, assert_builds_as_laid_out, built, command_header, count, field, frame, nonzero_byte,
    outcome, read_back, reserved, section_length, sections, term_table, u32s, viewed, within,
};

/// An index naming nothing (`PROTOCOL.md` §1).
const ABSENT: u32 = u32::MAX;

/// A Declare's command header fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Fields {
    kind: u8,
    drop: u8,
    tenant: u64,
    built_on: u64,
    principal: u64,
    version: u64,
    commit_version: u64,
}

/// A drawn declaration, by the fields §4.6 lays out for kinds 8, 9 and 10.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Drawn {
    Predicate {
        name: String,
        kind: u8,
        single: bool,
        unique: bool,
        count_index: bool,
        datatype: Option<String>,
        tokenizers: Vec<String>,
        domains: Vec<String>,
    },
    EntityType {
        name: String,
        parent: Option<String>,
        is_abstract: bool,
    },
    Relation {
        name: String,
        roles: Vec<String>,
    },
}

/// A drawn name, from a few so that declarations share names and the string list interns them.
fn name(tc: &TestCase) -> String {
    let tail = tc.draw(gs::sampled_from(vec!["a", "b", "name", "Person", "é", ""]));
    format!("urn:{tail}")
}

fn names(tc: &TestCase, max: usize) -> Vec<String> {
    (0..within(tc, 0, max)).map(|_| name(tc)).collect()
}

fn declaration(tc: &TestCase) -> Drawn {
    match within(tc, 0, 2) {
        0 => Drawn::Predicate {
            name: name(tc),
            kind: tc.draw(gs::sampled_from(vec![0u8, 1, 2])),
            single: tc.draw(gs::booleans()),
            unique: tc.draw(gs::booleans()),
            count_index: tc.draw(gs::booleans()),
            datatype: tc.draw(gs::booleans()).then(|| name(tc)),
            tokenizers: names(tc, 2),
            domains: names(tc, 2),
        },
        1 => Drawn::EntityType {
            name: name(tc),
            parent: tc.draw(gs::booleans()).then(|| name(tc)),
            is_abstract: tc.draw(gs::booleans()),
        },
        _ => Drawn::Relation {
            name: name(tc),
            roles: names(tc, 3),
        },
    }
}

/// The string list a body's names intern into, in order of first use (§4.3).
#[derive(Debug, Default)]
struct Names(Vec<String>);

impl Names {
    fn intern(&mut self, name: &str) -> u32 {
        let found = self.0.iter().position(|held| held == name);
        let index = found.unwrap_or_else(|| {
            self.0.push(name.to_string());
            self.0.len() - 1
        });
        u32::try_from(index).unwrap_or(ABSENT)
    }

    fn sections(&self) -> [Vec<u8>; 2] {
        let mut ends = Vec::new();
        let mut text = Vec::new();
        for name in &self.0 {
            text.extend_from_slice(name.as_bytes());
            ends.extend_from_slice(&u32::try_from(text.len()).unwrap_or(ABSENT).to_le_bytes());
        }
        [ends, text]
    }
}

/// The record and the args of `drawn` by §4.6's table: kind 8 with its flags and the predicate, the datatype or
/// `ABSENT`, the tokenizers' count and names, the domains' count and names; kind 9 with bit 0 for an abstract type,
/// the type and its parent or `ABSENT`; kind 10 with the relation and its roles.
fn record_of(drawn: &Drawn, names: &mut Names) -> (u8, u8, Vec<u32>) {
    let counted = |list: &[String], interned: &mut Names| {
        let mut args = vec![u32::try_from(list.len()).unwrap_or(ABSENT)];
        args.extend(list.iter().map(|name| interned.intern(name)));
        args
    };
    match drawn {
        Drawn::Predicate {
            name,
            kind,
            single,
            unique,
            count_index,
            datatype,
            tokenizers,
            domains,
        } => {
            let flags = kind
                | (u8::from(*single) << 2)
                | (u8::from(*unique) << 3)
                | (u8::from(*count_index) << 4);
            let mut args = vec![names.intern(name)];
            args.push(datatype.as_ref().map_or(ABSENT, |d| names.intern(d)));
            args.extend(counted(tokenizers, names));
            args.extend(counted(domains, names));
            (8, flags, args)
        }
        Drawn::EntityType {
            name,
            parent,
            is_abstract,
        } => {
            let mut args = vec![names.intern(name)];
            args.push(parent.as_ref().map_or(ABSENT, |p| names.intern(p)));
            (9, u8::from(*is_abstract), args)
        }
        Drawn::Relation { name, roles } => {
            let mut args = vec![names.intern(name)];
            args.extend(roles.iter().map(|role| names.intern(role)));
            (10, 0, args)
        }
    }
}

/// The six sections of a Declare holding `declarations` and the term table `terms`, laid out by hand.
fn six_sections(declarations: &[Drawn], terms: &[(u8, &str, &str)]) -> Vec<Vec<u8>> {
    let mut names = Names::default();
    let mut records = Vec::new();
    let mut args: Vec<u32> = Vec::new();
    for drawn in declarations {
        let (kind, flags, record_args) = record_of(drawn, &mut names);
        records.extend_from_slice(&[kind, flags, 0, 0]);
        records.extend(u32s(&[
            u32::try_from(args.len()).unwrap_or(ABSENT),
            u32::try_from(record_args.len()).unwrap_or(ABSENT),
            0,
        ]));
        args.extend(record_args);
    }
    let [term_records, term_text] = term_table(terms);
    let [ends, text] = names.sections();
    vec![records, u32s(&args), term_records, term_text, ends, text]
}

/// The Declare frame of `fields` and `parts` laid out by hand, `unused` a command-header byte set outside the fields.
fn laid_out(fields: Fields, unused: Option<(usize, u8)>, parts: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut header = command_header(&[
        (0, &[fields.kind, fields.drop]),
        (8, &fields.tenant.to_le_bytes()),
        (16, &fields.built_on.to_le_bytes()),
        (24, &fields.principal.to_le_bytes()),
        (32, &fields.version.to_le_bytes()),
        (40, &fields.commit_version.to_le_bytes()),
    ]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    frame(Command::Declare, header, &sections(parts), 0)
}

/// `drawn` as the owned declaration a builder takes.
fn owned(drawn: &Drawn) -> Declared {
    match drawn.clone() {
        Drawn::Predicate {
            name,
            kind,
            single,
            unique,
            count_index,
            datatype,
            tokenizers,
            domains,
        } => {
            let kind = match kind {
                0 => DeclaredKind::Edge,
                1 => DeclaredKind::Property,
                _ => DeclaredKind::Either,
            };
            let predicate = DeclaredPredicate::new(name)
                .with_kind(kind)
                .with_single(single)
                .with_unique(unique)
                .with_count_index(count_index)
                .with_tokenizers(tokenizers)
                .with_domains(domains);
            Declared::Predicate(match datatype {
                Some(datatype) => predicate.with_datatype(datatype),
                None => predicate,
            })
        }
        Drawn::EntityType {
            name,
            parent,
            is_abstract,
        } => Declared::EntityType(DeclaredEntityType::new(name, parent, is_abstract)),
        Drawn::Relation { name, roles } => Declared::Relation(DeclaredRelation::new(name, roles)),
    }
}

/// The Drop kind numbered `number`, by the table of §6.5.
fn drop_kind(number: u8) -> DropKind {
    match number {
        1 => DropKind::Predicate,
        2 => DropKind::Graph,
        3 => DropKind::TenantData,
        _ => DropKind::Tenant,
    }
}

/// Drawn fields of a `kind` 1 Declare; a reply's `version` at least 1 and `commit_version` the first version of a
/// drawn epoch when `reply`.
fn declaring(tc: &TestCase, reply: bool) -> Fields {
    let any = || tc.draw(gs::integers::<u64>());
    let (version, commit_version) = if reply {
        let epoch = tc.draw(gs::integers::<u64>().min_value(1).max_value(EPOCH_MAX));
        (tc.draw(gs::integers::<u64>().min_value(1)), epoch << 22)
    } else {
        (0, 0)
    };
    Fields {
        kind: 1,
        drop: 0,
        tenant: any(),
        built_on: any(),
        principal: any(),
        version,
        commit_version,
    }
}

/// The fields a view reads from `bytes`, with its declarations.
fn read(bytes: &[u8]) -> Result<(Fields, Vec<Declared>, Option<String>), ProtocolErrorEnum> {
    viewed::<DeclareView, _>(bytes, |view| {
        let fields = Fields {
            kind: if view.drop_kind().is_some() { 2 } else { 1 },
            drop: view.drop_kind().map_or(0, DropKind::number),
            tenant: view.tenant(),
            built_on: view.built_on(),
            principal: view.principal(),
            version: view.version(),
            commit_version: view.commit_version(),
        };
        let declarations = (0..view.len())
            .filter_map(|index| view.declaration(index))
            .collect();
        (fields, declarations, view.target().map(str::to_string))
    })
}

fn round_trips(fields: Fields, declarations: &[Drawn], target: Option<&str>, body: Declare) {
    let terms: Vec<(u8, &str, &str)> = target.iter().map(|iri| (1, *iri, "")).collect();
    let expected = laid_out(fields, None, &six_sections(declarations, &terms));
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    let declared: Vec<Declared> = declarations.iter().map(owned).collect();
    assert_eq!(
        read(&bytes),
        Ok((fields, declared, target.map(str::to_string)))
    );
    let again = read_back::<DeclareView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// L1.5, L1.6: a Declare of `kind` 1 with 1 to 8 drawn declarations, a request or a reply, is the bytes laid out by
/// hand, reads back to its fields and builds again to the same bytes.
#[hegel::test]
fn test_a_declare_of_declarations_round_trips(tc: TestCase) {
    let fields = declaring(&tc, tc.draw(gs::booleans()));
    let declarations: Vec<Drawn> = (0..within(&tc, 1, 8)).map(|_| declaration(&tc)).collect();
    let request = Declare::declarations(
        fields.tenant,
        fields.built_on,
        fields.principal,
        declarations.iter().map(owned).collect(),
    );
    let body = if fields.version == 0 {
        request
    } else {
        request.with_version(fields.version, fields.commit_version)
    };
    round_trips(fields, &declarations, None, body);
}

/// L1.7: a Declare of `kind` 2, a request or a reply of any `version`, round-trips: one IRI for `drop` 1 or 2 and no
/// term for 3 or 4, `built_on` and `commit_version` 0.
#[hegel::test]
fn test_a_declare_of_a_drop_round_trips(tc: TestCase) {
    let drop = tc.draw(gs::sampled_from(vec![1u8, 2, 3, 4]));
    let fields = Fields {
        kind: 2,
        drop,
        tenant: tc.draw(gs::integers::<u64>()),
        principal: tc.draw(gs::integers::<u64>()),
        version: tc.draw(gs::sampled_from(vec![0, 0, 1, 5, u64::MAX])),
        ..Fields::default()
    };
    let target = (drop <= 2).then(|| name(&tc));
    let request = Declare::drop(
        fields.tenant,
        fields.principal,
        drop_kind(drop),
        target.clone(),
    );
    let body = if fields.version == 0 {
        request
    } else {
        request.with_version(fields.version, 0)
    };
    round_trips(fields, &[], target.as_deref(), body);
}

/// One entity type, which every rule of section 7.2 passes.
fn one_type() -> Vec<Drawn> {
    vec![Drawn::EntityType {
        name: "urn:a".to_string(),
        parent: None,
        is_abstract: false,
    }]
}

/// The six sections of entity types with one record of `kind`, a change over four args naming the table's one term,
/// at `index` of `count` records.
fn with_a_change(kind: u8, index: usize, count: usize) -> Vec<Vec<u8>> {
    let mut parts = six_sections(&[], &[(1, "urn:s", "")]);
    let mut args: Vec<u32> = Vec::new();
    let mut names = Names::default();
    for position in 0..count {
        let start = u32::try_from(args.len()).unwrap_or(ABSENT);
        let (record_kind, record_args) = if position == index {
            (kind, vec![ABSENT, 0, 0, 0])
        } else {
            (9, vec![names.intern(&format!("urn:t{position}")), ABSENT])
        };
        parts[0].extend_from_slice(&[record_kind, 0, 0, 0]);
        parts[0].extend(u32s(&[
            start,
            u32::try_from(record_args.len()).unwrap_or(ABSENT),
            0,
        ]));
        args.extend(record_args);
    }
    parts[1] = u32s(&args);
    let [ends, text] = names.sections();
    parts[4] = ends;
    parts[5] = text;
    parts
}

/// L1.17: a Declare with one drawn rule broken is refused with that rule's refusal, its field and its index.
#[hegel::test]
fn test_a_declare_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let declaring = declaring(&tc, false);
    let dropping = |drop: u8| Fields {
        kind: 2,
        drop,
        tenant: declaring.tenant,
        principal: declaring.principal,
        ..Fields::default()
    };
    let types = six_sections(&one_type(), &[]);
    let none = six_sections(&[], &[]);
    let one_iri = six_sections(&[], &[(1, "urn:p", "")]);
    let two_iris = six_sections(&[], &[(1, "urn:p", ""), (1, "urn:q", "")]);
    let (fields, unused, parts, refusal) = match within(&tc, 0, 12) {
        0 => {
            let at = tc.draw(gs::sampled_from(
                (2usize..=7).chain(48..=127).collect::<Vec<usize>>(),
            ));
            (
                declaring,
                Some((at, nonzero_byte(&tc))),
                types,
                reserved(at),
            )
        }
        1 => {
            let kind = tc.draw(gs::sampled_from(vec![0u8, 3, 4, 255]));
            let fields = Fields { kind, ..declaring };
            (fields, None, types, field(538, "kind", kind.into()))
        }
        2 => {
            let drop = nonzero_byte(&tc);
            let fields = Fields { drop, ..declaring };
            (fields, None, types, field(538, "drop", drop.into()))
        }
        3 => {
            let drop = tc.draw(gs::sampled_from(vec![0u8, 5, 6, 255]));
            (dropping(drop), None, none, field(538, "drop", drop.into()))
        }
        4 => {
            let built_on = tc.draw(gs::integers::<u64>().min_value(1));
            let fields = Fields {
                built_on,
                ..dropping(3)
            };
            (fields, None, none, field(538, "built_on", built_on))
        }
        5 => {
            let commit_version = tc.draw(gs::integers::<u64>().min_value(1));
            let fields = Fields {
                commit_version,
                ..dropping(4)
            };
            let refusal = field(538, "commit_version", commit_version);
            (fields, None, none, refusal)
        }
        6 => {
            let low = tc.draw(gs::integers::<u64>().min_value(1).max_value((1 << 22) - 1));
            let epoch = tc.draw(gs::integers::<u64>().max_value(EPOCH_MAX));
            let commit_version = (epoch << 22) | low;
            let fields = Fields {
                version: 1,
                commit_version,
                ..declaring
            };
            let refusal = field(538, "commit_version", commit_version);
            (fields, None, types, refusal)
        }
        7 => (declaring, None, none, count(538, "declarations", 0, 1)),
        8 => {
            let count = within(&tc, 1, 4);
            let index = within(&tc, 0, count - 1);
            let kind = tc.draw(gs::sampled_from(vec![1u8, 7]));
            let parts = with_a_change(kind, index, count);
            (
                declaring,
                None,
                parts,
                field(538, "declaration", index as u64),
            )
        }
        9 => (dropping(3), None, types, count(538, "declarations", 1, 0)),
        10 => {
            let (drop, parts, held) = match within(&tc, 0, 3) {
                0 => (within(&tc, 1, 2), none, 0),
                1 => (within(&tc, 1, 2), two_iris, 2),
                2 => (within(&tc, 3, 4), one_iri, 1),
                _ => (within(&tc, 3, 4), two_iris, 2),
            };
            let expected = u64::from(drop <= 2);
            let drop = u8::try_from(drop).unwrap_or(1);
            (
                dropping(drop),
                None,
                parts,
                count(538, "drop terms", held, expected),
            )
        }
        11 => {
            let (kind, second) = tc.draw(gs::sampled_from(vec![(2u8, ""), (3, ""), (4, "en")]));
            let parts = six_sections(&[], &[(kind, "x", second)]);
            let drop = u8::try_from(within(&tc, 1, 2)).unwrap_or(1);
            (
                dropping(drop),
                None,
                parts,
                field(538, "drop term", kind.into()),
            )
        }
        _ => {
            let version = tc.draw(gs::sampled_from(vec![0u64, 4]));
            let commit_version = if version == 0 { 1 << 22 } else { 0 };
            let fields = Fields {
                version,
                commit_version,
                ..declaring
            };
            (fields, None, types, field(538, "version", version))
        }
    };
    let laid = laid_out(fields, unused, &parts);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<DeclareView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}

/// L1.22: with two rules of the Order line broken in a Declare of `kind` 2, the refusal is that of the earlier one:
/// an unused byte, `drop`, `built_on`, `commit_version`, the names (§7.2), a record, the term table's count.
#[hegel::test]
fn test_a_declare_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let first = within(&tc, 0, 5);
    let second = within(&tc, first + 1, 6);
    let broken = |rule: usize| rule == first || rule == second;
    let at = tc.draw(gs::sampled_from(
        (2usize..=7).chain(48..=127).collect::<Vec<usize>>(),
    ));
    let built_on = tc.draw(gs::integers::<u64>().min_value(1));
    let commit_version = tc.draw(gs::integers::<u64>().min_value(1));
    let fields = Fields {
        kind: 2,
        drop: if broken(1) { 7 } else { 3 },
        tenant: 3,
        built_on: if broken(2) { built_on } else { 0 },
        principal: 9,
        version: 5,
        commit_version: if broken(3) { commit_version } else { 0 },
    };
    let declarations = if broken(5) { one_type() } else { Vec::new() };
    let terms: Vec<(u8, &str, &str)> = if broken(6) {
        vec![(1, "urn:p", "")]
    } else {
        Vec::new()
    };
    let mut parts = six_sections(&declarations, &terms);
    if broken(4) {
        parts[4] = vec![0; 3];
    }
    let unused = broken(0).then(|| (at, nonzero_byte(&tc)));
    let refusals = [
        reserved(at),
        field(538, "drop", 7),
        field(538, "built_on", built_on),
        field(538, "commit_version", commit_version),
        section_length(538, 4, 3, 4),
        count(538, "declarations", 1, 0),
        count(538, "drop terms", 1, 0),
    ];
    let laid = laid_out(fields, unused, &parts);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<DeclareView>(&laid.unwrap_or_default()),
        Err(refusals[first].clone())
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same fields laid out by hand, with the same refusal:
/// no declaration, a reply's versions breaking a rule, and a Drop whose target does not fit its kind.
#[hegel::test]
fn test_a_declare_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let versions = vec![
        (0u64, 0u64),
        (1, 1 << 22),
        (7, 9 << 22),
        (4, 0),
        (0, 1 << 22),
        (3, 1),
        (3, (1 << 22) + 2),
    ];
    let (version, commit_version) = tc.draw(gs::sampled_from(versions));
    let replied = |request: Declare| {
        if (version, commit_version) == (0, 0) {
            request
        } else {
            request.with_version(version, commit_version)
        }
    };
    if tc.draw(gs::booleans()) {
        let declarations: Vec<Drawn> = (0..within(&tc, 0, 3)).map(|_| declaration(&tc)).collect();
        let fields = Fields {
            version,
            commit_version,
            ..declaring(&tc, false)
        };
        let expected = laid_out(fields, None, &six_sections(&declarations, &[]));
        let body = replied(Declare::declarations(
            fields.tenant,
            fields.built_on,
            fields.principal,
            declarations.iter().map(owned).collect(),
        ));
        assert_builds_as_laid_out::<_, DeclareView>(&body, &expected);
    } else {
        let drop = tc.draw(gs::sampled_from(vec![1u8, 2, 3, 4]));
        let target = tc.draw(gs::booleans()).then(|| name(&tc));
        let fields = Fields {
            kind: 2,
            drop,
            tenant: 3,
            principal: 9,
            version,
            commit_version,
            ..Fields::default()
        };
        let terms: Vec<(u8, &str, &str)> = target.iter().map(|iri| (1, iri.as_str(), "")).collect();
        let expected = laid_out(fields, None, &six_sections(&[], &terms));
        let body = replied(Declare::drop(3, 9, drop_kind(drop), target.clone()));
        assert_builds_as_laid_out::<_, DeclareView>(&body, &expected);
    }
}
