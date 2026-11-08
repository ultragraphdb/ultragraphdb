/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Catalog versions in a partition: a version loaded at start installs its declarations without writing; a version
//! taking effect is a round of its own, its declarations checked against the data, its builds started, its removals
//! applied; the default entries a round first writes are asked of the metadata group once each.

use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_store_apply::{ChangesetErrorEnum, Executed};
use ultragraphdb_traits_catalog::DefaultEntry;

use crate::utils::changesets::{
    TENANT, TestChangesets, changesets, declaration, ex, finish_builds, indexed_term, stored_keys,
    uid, write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Version `version` of the root tenant's catalog declaring each `(predicate, phrase)`.
fn version(version: u64, declared: &[(&str, &str)]) -> TestResult<CatalogContents> {
    let declarations = declared
        .iter()
        .map(|(p, phrase)| declaration(p, phrase).map(Declaration::Predicate))
        .collect::<TestResult<Vec<_>>>()?;
    Ok(CatalogContents::new(TENANT, version, declarations))
}

/// Applies `executed` as a round of its own at the next version.
fn apply_alone(a: &TestChangesets, executed: Executed) -> TestResult {
    let version = a.quads().lists().next_version();
    let _applied = a.apply_round(vec![(version, executed)])?;
    Ok(())
}

/// Executes and applies `contents` taking effect, with the refusals it met.
fn take_effect(a: &TestChangesets, contents: &CatalogContents) -> TestResult<Vec<String>> {
    let execution = a.execute_catalog(contents)?;
    let refusals = execution
        .refusals()
        .iter()
        .map(ToString::to_string)
        .collect();
    apply_alone(a, execution.into_executed())?;
    Ok(refusals)
}

/// Whether the store holds a key of the catalog keyspace.
fn holds_catalog_key(a: &TestChangesets) -> bool {
    let catalog = Keyspace::Catalog.byte();
    stored_keys(a).iter().any(|k| k.first() == Some(&catalog))
}

#[test]
fn test_a_version_taking_effect_installs_its_declarations_and_number() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let refusals = take_effect(&a, &version(2, &[("name", "a property with `exact`")])?)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    let catalog = a.catalog(TENANT)?;
    assert_eq!(catalog.version(), 2);
    let name = catalog
        .predicate(uid(&a, "name")?)
        .context("name is declared")?;
    assert_eq!(name.tokenizers(), &["exact".to_string()]);
    assert!(!holds_catalog_key(&a), "no catalog key is written");
    Ok(())
}

/// A version replayed from its logged round reads the round's dictionary entries as its own: the IRI it names takes
/// the ID the first apply gave it, and nothing else differs from the version taking effect.
#[test]
fn test_a_replayed_version_takes_the_ids_its_logged_round_gave() -> TestResult {
    let original = changesets()?;
    let replica = changesets()?;
    for a in [&original, &replica] {
        write(a, &["<a> <knows> <b>"])?;
    }
    // The original creates terms it then releases, so its next ID is one the replica would not give `name`.
    let released = original.execute(&crate::utils::changesets::changeset(vec![
        crate::utils::changesets::insert("<gone> <p> <q>")?,
    ]))?;
    original.release(&released);
    let contents = version(2, &[("name", "a property with `exact`")])?;
    let first = original.execute_catalog(&contents)?.into_executed();
    let dictionary: Vec<ultragraphdb_traits_store::VersionlessEntry> = first
        .writes()
        .versionless()
        .iter()
        .filter(|e| e.key().first() == Some(&Keyspace::Dictionary.byte()))
        .cloned()
        .collect();
    assert!(!dictionary.is_empty(), "the version names a new IRI");
    let version = original.quads().lists().next_version();
    let _applied = original.apply_round(vec![(version, first)])?;
    let borrowed: Vec<&ultragraphdb_traits_store::VersionlessEntry> = dictionary.iter().collect();
    let replayed = replica.replay_catalog(&contents, &borrowed)?;
    assert!(replayed.refusals().is_empty(), "{:?}", replayed.refusals());
    // Recovery writes the logged round's dictionary entries beside the replayed execution.
    let _replayed = replica.apply_round_with(
        vec![(version, replayed.into_executed())],
        &dictionary,
        version,
    )?;
    assert_eq!(uid(&replica, "name")?, uid(&original, "name")?);
    assert_eq!(replica.catalog(TENANT)?.version(), 2);
    assert_eq!(
        replica.catalog(TENANT)?.predicate(uid(&replica, "name")?),
        original.catalog(TENANT)?.predicate(uid(&original, "name")?)
    );
    Ok(())
}

