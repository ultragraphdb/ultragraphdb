/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the metadata-group, sequencer and schema-changes capabilities this change added or rewrote, bound through
//! the system group: each `When` runs its scenario and checks what its `Then` states, and the `Then` holds that it ran.

use std::num::NonZeroU32;
use std::pin::pin;
use std::time::Duration;

use compio::runtime::spawn;
use cucumber::{then, when};
use futures::future::poll_immediate;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::SequencerConfig;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_system::{ReportAsked, SystemStart};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CATALOG_BYTES_MAX, Catalog, CatalogFrames, Challenge, ChallengeView,
    CloseRequest, Command, Credit, Declare, Declared, DeclaredPredicate, EpochCloseView, ErrorCode,
    FLAG_LAST, Header, MAX_FRAME_FLOOR_U32, MAX_FRAME_SIZE, Placement, PlacementView, Subscribe,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::bdd_stage::{Stage, heard, next_frame, recovered, start_of};
use crate::system_steps::run;

/// The length of the encoding of a version of tenant 0 numbered `version` holding one predicate of an IRI of `length` bytes, by
/// `ultragraphdb_core_catalog`'s encoder, which the group carries as opaque bytes.
fn version_length(version: u64, length: usize) -> TestResult<usize> {
    let iri = oxrdf::NamedNode::new(iri_of(length))?;
    let declared = vec![Declaration::Predicate(PredicateDeclaration::new(iri))];
    Ok(CatalogContents::new(0, version, declared).encode()?.len())
}

/// An IRI of `length` bytes.
fn iri_of(length: usize) -> String {
    let stem = "http://ex.org/";
    format!("{stem}{}", "a".repeat(length.saturating_sub(stem.len())))
}

/// A Declare of one predicate whose IRI makes version 1 of tenant 0 exactly `bytes` long.
fn declaring(bytes: usize) -> TestResult<Vec<Declared>> {
    let (short, long) = (version_length(1, 1_000)?, version_length(1, 1_001)?);
    assert_eq!(
        long,
        short + 1,
        "one byte of version a byte of IRI at this length"
    );
    let length = bytes + 1_000 - short;
    assert_eq!(version_length(1, length)?, bytes);
    Ok(vec![Declared::Predicate(DeclaredPredicate::new(iri_of(
        length,
    )))])
}

