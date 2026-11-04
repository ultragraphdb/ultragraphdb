/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the change's specs that need several partitions, bound to `features/`: partitions, the
//! term-dictionary's registration, quad-reads' reverse reads, catalog's unique values across partitions and
//! index-maintenance's summed reverse counts. Each runs a [`Cluster`] of partitions in one process.
//!
//! A scenario naming a bucket or a bucket group by number names one a registration carries as given; where the number
//! must come from a subject's IRI, which namespace 0's hash places in its own run, the subject's own group stands for
//! it, as a comment at the step says.

#[path = "utils/cluster.rs"]
mod cluster;

/// The hosts' fixture, which reaches the cluster's by the path the unit tests give it.
mod utils {
    pub(crate) use crate::cluster;
}

#[path = "utils/hosts.rs"]
mod hosts;

use std::collections::BTreeSet;
use std::num::NonZeroU64;

use cucumber::{World, then, when};
use oxrdf::{GraphName, Literal};
use ultragraphdb_cluster_partition::{
    Answer, BoundStep, Fanout, PartitionError, Reference, ReverseQuery,
};
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, BucketGroup, Epoch, Namespace, SystemId, Version};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_apply::Operation;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_commit::CommitErrorEnum;
use ultragraphdb_transactions_txn::{Commit, TxnError};

use crate::cluster::{
    Cluster, NS, Outcome, Written, as_of, begin, changeset, epoch, ex, group, insert,
    insert_literal, pid,
};
use crate::hosts::{Host, on_hosts, turn_all};

#[derive(Default, World)]
struct Steps {
    bound_done: bool,
    cluster: Option<Cluster>,
    /// The scenario's names, in the order its When step chose them.
    names: Vec<String>,
    written: Vec<Written>,
    answers: Vec<Answer>,
    references: Vec<Vec<Reference>>,
    errors: Vec<PartitionError>,
    outcomes: Vec<Outcome>,
    asked: Vec<Vec<PartitionId>>,
    subjects: Vec<Vec<SystemId>>,
    numbers: Vec<u64>,
    reverse: Vec<Result<Vec<SystemId>, PartitionError>>,
}

impl std::fmt::Debug for Steps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Steps")
            .field("names", &self.names)
            .finish_non_exhaustive()
    }
}

impl Steps {
    fn cluster(&mut self) -> TestResult<&mut Cluster> {
        self.cluster.as_mut().context("the scenario's cluster")
    }

    fn cluster_ref(&self) -> TestResult<&Cluster> {
        self.cluster.as_ref().context("the scenario's cluster")
    }

    fn name(&self, at: usize) -> TestResult<&str> {
        Ok(self.names.get(at).context("the scenario's name")?.as_str())
    }
}

/// The ID partition `id` holds for `<name>`.
fn id_at(cluster: &Cluster, id: u32, name: &str) -> TestResult<SystemId> {
    cluster
        .id_at(id, name)?
        .context("the partition holds the ID")
}

/// The epoch of commit version `version`.
fn epoch_of(version: u64) -> Epoch {
    Version::from_u64(version).epoch()
}

/// The groups `entity`'s referrer set names at its home `home` as of `at`, sorted: read at the epoch's top, the watermark
/// below it, since a read above the watermark waits for it.
fn groups_at(cluster: &Cluster, home: u32, entity: SystemId, at: Epoch) -> TestResult<Vec<u32>> {
    let quads = cluster.part(home)?.applier().quads();
    let version = as_of(at).min(quads.lists().applied_ts());
    let set = quads.referrers(NS, entity, version)?;
    Ok(set.at(at).map(|m| m.group().get()).collect())
}

/// A registration of `<name>` at `home` by its IRI from `group`, having seen `named`.
fn registration(home: u32, name: &str, group: BucketGroup, named: u64) -> TestResult<Reference> {
    Ok(Reference::new(
        pid(home)?,
        NS,
        ex(name).as_str().to_string(),
        None,
        Some(group),
        named,
    ))
}

/// The reverse read of `<object>`, homed in `home`, through `<predicate>`, homed in `predicate_home`, at `at`.
fn reverse(
    cluster: &Cluster,
    home: u32,
    object: &str,
    predicate_home: u32,
    predicate: &str,
    at: Epoch,
) -> TestResult<ReverseQuery> {
    Ok(ReverseQuery::new(
        pid(home)?,
        NS,
        Graph::Default,
        id_at(cluster, predicate_home, predicate)?,
        id_at(cluster, home, object)?,
        at,
    ))
}

/// Whether `key` holds the 16 bytes of `id`.
fn names(key: &[u8], id: SystemId) -> bool {
    let bytes = id.to_be_bytes();
    key.windows(bytes.len()).any(|w| w == bytes)
}

/// Whether partition `id`'s store holds a key naming `entity`.
fn holds_key_of(cluster: &Cluster, id: u32, entity: SystemId) -> TestResult<bool> {
    let contents = cluster.contents(id)?;
    Ok(contents.keys().keys().any(|k| names(k, entity)))
}

/// `placement` with each of `ranges` assigned to its partition from `from`, every other bucket as the map has it then.
fn assign(placement: &mut PlacementMap, from: Epoch, ranges: &[(BucketRange, u32)]) -> TestResult {
    let mut layout = placement.ranges_at(from);
    for (range, owner) in ranges {
        let mut next = Vec::with_capacity(layout.len() + 2);
        for (r, p) in layout {
            if r.end() <= range.start() || r.start() >= range.end() {
                next.push((r, p));
                continue;
            }
            if r.start() < range.start() {
                next.push((BucketRange::new(r.start(), range.start())?, p));
            }
            if r.end() > range.end() {
                next.push((BucketRange::new(range.end(), r.end())?, p));
            }
        }
        next.push((*range, pid(*owner)?));
        layout = next;
    }
    placement.change(from, &layout)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A storage process hosts partitions over bucket ranges
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "a storage process hosts partitions 1 and 2, and an entity of each partition's buckets is written"
)]
fn entity_of_each(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let one = cluster.name_in(1, "e")?;
    let two = cluster.name_in(2, "f")?;
    let p = cluster.name_in(1, "p")?;
    let q = cluster.name_in(2, "q")?;
    let first = cluster.write(1, vec![insert_literal(&one, &p, "x")])?;
    let second = cluster.write(2, vec![insert_literal(&two, &q, "y")])?;
    w.written.extend([first, second]);
    w.names = vec![one, two];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "each entity's keys are held only in its own partition's keyspaces, and each partition's log holds only its own rounds"
)]
fn data_apart(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (one, two) = (w.name(0)?, w.name(1)?);
    let one_id = id_at(cluster, 1, one)?;
    let two_id = id_at(cluster, 2, two)?;
    assert_eq!(cluster.id_at(2, one)?, None);
    assert_eq!(cluster.id_at(1, two)?, None);
    assert!(holds_key_of(cluster, 1, one_id)?);
    assert!(!holds_key_of(cluster, 2, one_id)?);
    assert!(holds_key_of(cluster, 2, two_id)?);
    assert!(!holds_key_of(cluster, 1, two_id)?);
    // Each partition applied exactly its own write: its watermark is its write's commit version.
    for (id, written) in [(1, &w.written[0]), (2, &w.written[1])] {
        let watermark = cluster.part(id)?.applier().quads().lists().applied_ts();
        assert_eq!(watermark, written.version()?, "partition {id}");
    }
    Ok(())
}