#[test]
fn test_a_version_is_installed_by_its_round_not_its_execution() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let execution = a.execute_catalog(&version(2, &[("name", "a property")])?)?;
    assert_eq!(a.catalog(TENANT)?.version(), 0, "nothing installed yet");
    let executed = execution.into_executed();
    assert_eq!(executed.catalog_version(), Some(2));
    let plain = a.execute(&crate::utils::changesets::changeset(vec![
        crate::utils::changesets::insert("<a> <knows> <c>")?,
    ]))?;
    assert_eq!(plain.catalog_version(), None, "a write installs no version");
    a.release(&plain);
    apply_alone(&a, executed)?;
    assert_eq!(a.catalog(TENANT)?.version(), 2);
    Ok(())
}

#[test]
fn test_a_version_adding_an_index_to_data_starts_its_build() -> TestResult {
    let a = changesets()?;
    write(&a, &[r#"<a> <name> "big apple""#])?;
    let execution = a.execute_catalog(&version(2, &[("name", "a property with `term`")])?)?;
    assert!(execution.into_executed().starts_build());
    Ok(())
}

#[test]
fn test_a_build_a_version_starts_indexes_the_existing_data() -> TestResult {
    let a = changesets()?;
    write(&a, &[r#"<a> <name> "big apple""#])?;
    let _refusals = take_effect(&a, &version(2, &[("name", "a property with `term`")])?)?;
    finish_builds(&a);
    assert!(!a.rebuilding());
    assert_eq!(indexed_term(&a, "name", "apple")?, vec![uid(&a, "a")?]);
    assert!(!holds_catalog_key(&a), "the swap writes index keys only");
    Ok(())
}

#[test]
fn test_a_declaration_the_data_contradicts_is_refused_and_the_rest_installed() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <nick> \"x\"", "<a> <nick> \"y\""])?;
    let refusals = take_effect(
        &a,
        &version(2, &[("nick", "single-valued"), ("age", "a property")])?,
    )?;
    assert_eq!(
        refusals,
        vec![
            "The declaration of <http://ex.org/nick> is refused by its data: <http://ex.org/a> holds more than one object"
                .to_string()
        ]
    );
    let catalog = a.catalog(TENANT)?;
    assert_eq!(catalog.version(), 2, "the version takes effect");
    assert_eq!(
        catalog.predicate(uid(&a, "nick")?),
        Some(&PredicateDeclaration::new(ex("nick"))),
        "nick keeps its previous declaration, its default entry"
    );
    assert!(
        catalog.predicate(uid(&a, "age")?).is_some(),
        "age is declared"
    );
    Ok(())
}

#[test]
fn test_a_predicate_the_version_no_longer_declares_is_removed_with_its_indexes() -> TestResult {
    let a = changesets()?;
    let _refusals = take_effect(&a, &version(2, &[("name", "a property with `term`")])?)?;
    write(&a, &[r#"<a> <name> "pear""#])?;
    assert_eq!(indexed_term(&a, "name", "pear")?, vec![uid(&a, "a")?]);
    let removed = take_effect(&a, &version(3, &[])?)?;
    assert!(removed.is_empty(), "{removed:?}");
    assert_eq!(a.catalog(TENANT)?.version(), 3);
    assert!(a.catalog(TENANT)?.predicate(uid(&a, "name")?).is_none());
    assert!(
        indexed_term(&a, "name", "pear")?.is_empty(),
        "its term keys are gone"
    );
    Ok(())
}

#[test]
fn test_the_initial_catalog_stays_beneath_every_version() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let _refusals = take_effect(&a, &version(2, &[])?)?;
    let rdf_type = a
        .quads()
        .id_of(TENANT, oxrdf::vocab::rdf::TYPE.as_str())?
        .context("rdf:type has an ID")?;
    assert!(
        a.catalog(TENANT)?.predicate(rdf_type).is_some(),
        "rdf:type stays an edge"
    );
    Ok(())
}

#[test]
fn test_a_version_waits_while_a_build_runs() -> TestResult {
    let a = changesets()?;
    write(&a, &[r#"<a> <name> "pear""#, r#"<a> <tag> "red""#])?;
    let _refusals = take_effect(&a, &version(2, &[("name", "a property with `term`")])?)?;
    assert!(a.rebuilding());
    let refused = a.execute_catalog(&version(
        3,
        &[
            ("name", "a property with `term`"),
            ("tag", "a property with `exact`"),
        ],
    )?);
    assert_eq!(
        refused.err().map(|e| e.0),
        Some(ChangesetErrorEnum::Rebuilding {
            predicate: ex("name").as_str().to_string()
        }),
        "the next version waits for the build's end, naming the build that runs"
    );
    Ok(())
}

#[test]
fn test_a_loaded_version_installs_without_writing() -> TestResult {
    let a = changesets()?;
    write(&a, &[r#"<a> <name> "pear""#])?;
    let before = a.quads().store().contents();
    a.forget_catalogs();
    a.load_catalog(&version(4, &[("name", "a property with `exact`")])?)?;
    assert_eq!(a.quads().store().contents(), before, "nothing is written");
    assert!(!a.rebuilding(), "nothing is built");
    let catalog = a.catalog(TENANT)?;
    assert_eq!(catalog.version(), 4);
    let name = catalog
        .predicate(uid(&a, "name")?)
        .context("name is declared")?;
    assert_eq!(name.tokenizers(), &["exact".to_string()]);
    let rdf_type = a
        .quads()
        .id_of(TENANT, oxrdf::vocab::rdf::TYPE.as_str())?
        .context("rdf:type")?;
    assert!(
        catalog.predicate(rdf_type).is_some(),
        "the initial catalog is beneath it"
    );
    Ok(())
}

#[test]
fn test_a_loaded_version_naming_an_iri_without_an_id_is_refused() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let refused = a.load_catalog(&version(2, &[("never", "a property")])?);
    let error = refused.err().context("refused")?.to_string();
    assert!(error.contains("http://ex.org/never"), "{error}");
    assert_eq!(a.catalog(TENANT)?.version(), 0, "nothing is installed");
    Ok(())
}

#[test]
fn test_a_first_write_asks_for_its_default_entry_once() -> TestResult {
    let a = changesets()?;
    let _initial = a.take_default_entries();
    write(&a, &[r#"<a> <nickname> "Al""#])?;
    let asked = a.take_default_entries();
    let entry = ultragraphdb_core_catalog::encode_declaration(&Declaration::Predicate(
        PredicateDeclaration::new(ex("nickname")),
    ));
    assert_eq!(
        asked,
        vec![DefaultEntry::new(
            TENANT,
            ex("nickname").as_str().to_string(),
            entry
        )]
    );
    write(&a, &[r#"<b> <nickname> "Bo""#])?;
    assert!(a.take_default_entries().is_empty(), "asked once");
    Ok(())
}

#[test]
fn test_a_refused_write_asks_for_no_default_entry() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <knows> <b>"])?;
    let _earlier = a.take_default_entries();
    let refused = a.execute(&crate::utils::changesets::changeset(vec![
        crate::utils::changesets::insert(r#"<a> <alias> "Al""#)?,
        crate::utils::changesets::insert(r#"<a> <knows> "Bob""#)?,
    ]));
    // `knows` is no edge here, so the write is not refused: the default entry is asked only once it commits.
    let executed = refused?;
    assert!(
        a.take_default_entries().is_empty(),
        "nothing is asked before the round"
    );
    a.release(&executed);
    assert!(
        a.take_default_entries().is_empty(),
        "a released write asks for none"
    );
    Ok(())
}

#[test]
fn test_a_declared_predicate_asks_for_no_default_entry() -> TestResult {
    let a = changesets()?;
    let _refusals = take_effect(&a, &version(2, &[("name", "a property")])?)?;
    let _earlier = a.take_default_entries();
    write(&a, &[r#"<a> <name> "Ann""#])?;
    let asked: Vec<String> = a
        .take_default_entries()
        .iter()
        .map(|e| e.predicate().to_string())
        .collect();
    assert!(
        !asked.contains(&ex("name").as_str().to_string()),
        "{asked:?}"
    );
    Ok(())
}

/// A store failing while a version's declarations are checked against the data fails the version whole: no refusal of
/// one declaration, which would let the version take effect without it.
#[test]
fn test_a_store_failure_checking_a_version_fails_it_whole() -> TestResult {
    let a = changesets()?;
    write(&a, &["<a> <nick> \"x\""])?;
    a.quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextScanAfter(0));
    let failed = a.execute_catalog(&version(2, &[("nick", "single-valued")])?);
    assert!(
        matches!(failed.map_err(|e| e.0), Err(ChangesetErrorEnum::Store(_))),
        "the store's failure, not a refusal"
    );
    assert_eq!(a.catalog(TENANT)?.version(), 0, "nothing is installed");
    Ok(())
}

/// The build running is named by its rebuild record's number, and no build by none.
#[test]
fn test_the_build_running_is_named_by_its_record() -> TestResult {
    use ultragraphdb_traits_store::Store;
    let a = changesets()?;
    write(&a, &[r#"<a> <name> "pear""#])?;
    assert_eq!(a.build_record(), None);
    let _refusals = take_effect(&a, &version(2, &[("name", "a property with `term`")])?)?;
    let record = a
        .quads()
        .store()
        .rebuild_records()?
        .first()
        .map(|r| r.id())
        .context("the build's record")?;
    assert_eq!(a.build_record(), Some(record));
    assert!(record > 1, "the round's first version, not a constant");
    finish_builds(&a);
    assert_eq!(a.build_record(), None);
    Ok(())
}

/// At start, a partition asks for the default entry of every predicate its store holds statements of that no version
/// declares, whichever crash point left the command unsent; the group finds the ones it holds.
#[test]
fn test_a_partition_reconciles_its_default_entries_at_start() -> TestResult {
    let a = changesets()?;
    let _refusals = take_effect(&a, &version(2, &[("name", "a property")])?)?;
    write(
        &a,
        &[
            r#"<a> <nick> "Al""#,
            r#"<a> <name> "Ann""#,
            "<a> <knows> <b>",
        ],
    )?;
    // The rounds' own commands are lost with the process.
    let _lost = a.take_default_entries();
    let group = ultragraphdb_traits_catalog::TestCatalogSource::new();
    let knows = DefaultEntry::new(
        TENANT,
        ex("knows").as_str().to_string(),
        ultragraphdb_core_catalog::encode_declaration(&Declaration::Predicate(
            PredicateDeclaration::new(ex("knows")),
        )),
    );
    assert!(
        group.record_default(&knows)?,
        "an earlier command reached the group"
    );
    let restarted = crate::utils::changesets::changesets_over(a.quads().store().clone())?;
    restarted.load_catalog(&version(2, &[("name", "a property")])?)?;
    let sent = restarted.reconcile_defaults(TENANT)?;
    let mut names: Vec<String> = sent.iter().map(|e| e.predicate().to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            ex("knows").as_str().to_string(),
            ex("nick").as_str().to_string()
        ]
    );
    let created: Vec<bool> = sent
        .iter()
        .map(|e| group.record_default(e))
        .collect::<Result<_, _>>()?;
    assert_eq!(
        created.iter().filter(|c| **c).count(),
        1,
        "`nick` created, `knows` found"
    );
    assert_eq!(group.defaults(TENANT).len(), 2, "no record duplicated");
    assert!(
        restarted.reconcile_defaults(3)?.is_empty(),
        "an empty tenant sends none"
    );
    Ok(())
}

/// A version that no longer names an entity type or a relation removes their declarations.
#[test]
fn test_a_version_removes_the_types_and_relations_it_no_longer_names() -> TestResult {
    use ultragraphdb_core_mapping::{RelationDeclaration, TypeDeclaration};
    let a = changesets()?;
    let named = CatalogContents::new(
        TENANT,
        2,
        vec![
            Declaration::EntityType(TypeDeclaration::new(ex("Agent"), None, true)),
            Declaration::EntityType(TypeDeclaration::new(ex("Person"), Some(ex("Agent")), false)),
            Declaration::Relation(RelationDeclaration::new(ex("Marriage"), vec![ex("spouse")])),
        ],
    );
    let execution = a.execute_catalog(&named)?;
    apply_alone(&a, execution.into_executed())?;
    assert_eq!(a.catalog(TENANT)?.entity_types().count(), 2);
    let kept = CatalogContents::new(
        TENANT,
        3,
        vec![Declaration::EntityType(TypeDeclaration::new(
            ex("Person"),
            None,
            false,
        ))],
    );
    let removing = a.execute_catalog(&kept)?;
    apply_alone(&a, removing.into_executed())?;
    let catalog = a.catalog(TENANT)?;
    let types: Vec<&str> = catalog
        .entity_types()
        .map(|(_, t)| t.iri().as_str())
        .collect();
    assert_eq!(types, vec![ex("Person").as_str()]);
    assert_eq!(catalog.relations().count(), 0);
    assert!(!holds_catalog_key(&a));
    Ok(())
}

/// The declarations of a version naming `<foreign>`, whose ID another partition gives, as a predicate, an entity type and a
/// relation, beside an entity type and a relation this partition holds.
fn naming_another_partitions(number: u64) -> TestResult<CatalogContents> {
    use ultragraphdb_core_mapping::{RelationDeclaration, TypeDeclaration};
    Ok(CatalogContents::new(
        TENANT,
        number,
        vec![
            Declaration::Predicate(declaration("foreign", "a property with `exact`")?),
            Declaration::EntityType(TypeDeclaration::new(ex("foreign"), None, false)),
            Declaration::Relation(RelationDeclaration::new(ex("foreign"), vec![ex("role")])),
            Declaration::EntityType(TypeDeclaration::new(ex("Person"), None, false)),
            Declaration::Relation(RelationDeclaration::new(ex("Marriage"), vec![ex("spouse")])),
        ],
    ))
}

/// A loaded version holds a predicate whose ID another partition gives by its name, and leaves out an entity type and a relation
/// of such a name, which no write here names; it declares those whose IDs this partition holds.
#[test]
fn test_a_loaded_version_holds_another_partitions_predicate_by_name() -> TestResult {
    let a = crate::utils::partitions::owning_all_but("foreign")?;
    write(&a, &["<x> <knows> <Person>", "<x> <knows> <Marriage>"])?;
    a.forget_catalogs();
    a.load_catalog(&naming_another_partitions(2)?)?;
    let catalog = a.catalog(TENANT)?;
    assert_eq!(
        catalog.unresolved(ex("foreign").as_str()),
        Some(&declaration("foreign", "a property with `exact`")?)
    );
    let types: Vec<&str> = catalog
        .entity_types()
        .map(|(_, t)| t.iri().as_str())
        .collect();
    assert_eq!(types, vec![ex("Person").as_str()]);
    let relations: Vec<&str> = catalog.relations().map(|(_, r)| r.iri().as_str()).collect();
    assert_eq!(relations, vec![ex("Marriage").as_str()]);
    Ok(())
}

/// A version taking effect holds a predicate whose ID another partition gives by its name, and the next version, which no longer
/// names it, forgets it.
#[test]
fn test_a_version_taking_effect_holds_another_partitions_predicate_until_the_next() -> TestResult {
    let a = crate::utils::partitions::owning_all_but("foreign")?;
    write(&a, &["<x> <knows> <y>"])?;
    let refusals = take_effect(&a, &version(2, &[("foreign", "a property with `exact`")])?)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    assert_eq!(
        a.catalog(TENANT)?.unresolved(ex("foreign").as_str()),
        Some(&declaration("foreign", "a property with `exact`")?)
    );
    let next = take_effect(&a, &version(3, &[])?)?;
    assert!(next.is_empty(), "{next:?}");
    assert_eq!(a.catalog(TENANT)?.unresolved(ex("foreign").as_str()), None);
    Ok(())
}