/// A Catalog answer read across its frames on `client`, granting credit as each arrives: the answer and its frames' count.
async fn catalog_read(
    client: &mut RoleClient,
    request: &Catalog,
) -> TestResult<(CatalogFrames, usize)> {
    client.send_only(request).await?;
    let stream = client.last_stream();
    let mut frames = CatalogFrames::new();
    for taken in 1..=64 {
        let last = {
            let frame = next_frame(client).await?;
            let view = frame.view()?;
            frames.push(&view)?;
            view.header().flags() & FLAG_LAST != 0
        };
        if last {
            return Ok((frames, taken));
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
    Err(TestError::new("the answer took more than 64 frames"))
}

#[when(
    "version 3 of `t1` is current, a first write of `score` records its default entry, and a client offers version 4 built on version 3"
)]
fn default_no_version(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(41, &start_of(1, 8)?.with_tenants(vec![1])).await?;
        for (built_on, iri) in [
            (0, "http://ex.org/a"),
            (1, "http://ex.org/b"),
            (2, "http://ex.org/c"),
        ] {
            let declared = vec![Declared::Predicate(DeclaredPredicate::new(iri))];
            let _version = heard(
                stage
                    .closing(stage.handle.declare_version(1, built_on, declared))
                    .await?,
            )?;
        }
        let schema = heard(stage.handle.schema_id(1).await)?;
        assert!(!heard(
            stage
                .handle
                .record_default(1, "http://ex.org/score".to_string())
                .await
        )?);
        assert_eq!(
            heard(stage.handle.catalog(1, 0).await)?.version(),
            3,
            "no version"
        );
        assert_eq!(
            heard(stage.handle.schema_id(1).await)?,
            schema,
            "no prepared statement stale"
        );
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(
            "http://ex.org/d",
        ))];
        let (version, _) = heard(
            stage
                .closing(stage.handle.declare_version(1, 3, declared))
                .await?,
        )?;
        assert_eq!(version, 4);
        let recorded = heard(stage.handle.defaults(1).await)?;
        assert_eq!(recorded.predicates(), ["http://ex.org/score".to_string()]);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the default-entry record of (`t1`, `score`) is held, version 4 is accepted, and no prepared statement of `t1` is stale"
)]
fn default_no_version_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "declaring Commits, each within one Declare frame, build a tenant's catalog version until one more would pass 16,777,216 bytes, and the client commits that one"
)]
fn past_cap(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(42, &start_of(1, 8)?).await?;
        // A Declare is one frame within the role connection's `max_frame`, at most `MAX_FRAME_SIZE` (`PROTOCOL.md` §11.13, *A new
        // version*): each Declare here holds one predicate whose IRI fills such a frame but for 4 KiB, built on the version before,
        // until the group refuses one. The versions grow by one declaration of one length each, so the version the refused
        // Declare would build is the last one's bytes and that growth again.
        let length = MAX_FRAME_SIZE - 4_096;
        let mut sizes: Vec<usize> = Vec::new();
        let mut version = 0;
        let mut refusal = None;
        for at in 0..CATALOG_BYTES_MAX / length + 2 {
            let stem = format!("http://ex.org/{at:02}/");
            let iri = format!("{stem}{}", "a".repeat(length - stem.len()));
            let one = vec![Declared::Predicate(DeclaredPredicate::new(iri))];
            let frame = Declare::declarations(0, version, 0, one.clone())
                .frame(Header::new(0, Command::Declare))?;
            assert!(frame.len() <= MAX_FRAME_SIZE, "one Declare frame");
            match stage
                .closing(stage.handle.declare_version(0, version, one))
                .await?
            {
                Ok((built, _)) => {
                    version = built;
                    sizes.push(heard(stage.handle.catalog(0, built).await)?.bytes().len());
                }
                Err(refused) => {
                    refusal = Some(refused);
                    break;
                }
            }
        }
        let refusal = refusal.context("a Declare refused")?;
        let [.., before, last] = sizes.as_slice() else {
            return Err(TestError::new(format!("two versions built: {sizes:?}")));
        };
        let would = last + (last - before);
        assert!(
            *last <= CATALOG_BYTES_MAX && would > CATALOG_BYTES_MAX,
            "the Declare refused is the first past the cap: {sizes:?}"
        );
        assert_eq!(
            (refusal.code(), refusal.detail()),
            (ErrorCode::Refused, CATALOG_BYTES_MAX as u64) // lossless: a `usize` is at most 64 bits
        );
        assert!(
            refusal.message().contains(&would.to_string()),
            "{} names {would}",
            refusal.message()
        );
        assert_eq!(
            heard(stage.handle.catalog(0, 0).await)?.version(),
            version,
            "the current version stays"
        );
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the Commit is refused with code 7, `detail` 16,777,216, the message naming the size, and the current version stays"
)]
fn past_cap_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a catalog version of 523,001 bytes is committed, and a storage process at the floor of `max_frame` reads it with Catalog"
)]
fn past_one_frame(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(43, &start_of(1, 8)?).await?;
        let (version, _) = heard(
            stage
                .closing(stage.handle.declare_version(0, 0, declaring(523_001)?))
                .await?,
        )?;
        let committed = heard(stage.handle.catalog(0, version).await)?;
        let mut storage = stage.dial_taking("storage", MAX_FRAME_FLOOR_U32).await?;
        let (read, frames) = catalog_read(&mut storage, &Catalog::request(0, version)).await?;
        assert_eq!(frames, 2, "two frames, the second alone flagged LAST");
        assert_eq!(
            read.view().context("the answer")?.bytes(),
            committed.bytes()
        );
        assert_eq!(committed.bytes().len(), 523_001);
        drop(storage);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "two Catalog frames hold its bytes, the second alone flagged `LAST`, and the bytes read equal the bytes committed"
)]
fn past_one_frame_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a catalog version of 16,777,216 bytes is committed, the system group's host restarts, and a storage process reads the version with Catalog"
)]
fn at_cap(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 8)?;
        let stage = Stage::new(44, &start).await?;
        let (version, _) = heard(
            stage
                .closing(stage.handle.declare_version(0, 0, declaring(16_777_216)?))
                .await?,
        )?;
        let committed = heard(stage.handle.catalog(0, version).await)?
            .bytes()
            .to_vec();
        assert_eq!(committed.len(), 16_777_216);
        let (storage, clock) = stage.stop().await?;
        storage.power_loss();
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        let mut reader = stage.dial("storage").await?;
        let (read, _frames) = catalog_read(&mut reader, &Catalog::request(0, version)).await?;
        assert!(read.view().context("the answer")?.bytes() == committed.as_slice());
        drop(reader);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the group holds the version after recovery, and the bytes read equal the bytes committed")]