#[when("the storage binary starts with no partition count configured")]
fn no_partition_count(w: &mut Steps) -> TestResult {
    // The binary's default count is 1 (role-binaries builds the setting); its placement is the even map of one.
    w.cluster = Some(Cluster::even(1)?);
    Ok(())
}

#[then("it hosts one partition owning every bucket")]
fn one_owning_every_bucket(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    assert_eq!(cluster.parts().len(), 1);
    let part = cluster.part(1)?;
    for b in [0, 30_000, Bucket::MAX.get()] {
        part.route(Bucket::new(b)?, cluster.closed())?;
    }
    for namespace in [NS, 1, u64::MAX] {
        assert!(part.owns_run(namespace, cluster.closed()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Requests reach the partition of their bucket
// ---------------------------------------------------------------------------------------------------------------------

/// Two partitions, buckets 0 to 32,767 given to partition 2 and 32,768 to 65,535 to partition 1 from epoch 2, which
/// is closed.
fn bucket_30000_in_two() -> TestResult<Cluster> {
    let mut placement = PlacementMap::even(2, NodeId::new(1))?;
    let ranges = [
        (BucketRange::new(0, 32_768)?, 2),
        (BucketRange::new(32_768, 65_536)?, 1),
    ];
    assign(&mut placement, epoch(2)?, &ranges)?;
    let mut cluster = Cluster::with_placement(&placement)?;
    let _closed = cluster.close()?;
    Ok(cluster)
}

#[when("a write's subject falls in bucket 30,000, which partition 2 owns")]
fn write_in_30000(w: &mut Steps) -> TestResult {
    let cluster = bucket_30000_in_two()?;
    let routed = cluster
        .part(2)?
        .route(Bucket::new(30_000)?, cluster.closed());
    w.errors.extend(routed.err());
    w.cluster = Some(cluster);
    Ok(())
}

#[then("partition 2 applies it")]
fn partition_2_applies(w: &mut Steps) -> TestResult {
    assert!(
        w.errors.is_empty(),
        "partition 2 takes bucket 30,000: {:?}",
        w.errors
    );
    // Namespace 0's subjects lie in its own run, not bucket 30,000's; a subject partition 2 owns stands for it.
    let cluster = w.cluster()?;
    let subject = cluster.name_in(2, "s")?;
    let predicate = cluster.name_in(2, "p")?;
    let version = cluster
        .write(2, vec![insert_literal(&subject, &predicate, "v")])?
        .version()?;
    assert_eq!(
        cluster.part(2)?.applier().quads().lists().applied_ts(),
        version
    );
    Ok(())
}

#[when("a write for bucket 30,000 reaches partition 1")]
fn write_for_30000_at_1(w: &mut Steps) -> TestResult {
    let cluster = bucket_30000_in_two()?;
    w.numbers
        .push(cluster.part(1)?.applier().quads().lists().applied_ts());
    let routed = cluster
        .part(1)?
        .route(Bucket::new(30_000)?, cluster.closed());
    w.errors.push(routed.err().context("a refusal")?);
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "partition 1 refuses it with an error naming bucket 30,000 and partition 2, and writes nothing"
)]
fn refused_with_owner(w: &mut Steps) -> TestResult {
    let error = w.errors.first().context("the refusal")?;
    assert_eq!(error, &PartitionError::wrong_partition(30_000, 2));
    assert!(error.to_string().contains("30000") && error.to_string().contains('2'));
    let before = *w.numbers.first().context("the watermark before")?;
    assert_eq!(
        w.cluster_ref()?
            .part(1)?
            .applier()
            .quads()
            .lists()
            .applied_ts(),
        before
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A partition owns every entry its subjects' writes derive
// ---------------------------------------------------------------------------------------------------------------------

#[when("`<a>`, homed in partition 1, gains `<a> <knows> <b>`, and `<b>` is homed in partition 2")]
fn edge_to_other_partition(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let written = cluster.write(1, vec![insert(&a, &knows, &b)])?;
    let _version = written.version()?;
    w.written.push(written);
    w.names = vec![a, b, knows];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "partition 1 holds the forward entry of `knows` for `a`, its own reverse entry of `knows` for `b` naming `a`, and the object-predicates entry of `b`, all written in one commit, and partition 2's keyspaces gain no entry of the statement"
)]
fn edge_entries_at_subject(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (a, b, knows) = (w.name(0)?, w.name(1)?, w.name(2)?);
    let (a_id, b_id, knows_id) = (
        id_at(cluster, 1, a)?,
        id_at(cluster, 1, b)?,
        id_at(cluster, 1, knows)?,
    );
    let version = w.written.first().context("the write")?.version()?;
    let quads = cluster.part(1)?.applier().quads();
    let object = EncodedTerm::id(b_id);
    let mut forward = Vec::new();
    let read = cluster.read(1, |txn| {
        txn.objects_into(
            GraphName::DefaultGraph.as_ref(),
            ex(a).as_ref().into(),
            ex(knows).as_ref(),
            0,
            &mut forward,
        )
    })?;
    read?;
    assert_eq!(forward, vec![ex(b).into()]);
    assert_eq!(
        quads.subjects(NS, Graph::Default, knows_id, &object, version)?,
        vec![a_id]
    );
    assert_eq!(
        quads.object_predicates(NS, Graph::Default, &object, version)?,
        vec![knows_id]
    );
    // One commit: none of the three is there just before it.
    assert!(
        quads
            .subjects(NS, Graph::Default, knows_id, &object, version - 1)?
            .is_empty()
    );
    assert!(
        quads
            .object_predicates(NS, Graph::Default, &object, version - 1)?
            .is_empty()
    );
    assert!(
        !holds_key_of(cluster, 2, a_id)?,
        "partition 2 holds nothing of a"
    );
    assert!(
        !holds_key_of(cluster, 2, knows_id)?,
        "partition 2 holds nothing of knows"
    );
    Ok(())
}

#[when(
    "subjects homed in partitions 1, 2 and 3 each gain an edge to `<hub>`, homed in partition 1"
)]
fn hub_edges(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(3)?;
    let hub = cluster.name_in(1, "hub")?;
    let knows = cluster.name_in(1, "knows")?;
    let mut subjects = Vec::new();
    for id in 1..=3 {
        let subject = cluster.name_in(id, "s")?;
        let _version = cluster
            .write(id, vec![insert(&subject, &knows, &hub)])?
            .version()?;
        subjects.push(subject);
    }
    w.names = vec![hub, knows];
    w.names.extend(subjects);
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "each partition's reverse index of `<hub>` names only its own subjects, and together they name every subject"
)]
fn hub_spread(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (hub, knows) = (w.name(0)?, w.name(1)?);
    let mut all = BTreeSet::new();
    for id in 1..=3u32 {
        let subject = w.name(id as usize + 1)?;
        let own = id_at(cluster, id, subject)?;
        let quads = cluster.part(id)?.applier().quads();
        let object = EncodedTerm::id(id_at(cluster, id, hub)?);
        let found = quads.subjects(
            NS,
            Graph::Default,
            id_at(cluster, id, knows)?,
            &object,
            u64::MAX,
        )?;
        assert_eq!(found, vec![own], "partition {id}");
        all.extend(found);
    }
    assert_eq!(all.len(), 3);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// An entity's referrer set names where its inbound edges may be
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "`R(o)` holds no member for bucket group 7 and the home accepts, at epoch 12, a registration for group 7 naming generation 0"
)]
fn first_registration(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let o = cluster.name_in(2, "o")?;
    // The home's open epoch is one above its latest delivered close.
    cluster.deliver_through(2, epoch(11)?)?;
    let answer = cluster.register(&registration(2, &o, BucketGroup::new(7)?, 0)?)?;
    w.answers.push(answer);
    w.names = vec![o];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "`R(o)` holds group 7 with epoch 12 and generation 1, and a read of `R(o)` at epoch 11 does not name it"
)]
fn member_at_12(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let answer = *w.answers.first().context("the answer")?;
    let quads = cluster.part(2)?.applier().quads();
    let set = quads.referrers(NS, answer.entity(), u64::MAX)?;
    let member = set
        .member(BucketGroup::new(7)?)
        .context("group 7's member")?;
    assert_eq!((member.epoch(), member.generation().get()), (epoch(12)?, 1));
    assert_eq!(answer.generation(), NonZeroU64::new(1));
    assert!(groups_at(cluster, 2, answer.entity(), epoch(11)?)?.is_empty());
    Ok(())
}

/// `R(o)` brought to generation 3 for group 7: three registrations, naming 0, 1 and 2.
fn at_generation_3(w: &mut Steps) -> TestResult<SystemId> {
    let mut cluster = Cluster::even(2)?;
    let o = cluster.name_in(2, "o")?;
    let g = BucketGroup::new(7)?;
    let mut entity = None;
    for named in 0..3 {
        let answer = cluster.register(&registration(2, &o, g, named)?)?;
        assert_eq!(answer.generation().map(NonZeroU64::get), Some(named + 1));
        entity = Some(answer.entity());
    }
    let entity = entity.context("the entity")?;
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, entity, u64::MAX)?;
    w.numbers
        .push(set.member(g).context("the member")?.epoch().get());
    w.names = vec![o];
    w.cluster = Some(cluster);
    Ok(entity)
}

#[when(
    "`R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 2 arrives"
)]
fn repeat_registration(w: &mut Steps) -> TestResult {
    let _entity = at_generation_3(w)?;
    let o = w.name(0)?.to_string();
    let answer = w
        .cluster()?
        .register(&registration(2, &o, BucketGroup::new(7)?, 2)?)?;
    w.answers.push(answer);
    Ok(())
}

#[then(
    "`R(o)` still holds group 7 at generation 3 with its epoch, and the reply names generation 3"
)]
fn still_generation_3(w: &mut Steps) -> TestResult {
    member_is(w, 3)
}

#[when(
    "`R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 3 arrives"
)]
fn renewal_registration(w: &mut Steps) -> TestResult {
    let _entity = at_generation_3(w)?;
    let o = w.name(0)?.to_string();
    let answer = w
        .cluster()?
        .register(&registration(2, &o, BucketGroup::new(7)?, 3)?)?;
    w.answers.push(answer);
    Ok(())
}

#[then(
    "`R(o)` holds group 7 at generation 4 with its earlier epoch, and the reply names generation 4"
)]
fn generation_4(w: &mut Steps) -> TestResult {
    member_is(w, 4)
}

/// Group 7's member is at `generation` with the epoch it was added at, and the reply named `generation`.
fn member_is(w: &Steps, generation: u64) -> TestResult {
    let cluster = w.cluster_ref()?;
    let answer = *w.answers.first().context("the answer")?;
    assert_eq!(answer.generation().map(NonZeroU64::get), Some(generation));
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, answer.entity(), u64::MAX)?;
    let member = set
        .member(BucketGroup::new(7)?)
        .context("group 7's member")?;
    assert_eq!(member.generation().get(), generation);
    assert_eq!(
        Some(member.epoch().get()),
        w.numbers.first().copied(),
        "its epoch kept"
    );
    Ok(())
}

#[when(
    "a subject of bucket group 3, homed with `<hub>` in partition 1, gains an edge to `<hub>` in a commit at epoch 8"
)]
fn own_group_edge(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let hub = cluster.name_in(1, "hub")?;
    // The subject's own group stands for group 3: namespace 0's subjects lie in its own run.
    let subject = cluster.name_in(1, "s")?;
    let knows = cluster.name_in(1, "knows")?;
    cluster.deliver_through(1, epoch(7)?)?;
    let written = cluster.write(1, vec![insert(&subject, &knows, &hub)])?;
    assert_eq!(epoch_of(written.version()?), epoch(8)?);
    w.written.push(written);
    w.names = vec![hub, subject];
    w.cluster = Some(cluster);
    Ok(())
}

#[then("`R(hub)` at epoch 8 names group 3, added by that commit, and no registration is sent")]
fn own_group_added(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (hub, subject) = (w.name(0)?, w.name(1)?);
    let written = w.written.first().context("the write")?;
    assert!(written.references.is_empty(), "no registration");
    let hub_id = id_at(cluster, 1, hub)?;
    assert_eq!(
        groups_at(cluster, 1, hub_id, epoch(8)?)?,
        vec![group(subject)?.get()]
    );
    assert!(
        groups_at(cluster, 1, hub_id, epoch(7)?)?.is_empty(),
        "added by that commit"
    );
    Ok(())
}