fn at_cap_held(w: &mut Steps) {
    assert!(w.ran);
}

/// The salt a Challenge for `name` over a compute connection to `stage` answers.
async fn challenged(stage: &Stage, name: &str) -> TestResult<Vec<u8>> {
    let mut compute = stage.dial("compute").await?;
    let reply = compute.request(&Challenge::request(name)).await?;
    let view = reply.view()?;
    Ok(ChallengeView::from_frame(&view)?.salt().to_vec())
}

#[when(
    "a client authenticates as the unknown name `nobody`, the system group's host and compute restart, and a client authenticates as `nobody` on another core"
)]
fn unknown_one_salt(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 8)?;
        let stage = Stage::new(45, &start).await?;
        let first = challenged(&stage, "nobody").await?;
        let (storage, clock) = stage.stop().await?;
        let restarted = recovered(&storage, &start, &clock).await?;
        let secret = restarted
            .unknown_secret()
            .context("the log holds the secret")?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        let again = challenged(&stage, "nobody").await?;
        assert_eq!(again, first);
        // The salt the group's half of the exchange derives from the secret (`ultragraphdb_protocol_scram`).
        let derived = ScramVerifier::unknown(
            &secret,
            "nobody",
            ultragraphdb_protocol_scram::DEFAULT_ITERATIONS,
        );
        assert_eq!(again, derived.salt());
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "both challenges carry one salt, and the metadata group's log holds the secret it derives from"
)]
fn unknown_one_salt_held(w: &mut Steps) {
    assert!(w.ran);
}

/// The tenants Placement's runs list at `stage`, and the log position the map was read at.
async fn runs(stage: &Stage) -> TestResult<(Vec<u64>, u64)> {
    let mut storage = stage.dial("storage").await?;
    let reply = storage.request(&Placement::request(0)).await?;
    let view = reply.view()?;
    let placement = PlacementView::from_frame(&view)?;
    let tenants = (0..placement.run_count())
        .filter_map(|index| placement.run(index))
        .map(|run| run.tenant())
        .collect();
    Ok((tenants, placement.position()))
}

#[when("the system group's host starts on a metadata group holding no tenant")]
fn first_start(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 8)?;
        let stage = Stage::new(46, &start).await?;
        let (tenants, position) = runs(&stage).await?;
        assert_eq!(tenants, [0]);
        let (storage, clock) = stage.stop().await?;
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        assert_eq!(
            runs(&stage).await?,
            (vec![0], position),
            "a restart creates none"
        );
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the group holds tenant 0 durably before it listens, Placement's runs list it, and a restart creates none"
)]
fn first_start_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a Bind names tenant 9, for which the partition's placement map lists no run")]
fn bind_unknown_tenant(w: &mut Steps) -> TestResult {
    // The group's half: the placement map it answers lists no run for tenant 9; the partition's refusal is group 4's.
    run(w, || async {
        let stage = Stage::new(47, &start_of(1, 8)?).await?;
        let (tenants, _) = runs(&stage).await?;
        assert!(!tenants.contains(&9), "{tenants:?}");
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction"
)]
fn bind_unknown_tenant_held(w: &mut Steps) {
    assert!(w.ran);
}

/// A stage whose epochs last 1 ms to 10 s, so only a request closes one early.
async fn slow_stage(seed: u64, retained: Option<u32>) -> TestResult<Stage> {
    let mut sequencer = SequencerConfig::new(1, 10_000)?;
    if let Some(retained) = retained.and_then(NonZeroU32::new) {
        sequencer = sequencer.with_closes_retained(retained);
    }
    let start = SystemStart::new(MetadataConfig::new(), sequencer, 1);
    Stage::new(seed, &start).await
}

/// The epoch's longest length at 10 s, in microseconds.
const LONG_US: u64 = 10_000_000;

/// Closes epochs on a stage of long epochs until `through`, each at its longest.
async fn closed_through(stage: &Stage, through: u64) -> TestResult {
    let mut closed = heard(stage.handle.latest_close().await)?;
    while closed < through {
        stage.clock.advance(LONG_US);
        closed = heard(stage.handle.next_close(closed).await)?.epoch().get();
    }
    Ok(())
}

/// The next close on `client`'s subscription: its epoch.
async fn next_close_on(client: &mut RoleClient) -> TestResult<u64> {
    let frame = next_frame(client).await?;
    Ok(EpochCloseView::from_frame(&frame.view()?)?.epoch())
}

#[when(
    "epoch 12 is the latest close, and compute sends CloseRequest naming epoch 13 with `partition` 0"
)]
fn compute_closes(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = slow_stage(48, None).await?;
        closed_through(&stage, 12).await?;
        let mut compute = stage.dial("compute").await?;
        let mut storage = stage.dial("storage").await?;
        compute.send_only(&Subscribe::new(12)).await?;
        storage.send_only(&Subscribe::new(12)).await?;
        compute.send_only(&CloseRequest::frontend(13)).await?;
        // `epoch_ms_min`, 1 ms, past the last close: the request closes the epoch then, not at `epoch_ms_max`.
        compio::time::sleep(Duration::from_millis(20)).await;
        stage.clock.advance(1_000);
        assert_eq!(next_close_on(&mut compute).await?, 13);
        assert_eq!(next_close_on(&mut storage).await?, 13);
        drop((compute, storage));
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the sequencer closes epoch 13 before `epoch_ms_max` has passed, once `epoch_ms_min` has, and every subscriber receives the close"
)]
fn compute_closes_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("epoch 13 is closed and a CloseRequest names epoch 13")]
fn closed_closes_nothing(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = slow_stage(49, None).await?;
        closed_through(&stage, 13).await?;
        let mut compute = stage.dial("compute").await?;
        compute.send_only(&Subscribe::new(13)).await?;
        compute.send_only(&CloseRequest::frontend(13)).await?;
        stage.clock.advance(LONG_US - 1);
        let early = compio::time::timeout(Duration::from_millis(100), compute.next_frame()).await;
        assert!(
            early.is_err(),
            "no frame answers it, and nothing closes before its time"
        );
        stage.clock.advance(1);
        // The group wakes for the next ask and finds epoch 14 due.
        let _latest = heard(stage.handle.latest_close().await)?;
        assert_eq!(next_close_on(&mut compute).await?, 14);
        drop(compute);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("no frame answers it, and epoch 14 closes at its own time")]
fn closed_closes_nothing_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("the sequencer has closed 40 epochs and holds the last 10, and a Subscribe sets `latest`")]
fn late_subscriber(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = slow_stage(50, Some(10)).await?;
        closed_through(&stage, 40).await?;
        let mut compute = stage.dial("compute").await?;
        compute.send_only(&Subscribe::at_latest()).await?;
        assert_eq!(next_close_on(&mut compute).await?, 40);
        stage.clock.advance(LONG_US);
        let _latest = heard(stage.handle.latest_close().await)?;
        assert_eq!(next_close_on(&mut compute).await?, 41);
        drop(compute);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("the stream's first close is that of epoch 40, and the close of epoch 41 follows it")]