#[when(
    "group 7's buckets belong to partition 2 up to epoch 20 and to partition 3 from epoch 21, and `R(o)` names group 7"
)]
fn group_moves(w: &mut Steps) -> TestResult {
    let g = BucketGroup::new(7)?;
    let mut placement = PlacementMap::even(3, NodeId::new(1))?;
    assign(&mut placement, epoch(1)?, &[(BucketRange::of_group(g), 2)])?;
    assign(&mut placement, epoch(21)?, &[(BucketRange::of_group(g), 3)])?;
    let mut cluster = Cluster::with_placement(&placement)?;
    let o = cluster.name_in(1, "o")?;
    let answer = cluster.register(&registration(1, &o, g, 0)?)?;
    while cluster.closed() < epoch(21)? {
        let _closed = cluster.close()?;
    }
    w.answers.push(answer);
    w.names = vec![o];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "a reverse read of `o` at epoch 20 asks partition 2 for group 7 and one at epoch 21 asks partition 3, and `R(o)` is unchanged by the move"
)]
fn group_resolves_by_epoch(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let entity = w.answers.first().context("the answer")?.entity();
    for (at, asked) in [(20, 2), (21, 3)] {
        // Any predicate serves: the read asks the partitions the set names whatever they hold.
        let query = ReverseQuery::new(pid(1)?, NS, Graph::Default, entity, entity, epoch(at)?);
        let mut subjects = Vec::new();
        let partitions = Fanout::new(cluster.parts()).subjects(&query, &mut subjects)?;
        assert_eq!(partitions, vec![pid(asked)?], "at epoch {at}");
    }
    assert_eq!(
        groups_at(cluster, 1, entity, epoch(20)?)?,
        groups_at(cluster, 1, entity, epoch(21)?)?
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A commit under a registration reply reads its mark
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "transaction T in partition 1 receives a reply naming generation 4 for `(o, 7)`, and another transaction of partition 1 renews `(o, 7)` to generation 5 and commits before T"
)]
fn renewal_races_t(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let o = cluster.name_in(2, "o")?;
    let knows = cluster.name_in(1, "knows")?;
    // The subject's own group stands for group 7.
    let s = cluster.name_in(1, "s")?;
    let g = group(&s)?;
    let other = cluster.name_where("t", |n| Ok(cluster.owner(n)? == 1 && group(n)? == g))?;
    let mut answer = None;
    for named in 0..4 {
        answer = Some(cluster.register(&registration(2, &o, g, named)?)?);
    }
    let four = answer.context("generation 4's answer")?;
    assert_eq!(four.generation().map(NonZeroU64::get), Some(4));
    let t_reference = registration(2, &o, g, 3)?;
    cluster.deliver_through(1, four.close())?;
    let t = cluster.prepare(1, &[(t_reference, four)], vec![insert(&s, &knows, &o)])??;
    let renewal = registration(2, &o, g, 4)?;
    let five = cluster.register(&renewal)?;
    cluster.deliver_through(1, five.close())?;
    let renewing = cluster.prepare(1, &[(renewal, five)], vec![insert(&other, &knows, &o)])??;
    let (renewed, of_t) = cluster.commit_in_turn(1, renewing, t)?;
    assert!(matches!(renewed, Outcome::Committed(_)), "{renewed:?}");
    w.outcomes.push(of_t);
    let retry = cluster.write(1, vec![insert(&s, &knows, &o)])?;
    w.written.push(retry);
    w.answers.push(five);
    w.names = vec![o, s];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "T aborts with a conflict naming the mark key, writes nothing, and its retry marks `(o, 7)` at generation 5"
)]
fn t_conflicts(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let outcome = w.outcomes.first().context("T's outcome")?;
    // The commit step's conflict names the kind of read the renewal overlapped: T's read of the mark key.
    assert!(
        matches!(outcome, Outcome::Rejected(e) if matches!(e.0, CommitErrorEnum::Conflict { .. })),
        "{outcome:?}"
    );
    let retry = w.written.first().context("the retry")?;
    let _version = retry.version()?;
    assert!(
        retry.references.is_empty(),
        "the retry finds the renewed mark"
    );
    let (o, s) = (w.name(0)?, w.name(1)?);
    let mark =
        cluster
            .part(1)?
            .applier()
            .quads()
            .mark(NS, id_at(cluster, 1, o)?, group(s)?, u64::MAX)?;
    assert_eq!(mark.map(NonZeroU64::get), Some(5));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A registration's epoch is at most the epoch of every write referencing it
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "partition 1, whose latest delivered close is 9, registers a reference to an entity homed in partition 2, whose latest delivered close is 11, and the reply names close 11"
)]
fn registrant_behind(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    cluster.deliver_through(1, epoch(9)?)?;
    cluster.deliver_through(2, epoch(11)?)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let references = cluster.references(1, vec![insert(&a, &knows, &b)])?;
    let reference = references.first().context("the reference")?.clone();
    let answer = cluster.register(&reference)?;
    assert_eq!(answer.close(), epoch(11)?);
    assert_eq!(cluster.delivered(1)?, epoch(9)?, "nothing delivered yet");
    let written =
        cluster.write_answered(1, vec![insert(&a, &knows, &b)], vec![(reference, answer)])?;
    w.written.push(written);
    w.answers.push(answer);
    w.names = vec![a, b];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "partition 2 adds `R(o)`'s member at epoch 12, and partition 1 commits the referencing write only after close 11 is delivered to it, at epoch 12 or later"
)]
fn stamped_above_registration(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let answer = *w.answers.first().context("the answer")?;
    let a = w.name(0)?;
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, answer.entity(), u64::MAX)?;
    assert_eq!(set.member(group(a)?).map(|m| m.epoch()), Some(epoch(12)?));
    let version = w.written.first().context("the write")?.version()?;
    assert!(epoch_of(version) >= epoch(12)?, "{version}");
    assert!(cluster.delivered(1)? >= epoch(11)?);
    Ok(())
}

#[when(
    "partitions 1 and 2 each commit an edge to `o` at epochs 14 and 15, and a reverse read of `o` runs at epoch 15 once every partition has closed it"
)]
fn edges_at_14_and_15(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let o = cluster.name_in(1, "o")?;
    let knows = cluster.name_in(1, "knows")?;
    let one = cluster.name_in(1, "s")?;
    let two = cluster.name_in(2, "t")?;
    cluster.deliver_through(1, epoch(13)?)?;
    let first = cluster.write(1, vec![insert(&one, &knows, &o)])?;
    assert_eq!(epoch_of(first.version()?), epoch(14)?);
    cluster.deliver_through(2, epoch(14)?)?;
    let second = cluster.write(2, vec![insert(&two, &knows, &o)])?;
    assert_eq!(epoch_of(second.version()?), epoch(15)?);
    for id in 1..=2 {
        cluster.deliver_through(id, epoch(15)?)?;
    }
    let query = reverse(&cluster, 1, &o, 1, &knows, epoch(15)?)?;
    let mut subjects = Vec::new();
    let _asked = Fanout::new(cluster.parts()).subjects(&query, &mut subjects)?;
    w.subjects.push(subjects);
    w.names = vec![o, one, two];
    w.cluster = Some(cluster);
    Ok(())
}