fn late_subscriber_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when("a connection whose role is `storage` sends a Subscribe with `latest` 1")]
fn storage_latest(w: &mut Steps) -> TestResult {
    run(w, || async {
        let stage = Stage::new(51, &start_of(1, 8)?).await?;
        let mut storage = stage.dial("storage").await?;
        let refusal = storage
            .request(&Subscribe::at_latest())
            .await
            .err()
            .context("a refusal")?;
        let rule = ProtocolError::field_value(Command::Subscribe.number(), "latest", 1);
        assert!(refusal.to_string().contains(&rule.to_string()), "{refusal}");
        drop(storage);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then("it is refused with `FieldValue` naming `latest`")]
fn storage_latest_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a catalog version is committed as pending while epoch 38 is the latest close, and the system group's host is killed before it closes epoch 39"
)]
fn crash_before_close(w: &mut Steps) -> TestResult {
    run(w, || async {
        let start = start_of(1, 8)?;
        let stage = Stage::new(52, &start).await?;
        let mut closed = 0;
        while closed < 38 {
            closed = stage.close_after(closed).await?;
        }
        let mut declaring = pin!({
            let handle = stage.handle.clone();
            async move {
                let declared = vec![Declared::Predicate(DeclaredPredicate::new(
                    "http://ex.org/p",
                ))];
                handle.declare_version(0, 0, declared).await
            }
        });
        // Polled once, the Declare is sent before the lease, so the group takes it first, whichever of its turns takes either.
        if let Some(early) = poll_immediate(declaring.as_mut()).await {
            return Err(TestError::new(format!(
                "answered before the group ran: {early:?}"
            )));
        }
        // A lease is answered once durable, so its sync makes the version appended before it durable too.
        let _leased = heard(stage.handle.lease(1, 1, 8).await)?;
        let pending = heard(stage.handle.catalog(0, 0).await)?;
        assert_eq!((pending.version(), pending.epoch()), (1, 0));
        assert_eq!(heard(stage.handle.latest_close().await)?, 38);
        let (storage, clock) = stage.kill();
        let lost = declaring.await;
        assert!(lost.is_err(), "the Commit's reply was lost with the host");
        storage.power_loss();
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        let still = heard(stage.handle.catalog(0, 1).await)?;
        assert_eq!(still.epoch(), 0, "still pending");
        assert_eq!(stage.close_after(38).await?, 39);
        let mut assigned = heard(stage.handle.catalog(0, 1).await)?.epoch();
        for _ in 0..100 {
            if assigned != 0 {
                break;
            }
            compio::time::sleep(Duration::from_millis(1)).await;
            assigned = heard(stage.handle.catalog(0, 1).await)?.epoch();
        }
        assert_eq!(assigned, 40);
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "after recovery the version is still pending, and the next close, of epoch 39, assigns it effective epoch 40"
)]
fn crash_before_close_held(w: &mut Steps) {
    assert!(w.ran);
}

#[when(
    "a declaration of `nick` single-valued is being checked over two partitions, one has reported its check passed, the system group's host is killed and restarted, and the other partition then reports its check passed"
)]
fn check_after_restart(w: &mut Steps) -> TestResult {
    const NICK: &str = "http://ex.org/nick";
    run(w, || async {
        let start = start_of(2, 8)?;
        let stage = Stage::new(53, &start).await?;
        let declaring = {
            let handle = stage.handle.clone();
            spawn(async move {
                let declared = vec![Declared::Predicate(
                    DeclaredPredicate::new(NICK).with_single(true),
                )];
                handle.declare_version(0, 0, declared).await
            })
        };
        let mut closed = heard(stage.handle.latest_close().await)?;
        while stage
            .handle
            .catalog(0, 1)
            .await
            .map(|held| held.epoch())
            .unwrap_or(0)
            == 0
        {
            closed = stage.close_after(closed).await?;
        }
        heard(
            stage
                .handle
                .report(ReportAsked::new(Tenant::new(0), 1, 1, None))
                .await,
        )?;
        let (storage, clock) = stage.kill();
        let lost = declaring
            .await
            .map_err(|error| TestError::new(format!("{error:?}")))?;
        assert!(lost.is_err(), "the Commit's reply was lost with the host");
        storage.power_loss();
        let restarted = recovered(&storage, &start, &clock).await?;
        let stage = Stage::serve(storage, clock, restarted).await?;
        heard(
            stage
                .handle
                .report(ReportAsked::new(Tenant::new(0), 1, 2, None))
                .await,
        )?;
        let mut described = None;
        for _ in 0..100 {
            closed = stage.close_after(closed).await?;
            let describe = heard(stage.handle.describe(0).await)?;
            if let Some(nick) = describe
                .predicates()
                .iter()
                .find(|held| held.predicate() == NICK)
            {
                described = Some(nick.single());
                break;
            }
        }
        assert_eq!(described, Some(true), "Describe reports nick single-valued");
        assert_eq!(
            heard(stage.handle.catalog(0, 0).await)?.version(),
            1,
            "the version stands"
        );
        let _ended = stage.stop().await?;
        Ok(())
    })
}

#[then(
    "the version stands, Describe reports `nick` single-valued, and the client's Commit, whose reply was lost, is shown committed by Describe"
)]
fn check_after_restart_held(w: &mut Steps) {
    assert!(w.ran);
}