#[then("`R(o)` at epoch 15 names both partitions' groups, and the read returns both subjects")]
fn both_at_15(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (o, one, two) = (w.name(0)?, w.name(1)?, w.name(2)?);
    let mut expected_groups = vec![group(one)?.get(), group(two)?.get()];
    expected_groups.sort_unstable();
    assert_eq!(
        groups_at(cluster, 1, id_at(cluster, 1, o)?, epoch(15)?)?,
        expected_groups
    );
    let mut expected = vec![id_at(cluster, 1, one)?, id_at(cluster, 2, two)?];
    expected.sort();
    assert_eq!(w.subjects.first(), Some(&expected));
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A read-write transaction stays within one partition in this stage
// ---------------------------------------------------------------------------------------------------------------------

/// The value `"x<n>"` with the smallest `n`, `"x"` first, whose bucket under the unique predicate `<predicate>` is
/// partition `id`'s: the scenario's `"x"` where its partition is chosen.
fn value_in(cluster: &Cluster, id: u32, predicate: SystemId) -> TestResult<String> {
    for n in 0..10_000u32 {
        let text = if n == 0 {
            "x".to_string()
        } else {
            format!("x{n}")
        };
        let b = Bucket::of_value(Namespace::new(NS), predicate, simple(&text)?.as_bytes());
        if cluster
            .part(1)?
            .placement()
            .partition_of(b, cluster.closed())
            .get()
            == id
        {
            return Ok(text);
        }
    }
    Err(TestError::new("no value of the partition"))
}

/// The term encoding of the simple literal `"text"`.
fn simple(text: &str) -> TestResult<EncodedTerm> {
    Ok(
        EncodedTerm::literal(Literal::new_simple_literal(text).as_ref(), |_| {
            Err(TermError::malformed(
                Vec::new(),
                "a simple literal names no datatype",
            ))
        })?
        .into_term(),
    )
}

/// The insert-if-absent of a node with `<predicate>` `"value"`.
fn insert_if_absent(predicate: &str, value: &str) -> Operation {
    Operation::InsertIfAbsent {
        graph: GraphName::DefaultGraph,
        predicate: ex(predicate),
        value: Literal::new_simple_literal(value).into(),
        properties: Vec::new(),
    }
}

/// Runs `operation` as the first operation of a transaction at partition `id`, committed there.
fn operate_at(cluster: &mut Cluster, id: u32, operation: Operation) -> TestResult<Outcome> {
    let prepared = {
        let mut txn = begin(cluster.part(id)?)?;
        txn.operate(operation, 0)?;
        match txn.commit(0, 0, &[], 0)? {
            Commit::ReadWrite(prepared) => prepared,
            Commit::ReadOnly(_) => return Err(TestError::new("a read-write commit")),
        }
    };
    cluster.commit(id, prepared)
}

#[when(
    "`epi.id` is declared unique, partition 2 owns the bucket of `\"x\"`, and a read-write transaction's first operation is an insert-if-absent of a node with `epi.id` `\"x\"`"
)]
fn first_claim(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    cluster.declare(
        "epi.id",
        PredicateDeclaration::new(ex("epi.id")).with_unique(true),
    )?;
    let predicate = id_at(&cluster, 2, "epi.id")?;
    let value = value_in(&cluster, 2, predicate)?;
    let claim = Bucket::of_value(Namespace::new(NS), predicate, simple(&value)?.as_bytes());
    // The frontend binds the transaction to the partition owning the claim's bucket: partition 1 refuses it, naming 2.
    let refused = cluster.part(1)?.route(claim, cluster.closed());
    w.errors
        .push(refused.err().context("partition 1 refuses the claim")?);
    cluster.part(2)?.route(claim, cluster.closed())?;
    w.outcomes.push(operate_at(
        &mut cluster,
        2,
        insert_if_absent("epi.id", &value),
    )?);
    w.numbers.push(u64::from(claim.get()));
    w.cluster = Some(cluster);
    Ok(())
}

#[then("the transaction binds to partition 2 and commits there")]
fn claim_commits_at_2(w: &mut Steps) -> TestResult {
    let claim = u32::try_from(*w.numbers.first().context("the claim's bucket")?)?;
    assert_eq!(
        w.errors.first(),
        Some(&PartitionError::wrong_partition(claim, 2))
    );
    assert!(
        matches!(w.outcomes.first(), Some(Outcome::Committed(_))),
        "{:?}",
        w.outcomes
    );
    Ok(())
}

#[when("a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2")]
fn reference_stays(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    w.written
        .push(cluster.write(1, vec![insert(&a, &knows, &b)])?);
    w.names = vec![a, b];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "it registers the reference at partition 2, commits in partition 1 alone, and does not abort"
)]
fn registered_and_committed(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let written = w.written.first().context("the write")?;
    let homes: Vec<u32> = written
        .references
        .iter()
        .map(|(r, _)| r.home().get())
        .collect();
    assert_eq!(homes, vec![2]);
    let _version = written.version()?;
    let a_id = id_at(cluster, 1, w.name(0)?)?;
    assert!(
        !holds_key_of(cluster, 2, a_id)?,
        "partition 2 holds nothing of the statement"
    );
    Ok(())
}

#[when("a namespace's buckets span partitions 1 and 2, and a client drops the namespace's data")]
fn drop_spread(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let p = cluster.name_in(1, "p")?;
    let _version = cluster
        .write(1, vec![insert_literal(&a, &p, "kept")])?
        .version()?;
    let refused = cluster.part(1)?.check_drop(Some(NS), cluster.closed());
    w.errors.push(refused.err().context("a refusal")?);
    w.names = vec![a];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped"
)]
fn drop_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.errors.first(),
        Some(&PartitionError::cross_partition(1, 2))
    );
    let cluster = w.cluster_ref()?;
    assert!(
        cluster.id_at(1, w.name(0)?)?.is_some(),
        "the data is still held"
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A reference to another partition's entity registers at its home
// ---------------------------------------------------------------------------------------------------------------------

#[when("a transaction in partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2 and new")]
fn first_reference(w: &mut Steps) -> TestResult {
    reference_stays(w)
}

#[then(
    "partition 2 creates `<b>`'s ID and adds `<a>`'s bucket group to `R(b)` durably before it replies, and partition 1 commits only after the reply, writing the mark of `(<b>, <a>'s group)` and `<b>`'s dictionary entries with `<a>`'s statement"
)]
fn first_reference_registered(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (a, b) = (w.name(0)?, w.name(1)?);
    let written = w.written.first().context("the write")?;
    let (_, answer) = written.references.first().context("one registration")?;
    let b_id = id_at(cluster, 2, b)?;
    assert_eq!(answer.entity(), b_id);
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, b_id, u64::MAX)?;
    assert_eq!(
        set.member(group(a)?).map(|m| m.generation()),
        answer.generation()
    );
    let version = written.version()?;
    assert!(
        epoch_of(version) > answer.close(),
        "committed after the reply's close"
    );
    let quads = cluster.part(1)?.applier().quads();
    assert_eq!(
        quads.mark(NS, b_id, group(a)?, version)?,
        answer.generation()
    );
    assert!(
        quads.mark(NS, b_id, group(a)?, version - 1)?.is_none(),
        "with the statement"
    );
    assert_eq!(quads.id_of(NS, ex(b).as_str())?, Some(b_id));
    assert_eq!(quads.iri_of(NS, b_id)?.0, ex(b).as_str());
    Ok(())
}

#[when(
    "partition 1 has committed a reference to `<b>` from a subject of bucket group 7, and a later transaction writes another edge to `<b>` from a subject of group 7"
)]
fn marked_reference(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    // The first subject's own group stands for group 7.
    let a = cluster.name_in(1, "a")?;
    let g = group(&a)?;
    let later = cluster.name_where("s", |n| Ok(cluster.owner(n)? == 1 && group(n)? == g))?;
    let _version = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    let delivered = cluster.delivered(2)?;
    w.references
        .push(cluster.references(1, vec![insert(&later, &knows, &b)])?);
    w.written
        .push(cluster.write(1, vec![insert(&later, &knows, &b)])?);
    cluster.restart(1)?;
    w.references
        .push(cluster.references(1, vec![insert(&later, &knows, &b)])?);
    w.numbers.push(delivered.get());
    w.names = vec![a, b, later];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "it finds `<b>`'s ID and the mark in its own store, sends no registration, and commits without waiting for partition 2, after a restart too"
)]
fn marked_sends_nothing(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    assert!(w.references.iter().all(Vec::is_empty), "{:?}", w.references);
    let written = w.written.first().context("the later write")?;
    assert!(written.references.is_empty());
    let _version = written.version()?;
    let delivered = *w.numbers.first().context("the home's close")?;
    assert_eq!(
        cluster.delivered(2)?.get(),
        delivered,
        "partition 2 did nothing for it"
    );
    Ok(())
}

#[when(
    "partition 1 holds the mark of `(<b>, 7)`, and a transaction writes an edge to `<b>` from a subject of partition 1's bucket group 9"
)]
fn second_group(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    // The two subjects' own groups stand for groups 7 and 9.
    let a = cluster.name_in(1, "a")?;
    let g = group(&a)?;
    let t = cluster.name_where("t", |n| Ok(cluster.owner(n)? == 1 && group(n)? != g))?;
    let _version = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    w.written
        .push(cluster.write(1, vec![insert(&t, &knows, &b)])?);
    w.names = vec![a, b, t];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "it registers `(<b>, 9)` at partition 2, `R(b)` names groups 7 and 9, and partition 1 then holds both marks"
)]
fn both_groups(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (a, b, t) = (w.name(0)?, w.name(1)?, w.name(2)?);
    let written = w.written.first().context("the second write")?;
    let groups: Vec<_> = written.references.iter().map(|(r, _)| r.group()).collect();
    assert_eq!(groups, vec![Some(group(t)?)]);
    let b_id = id_at(cluster, 2, b)?;
    let mut expected = vec![group(a)?.get(), group(t)?.get()];
    expected.sort_unstable();
    assert_eq!(
        groups_at(cluster, 2, b_id, cluster.closed().next()?)?,
        expected
    );
    let quads = cluster.part(1)?.applier().quads();
    for subject in [a, t] {
        assert!(
            quads.mark(NS, b_id, group(subject)?, u64::MAX)?.is_some(),
            "{subject}"
        );
    }
    Ok(())
}

#[when("partition 1 first writes a statement whose predicate `<knows>` is homed in partition 2")]
fn foreign_predicate(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let c = cluster.name_in(1, "c")?;
    let knows = cluster.name_in(2, "knows")?;
    w.written
        .push(cluster.write(1, vec![insert(&a, &knows, &c)])?);
    w.names = vec![knows];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "partition 1 gets `<knows>`'s ID from partition 2 before the commit and copies its entries in both directions, and no referrer set gains a member for `<knows>`"
)]
fn predicate_copied(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let knows = w.name(0)?;
    let written = w.written.first().context("the write")?;
    let _version = written.version()?;
    let (reference, answer) = written.references.first().context("one registration")?;
    assert_eq!((reference.home(), reference.group()), (pid(2)?, None));
    let home_id = id_at(cluster, 2, knows)?;
    assert_eq!(answer.entity(), home_id);
    let quads = cluster.part(1)?.applier().quads();
    assert_eq!(quads.id_of(NS, ex(knows).as_str())?, Some(home_id));
    assert_eq!(quads.iri_of(NS, home_id)?.0, ex(knows).as_str());
    let set = cluster
        .part(2)?
        .applier()
        .quads()
        .referrers(NS, home_id, u64::MAX)?;
    assert!(set.members().is_empty());
    Ok(())
}

#[when(
    "a transaction in partition 1 writes edges to 50 new IRIs in one write request, all homed in partitions of another storage process"
)]
fn fifty_new_iris(w: &mut Steps) -> TestResult {
    let cluster = Cluster::even(2)?;
    let a = cluster.name_in(1, "a")?;
    let knows = cluster.name_in(1, "knows")?;
    // Partition 2 stands for the partitions of the other storage process: one home node.
    let mut objects = Vec::new();
    let mut n = 0u32;
    while objects.len() < 50 {
        let name = format!("o{n}");
        if cluster.owner(&name)? == 2 {
            objects.push(name);
        }
        n += 1;
    }
    let changes = objects.iter().map(|o| insert(&a, &knows, o)).collect();
    w.references.push(cluster.references(1, changes)?);
    w.cluster = Some(cluster);
    Ok(())
}

#[then("partition 1 sends that process one batch of 50 registrations for the request")]
fn one_batch_of_50(w: &mut Steps) -> TestResult {
    let references = w.references.first().context("the references")?;
    assert_eq!(references.len(), 50);
    let batches = references.chunk_by(|x, y| x.home() == y.home()).count();
    assert_eq!(batches, 1, "one batch to the home");
    let home = pid(2)?;
    assert!(references.iter().all(|r| r.home() == home));
    Ok(())
}

#[when(
    "partitions 1 and 3 each register a reference to the new IRI `<b>` at its home, both registrations reaching the home before it replies to either"
)]
fn concurrent_registrants(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(3)?;
    let b = cluster.name_in(2, "b")?;
    let knows = cluster.name_in(1, "knows")?;
    let a = cluster.name_in(1, "a")?;
    let c = cluster.name_in(3, "c")?;
    let from_one = cluster.references(1, vec![insert(&a, &knows, &b)])?;
    let from_three = cluster.references(3, vec![insert(&c, &knows, &b)])?;
    let home = pid(2)?;
    let to_b = |refs: &[Reference]| refs.iter().find(|r| r.home() == home).cloned();
    let one_b = to_b(&from_one).context("partition 1's registration of b")?;
    let three_b = to_b(&from_three).context("partition 3's registration of b")?;
    let answers = cluster.register_all(2, &[one_b.clone(), three_b.clone()])?;
    let mut three_answered = Vec::new();
    for reference in from_three.iter().filter(|r| r.home() != one_b.home()) {
        three_answered.push((reference.clone(), cluster.register(reference)?));
    }
    let (first, second) = (
        *answers.first().context("an answer")?,
        *answers.get(1).context("an answer")?,
    );
    three_answered.push((three_b, second));
    let one = cluster.write_answered(1, vec![insert(&a, &knows, &b)], vec![(one_b, first)])?;
    let three = cluster.write_answered(3, vec![insert(&c, &knows, &b)], three_answered)?;
    w.written.extend([one, three]);
    w.answers.extend([first, second]);
    w.names = vec![a, b, c];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "both replies name the same ID, both partitions' transactions commit, and `R(b)` names both groups"
)]
fn shared_id(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let (a, b, c) = (w.name(0)?, w.name(1)?, w.name(2)?);
    let ids: Vec<_> = w.answers.iter().map(Answer::entity).collect();
    let b_id = id_at(cluster, 2, b)?;
    assert_eq!(ids, vec![b_id, b_id]);
    for written in &w.written {
        let _version = written.version()?;
    }
    let mut expected = vec![group(a)?.get(), group(c)?.get()];
    expected.sort_unstable();
    assert_eq!(
        groups_at(cluster, 2, b_id, cluster.closed().next()?)?,
        expected
    );
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// A reverse read asks the object's referrers
// ---------------------------------------------------------------------------------------------------------------------

/// `<a>` in partition 1 and `<c>` in partition 3 hold `knows` to `<b>`, homed in partition 2, over `count` partitions,
/// every partition's epoch closed after: the names a, b, c and knows.
fn referrers_of_b(w: &mut Steps, count: u32) -> TestResult {
    let mut cluster = Cluster::even(count)?;
    let a = cluster.name_in(1, "a")?;
    let b = cluster.name_in(2, "b")?;
    let c = cluster.name_in(3, "c")?;
    let knows = cluster.name_in(1, "knows")?;
    let _one = cluster.write(1, vec![insert(&a, &knows, &b)])?.version()?;
    let _three = cluster.write(3, vec![insert(&c, &knows, &b)])?.version()?;
    let _closed = cluster.close()?;
    w.names = vec![a, b, c, knows];
    w.cluster = Some(cluster);
    Ok(())
}

/// Runs the reverse read of `<b>` through `knows` at the closed epoch, the partitions' closed epochs `closed` when
/// given.
fn read_b(w: &mut Steps, closed: Option<Vec<Epoch>>) -> TestResult {
    let cluster = w.cluster_ref()?;
    let query = reverse(cluster, 2, w.name(1)?, 1, w.name(3)?, cluster.closed())?;
    let mut fanout = Fanout::new(cluster.parts());
    if let Some(closed) = closed {
        fanout = fanout.with_closed(closed);
    }
    let mut subjects = Vec::new();
    match fanout.subjects(&query, &mut subjects) {
        Ok(asked) => {
            w.asked.push(asked);
            w.reverse.push(Ok(subjects));
        }
        Err(e) => {
            w.reverse.push(Err(e));
            w.subjects.push(subjects);
        }
    }
    Ok(())
}

#[when(
    "`<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch"
)]
fn read_gathers(w: &mut Steps) -> TestResult {
    referrers_of_b(w, 3)?;
    read_b(w, None)
}

#[then("compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`")]
fn gathered(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    assert_eq!(w.asked.first(), Some(&vec![pid(1)?, pid(3)?]));
    let mut expected = vec![
        id_at(cluster, 1, w.name(0)?)?,
        id_at(cluster, 3, w.name(2)?)?,
    ];
    expected.sort();
    assert_eq!(w.reverse.first(), Some(&Ok(expected)));
    Ok(())
}

#[when(
    "`<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs"
)]
fn read_over_four(w: &mut Steps) -> TestResult {
    referrers_of_b(w, 4)?;
    read_b(w, None)
}

#[then(
    "compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4"
)]
fn four_not_asked(w: &mut Steps) -> TestResult {
    let asked = w.asked.first().context("the partitions asked")?;
    assert_eq!(asked, &vec![pid(1)?, pid(3)?]);
    assert!(!asked.contains(&pid(4)?));
    Ok(())
}

#[when(
    "`R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes"
)]
fn lagging_three(w: &mut Steps) -> TestResult {
    referrers_of_b(w, 3)?;
    let closed = w.cluster_ref()?.closed();
    let behind = Epoch::new(closed.get() - 1)?;
    // At the deadline the read gives up waiting: the closed epochs the read meets then.
    read_b(w, Some(vec![closed, closed, behind]))
}

#[then(
    "the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's"
)]
fn timed_out_on_three(w: &mut Steps) -> TestResult {
    assert_eq!(w.reverse.first(), Some(&Err(PartitionError::timed_out(3))));
    assert_eq!(w.subjects.first(), Some(&Vec::new()), "no partial answer");
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Unique values
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "`id` is declared unique, partition 1 owns the bucket of `\"x\"`, a transaction bound to partition 2 writes `<b> <id> \"x\"`, and a transaction bound to partition 1 insert-if-absents a node with `id` `\"x\"`"
)]
fn two_claims(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    cluster.declare("id", PredicateDeclaration::new(ex("id")).with_unique(true))?;
    let predicate = id_at(&cluster, 1, "id")?;
    let value = value_in(&cluster, 1, predicate)?;
    let b = cluster.name_in(2, "b")?;
    let plain = cluster.write(2, vec![insert_literal(&b, "id", &value)])?;
    w.outcomes.push(plain.outcome);
    w.outcomes
        .push(operate_at(&mut cluster, 1, insert_if_absent("id", &value))?);
    w.names = vec![b];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the write in partition 2 aborts with `CrossPartition` naming partitions 2 and 1, the insert-if-absent commits, and one node holds `id` `\"x\"`"
)]
fn one_claim_wins(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.outcomes.first(),
        Some(&Outcome::Refused(PartitionError::from(
            TxnError::cross_partition(2, 1)
        )))
    );
    assert!(
        matches!(w.outcomes.get(1), Some(Outcome::Committed(_))),
        "{:?}",
        w.outcomes
    );
    let cluster = w.cluster_ref()?;
    // Partition 2 holds no statement of `<b>`: the only node holding the value is partition 1's.
    let b = w.name(0)?;
    if let Some(id) = cluster.id_at(2, b)? {
        assert!(
            !holds_key_of(cluster, 2, id)?,
            "partition 2 holds no statement of b"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------
// Count indexes
// ---------------------------------------------------------------------------------------------------------------------

#[when(
    "`friend` has an exact count index, two subjects in partition 1 and one in partition 2 hold `friend` to `<b>`, and a read looks up the objects of reverse count 3"
)]
fn friends_of_b(w: &mut Steps) -> TestResult {
    let mut cluster = Cluster::even(2)?;
    cluster.declare(
        "friend",
        PredicateDeclaration::new(ex("friend")).with_count_index(true),
    )?;
    let b = cluster.name_in(1, "b")?;
    let first = cluster.name_in(1, "s")?;
    let second = cluster.name_where("t", |n| Ok(cluster.owner(n)? == 1))?;
    let third = cluster.name_in(2, "u")?;
    for (id, subject) in [(1, &first), (1, &second), (2, &third)] {
        let _version = cluster
            .write(id, vec![insert(subject, "friend", &b)])?
            .version()?;
    }
    let _closed = cluster.close()?;
    let mut found = Vec::new();
    let friend = id_at(&cluster, 1, "friend")?;
    Fanout::new(cluster.parts()).reverse_counted(
        NS,
        Graph::Default,
        friend,
        3,
        cluster.closed(),
        &mut found,
    )?;
    w.subjects
        .push(found.iter().filter_map(EncodedTerm::as_id).collect());
    w.names = vec![b];
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "partition 1 lists `<b>` under reverse count 2 and partition 2 under 1, and the lookup returns `<b>`"
)]
fn summed_count(w: &mut Steps) -> TestResult {
    let cluster = w.cluster_ref()?;
    let b = w.name(0)?;
    for (id, count) in [(1, 2), (2, 1)] {
        let quads = cluster.part(id)?.applier().quads();
        let object = EncodedTerm::id(id_at(cluster, id, b)?);
        let friend = id_at(cluster, id, "friend")?;
        assert_eq!(
            quads.reverse_count(NS, Graph::Default, friend, &object, u64::MAX)?,
            count,
            "partition {id}"
        );
    }
    assert_eq!(w.subjects.first(), Some(&vec![id_at(cluster, 1, b)?]));
    Ok(())
}

/// The transaction of connection 1's stream 1, bound to the partition.
const BOUND: (u64, u64) = (1, 1);

/// A second, in the microseconds a host's clock counts.
const SECOND_US: u64 = 1_000_000;

/// Runs `body` on a host whose transaction `BOUND` wrote `<new> <p> "x"`, with `<new>` named as partition 1 homes it.
fn on_written(body: impl FnOnce(&mut Host<'_>, &str, &str) -> TestResult) -> TestResult {
    on_hosts(1, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let (new, p) = (ex("new"), ex("p"));
        let write = changeset(vec![insert_literal("new", "p", "x")]);
        let _read = host.bind(BOUND, write.namespace(), 1, Epoch::ZERO, 0)?;
        let step = host.write_bound(BOUND, &write, 0)?;
        assert_eq!(step, BoundStep::Done);
        body(host, new.as_str(), p.as_str())
    })
}

/// The ID the dictionary holds for `iri`.
fn held_id(host: &Host<'_>, iri: &str) -> TestResult<Option<SystemId>> {
    Ok(host
        .partition()
        .applier()
        .quads()
        .id_of(changeset(Vec::new()).namespace(), iri)?)
}

/// A later write of `<new>` commits.
fn later_commits(host: &mut Host<'_>, now_us: u64) -> TestResult {
    let ticket = host.submit(changeset(vec![insert_literal("new", "p", "y")]), now_us)?;
    let ended = turn_all(std::slice::from_mut(host), 4, now_us)?;
    let outcome = ended
        .iter()
        .find(|ended| ended.ticket() == ticket)
        .context("the later write's outcome")?;
    assert!(outcome.outcome().is_ok(), "{:?}", outcome.outcome());
    Ok(())
}

#[when(
    "a read-write transaction writes `<new> <p> \"x\"`, creating `<new>`, and its client then sends nothing, its connection staying open"
)]
fn vanished(w: &mut Steps) -> TestResult {
    on_written(|host, new, _| {
        assert!(
            host.sweep_bound(10 * SECOND_US).is_empty(),
            "within its limit"
        );
        assert_eq!(host.sweep_bound(10 * SECOND_US + 1), [BOUND]);
        assert_eq!(held_id(host, new)?, None);
        assert_eq!(host.sweep_bound(20 * SECOND_US), [], "aborted once");
        later_commits(host, 11 * SECOND_US)
    })?;
    w.bound_done = true;
    Ok(())
}

#[then(
    "the host aborts the transaction once 10 s have passed, nothing it wrote is held, the dictionary holds no entry it created for `<new>`, and a later transaction writing `<new>` commits"
)]
fn aborted_at_limit(w: &mut Steps) {
    assert!(w.bound_done);
}

#[when("a read-write transaction creates `<new>` and the connection carrying it closes after 1 s")]
fn connection_closes(w: &mut Steps) -> TestResult {
    on_written(|host, new, _| {
        host.abort_connection(BOUND.0 + 1);
        assert_eq!(
            host.sweep_bound(10 * SECOND_US).len(),
            0,
            "another connection's close aborts nothing"
        );
        host.abort_connection(BOUND.0);
        assert_eq!(held_id(host, new)?, None);
        assert_eq!(
            host.sweep_bound(20 * SECOND_US),
            [],
            "the limit finds nothing left"
        );
        later_commits(host, SECOND_US)
    })?;
    w.bound_done = true;
    Ok(())
}

#[then(
    "the host aborts the transaction then, not at its limit, and its entry for `<new>` is released"
)]
fn aborted_at_close(w: &mut Steps) {
    assert!(w.bound_done);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
