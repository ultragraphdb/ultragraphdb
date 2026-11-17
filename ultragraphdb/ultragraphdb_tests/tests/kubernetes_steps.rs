/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The deployment's pods as processes (`features/kubernetes-deployment/pods.feature`): a pod deleted is a process killed by
//! SIGKILL, and its workload starting it again is the same instance started again on the settings the config service holds.

use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use service_utils::WaitStrategy;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogView, Declared, DeclaredPredicate, RoleKind, TermTable, Write,
    WriteChange,
};
use ultragraphdb_tests::{
    CLUSTER, CONFIG_ADDRESS, SECRET, START_SECONDS, STORAGE_BINARY, STORAGE_OBSERVABILITY,
    answering, compute_on_one_host, frontend_holding, holding, http_status, members_at,
    role_of_service, scratch, start_config_service, storage_holding, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Pattern, TENANT, direct_of, iri, read_whole};
use crate::cluster::{
    Cluster, Signal, Spec, client, iri_of_partition, placement_ranges, start_own_as,
};
use crate::shells::{TestClient, pause, run};

/// The writes a client has acknowledged before a pod is deleted under it.
const ACKNOWLEDGED: u32 = 8;

/// How long a write sent as its storage pod is deleted is waited for before it is left in doubt.
const IN_DOUBT: Duration = Duration::from_secs(1);

/// How long a step lets the frontend send the Ended of a commit it answered before a pod is deleted, so the session holds no
/// mark of a connection that closes with it (`PROTOCOL.md` §11.5): far above a turn.
const ENDED: Duration = Duration::from_millis(300);

/// The deadline of the compute process of the scenario whose host restarts: it asks the group every half of it.
const COMPUTE_DEADLINE_MS: u64 = 10_000;

/// How many read-only transactions a read-back tries, one per snapshot, until a snapshot holds every acknowledged commit.
const SNAPSHOTS: u32 = 200;

/// The scenario's own cluster of `spec`, held in `w`.
fn own(w: &mut Steps, spec: &Spec) -> TestResult {
    w.cluster = Some(start_own_as(w.carrier, spec)?);
    Ok(())
}

fn running(w: &Steps) -> TestResult<&Cluster> {
    w.cluster.as_ref().context("the scenario's cluster")
}

/// Kills `instance` as a pod deleted with a grace period of zero, starts it again, and waits until it and the frontend are
/// ready.
fn deleted_and_started_again(w: &mut Steps, instance: &str) -> TestResult {
    let cluster = w.cluster.as_mut().context("the scenario's cluster")?;
    cluster.signal(instance, Signal::Kill)?;
    cluster.restart(instance)?;
    until_answering(&cluster.observability(instance)?, "/ready")?;
    until_answering(&cluster.frontend_ready_address()?, "/ready")
}

/// Every cell of the rows `subject` `predicate` `?o` reads in a read-only transaction, read again from snapshot to snapshot
/// until it holds each of `wanted` or the tries are spent.
async fn read_back(
    client: &mut TestClient,
    (subject, predicate): (&str, &str),
    wanted: &[String],
) -> TestResult<Vec<String>> {
    let pattern = Pattern::of(Some(subject), Some(predicate));
    let mut cells = Vec::new();
    for _ in 0..SNAPSHOTS {
        let reader = client.begin(TENANT, true).await?;
        let answer = read_whole(client, &reader, &pattern).await?;
        client.abort(reader).await?;
        cells = answer.rows.into_iter().flatten().collect();
        if wanted.iter().all(|object| cells.contains(object)) {
            break;
        }
        pause(Duration::from_millis(20)).await;
    }
    Ok(cells)
}

/// Waits until the system group at `system` lists a compute process again: one that lost the group while its host restarted
/// joins it again on its own clock, and until then the frontend, ready already, refuses a read for want of compute.
fn until_compute_listed(cluster: &Cluster) -> TestResult {
    let system = &cluster.system;
    for _ in 0..START_SECONDS * 20 {
        if members_at(system)?
            .iter()
            .any(|(kind, _, _)| *kind == RoleKind::Compute)
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!(
        "no compute process joined the system group at {system} again; compute logged:\n{}",
        cluster.log("compute")?
    )))
}

/// The latest catalog version of the tenant at the system group at `system`, 0 for none, as a partition asks for it.
fn latest_version(system: &str) -> TestResult<u64> {
    let address = Address::parse(system)?;
    Runtime::new()?.block_on(async {
        let mut role = RoleClient::dial(&address, CLUSTER, "storage", SECRET).await?;
        let Ok(reply) = role.request(&Catalog::request(TENANT, 0)).await else {
            return Ok(0);
        };
        let view = reply.view()?;
        Ok(CatalogView::from_frame(&view)?.version())
    })
}

#[when("the chart is installed into an empty namespace of a cluster holding the images")]
fn empty_namespace(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let (s, p, o) = (w.iri("s"), w.iri("p"), w.iri("o"));
    let at = running(w)?.client.clone();
    let committed = run(async {
        let mut client = client(&at).await?;
        Ok(client
            .direct_write(direct_of(&s, &p, &iri(&o))?, None)
            .await)
    })?;
    w.committed = committed.is_ok();
    w.seen.facts = vec![format!("{committed:?}")];
    Ok(())
}

#[then(
    "the config service, every storage instance, compute and the frontend each answer `/ready`, the frontend's pod mounts no volume, and a client reaching the frontend's client endpoint authenticates and commits"
)]
fn namespace_up(w: &mut Steps) -> TestResult {
    let cluster = running(w)?;
    // The scenario's own cluster listens at the declared ports, the config service at its declared address.
    assert_eq!(
        http_status(CONFIG_ADDRESS, "/ready")?,
        200,
        "the config service"
    );
    for instance in ["storage", "compute", "frontend"] {
        assert_eq!(
            http_status(&cluster.observability(instance)?, "/ready")?,
            200,
            "{instance}"
        );
    }
    // The frontend's settings name no data directory: it holds nothing a volume would keep.
    let named: Vec<String> = frontend_holding()?
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| name.ends_with("data_dir"))
        .collect();
    assert!(named.is_empty(), "{named:?}");
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    w.cluster = None;
    Ok(())
}

#[when("a storage pod is deleted and its StatefulSet starts it again with another IP")]
fn storage_restarted(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition().second_storage())?;
    let cluster = running(w)?;
    let system = cluster.system.clone();
    let before = members_at(&system)?;
    deleted_and_started_again(w, "storage_2")?;
    let after = members_at(&system)?;
    let cluster = running(w)?;
    let partition = cluster.partitions.get(1).context("partition 2")?.clone();
    let subject = iri_of_partition(&placement_ranges(&system)?, &w.iri("s"), 2)?;
    let (p, o) = (w.iri("p"), w.iri("o"));
    let at = cluster.client.clone();
    let answered = run(async {
        let mut client = client(&at).await?;
        Ok(client
            .direct_write(direct_of(&subject, &p, &iri(&o))?, None)
            .await)
    })?;
    // The second process's member, its partition at its address, before the deletion and after.
    let held = (RoleKind::Storage, 2, partition);
    w.committed = answered.is_ok() && before == after && after.contains(&held);
    w.seen.facts = vec![
        format!("{before:?}"),
        format!("{after:?}"),
        format!("{answered:?}"),
    ];
    Ok(())
}

#[then(
    "it joins the system group from the same role address, claims the same partitions, and the frontend's next request to it is answered"
)]
fn restarted_at_its_address(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    w.cluster = None;
    Ok(())
}

#[when("the role secret's Secret is absent and a storage pod starts")]
fn secret_absent(w: &mut Steps) -> TestResult {
    let util = crate::services()?;
    let missing = scratch(&format!("absent-role-secret-{}", w.scenario));
    let named = missing.display().to_string();
    let storage = with(storage_holding()?, "cluster.role_secret_file", &named);
    let _service: u32 = start_config_service(
        &util,
        &[
            ("frontend", &frontend_holding()?),
            ("storage", &storage),
            ("compute", &holding(compute_on_one_host(), SECRET)?),
        ],
    )?;
    let log = scratch(&format!("storage-absent-secret-{}.log", w.scenario));
    let pid = util.start_service_from_config(role_of_service(
        STORAGE_BINARY,
        "storage",
        &log,
        WaitStrategy::NoWait,
    ))?;
    let exited = util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?;
    let said = std::fs::read_to_string(&log)?;
    // The Secret made: its file mounted where the setting names it.
    std::fs::write(&missing, SECRET)?;
    let again = scratch(&format!("storage-secret-made-{}.log", w.scenario));
    let ready = answering(STORAGE_OBSERVABILITY, "/ready");
    let _started: u32 =
        util.start_service_from_config(role_of_service(STORAGE_BINARY, "storage", &again, ready))?;
    w.seen.facts = vec![
        exited
            .code()
            .map_or_else(String::new, |code| code.to_string()),
        said,
        named,
        http_status(STORAGE_OBSERVABILITY, "/ready")?.to_string(),
    ];
    w.roles = Some(util);
    Ok(())
}

#[then(
    "the pod exits with code 78 naming the role secret's file, and starts once the Secret exists"
)]
fn secret_named(w: &mut Steps) -> TestResult {
    let [code, said, named, ready] = w.seen.facts.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen.facts)));
    };
    assert_eq!(code, "78", "{said}");
    assert!(said.contains(named.as_str()), "{named}: {said}");
    assert_eq!(ready, "200");
    w.roles = None;
    Ok(())
}

#[when(
    "a storage pod is deleted with a grace period of zero while a client commits through the frontend, and the pod starts again"
)]
fn killed_under_writes(w: &mut Steps) -> TestResult {
    // Compute asks the group every half deadline whether it still lists compute, and joins a restarted one again then: at
    // the default of 60 s, the frontend would refuse reads for up to 30 s after the host restarts (`notes/group-7.md`).
    own(
        w,
        &Spec::one_partition().compute("cluster.join_deadline_ms", COMPUTE_DEADLINE_MS),
    )?;
    let (s, p) = (w.iri("s"), w.iri("p"));
    let at = running(w)?.client.clone();
    let mut writer = run(async { client(&at).await })?;
    let mut acknowledged = Vec::new();
    run(async {
        for index in 0..ACKNOWLEDGED {
            let object = format!("{s}/o{index}");
            let _committed = writer
                .direct_write(direct_of(&s, &p, &iri(&object))?, None)
                .await?;
            acknowledged.push(object);
        }
        Ok(())
    })?;
    // The pod goes as the next write leaves: its answer, if one comes, is the last acknowledged.
    running(w)?.signal("storage", Signal::Kill)?;
    let last = format!("{s}/o{ACKNOWLEDGED}");
    let in_flight = run(async {
        let write = std::pin::pin!(writer.direct_write(direct_of(&s, &p, &iri(&last))?, None));
        let bound = std::pin::pin!(pause(IN_DOUBT));
        Ok(match futures::future::select(write, bound).await {
            futures::future::Either::Left((answered, _)) => answered.is_ok(),
            futures::future::Either::Right(_) => false,
        })
    })?;
    if in_flight {
        acknowledged.push(last);
    }
    drop(writer);
    let cluster = w.cluster.as_mut().context("the scenario's cluster")?;
    cluster.restart("storage")?;
    until_answering(&cluster.observability("storage")?, "/ready")?;
    until_answering(&cluster.frontend_ready_address()?, "/ready")?;
    until_compute_listed(cluster)?;
    let read = run(async {
        let mut reader = client(&at).await?;
        read_back(&mut reader, (&s, &p), &acknowledged).await
    })?;
    w.committed = acknowledged.iter().all(|object| read.contains(object));
    w.seen.facts = vec![format!("{acknowledged:?}"), format!("{read:?}")];
    Ok(())
}

#[then("every commit acknowledged before the deletion reads back once the pod is ready")]
fn acknowledged_read_back(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    w.cluster = None;
    Ok(())
}

#[when(
    "the storage pod hosting the system group is deleted with a grace period of zero after a session was registered and a catalog version committed, and the pod starts again"
)]
fn host_killed(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let (s, p, o, declared) = (w.iri("s"), w.iri("p"), w.iri("o"), w.iri("declared"));
    let cluster = running(w)?;
    let (at, system) = (cluster.client.clone(), cluster.system.clone());
    let mut kept = run(async { client(&at).await })?;
    let session = run(async {
        let session = kept.register().await?;
        let declaring = Write::new(
            TermTable::new(),
            vec![WriteChange::Declare(Declared::Predicate(
                DeclaredPredicate::new(&declared),
            ))],
        );
        let transaction = kept.begin(TENANT, false).await?;
        kept.write(&transaction, &declaring).await?;
        // A Commit of declarations names no session (`PROTOCOL.md` §11.13, *Declarations*).
        let _committed = kept.commit(transaction, None).await?;
        pause(ENDED).await;
        Ok(session)
    })?;
    let before = latest_version(&system)?;
    deleted_and_started_again(w, "storage")?;
    let next = run(async {
        Ok(kept
            .direct_write(direct_of(&s, &p, &iri(&o))?, session)
            .await)
    })?;
    let after = latest_version(&system)?;
    w.committed = next.is_ok() && before > 0 && after == before;
    w.seen.facts = vec![
        format!("{next:?}"),
        format!("versions {before} and {after}"),
    ];
    Ok(())
}

#[then("the session's next commit is accepted and the catalog version is still the tenant's")]
fn books_kept(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    w.cluster = None;
    Ok(())
}

#[when("the frontend's pod is deleted and its Deployment starts another")]
fn frontend_deleted(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let (s, p) = (w.iri("s"), w.iri("p"));
    let (first, second) = (format!("{s}/first"), format!("{s}/second"));
    let at = running(w)?.client.clone();
    let (number, last) = run(async {
        let mut before = client(&at).await?;
        let session = before.register().await?;
        let committed = before
            .direct_write(direct_of(&s, &p, &iri(&first))?, session)
            .await?;
        pause(ENDED).await;
        Ok((session.number(), committed.request()))
    })?;
    deleted_and_started_again(w, "frontend")?;
    let (next, read) = run(async {
        let mut after = client(&at).await?;
        // The session's next request is one above its last committed one (`PROTOCOL.md` §11.5).
        let resumed = after.resume_session(number, last + 1)?;
        let next = after
            .direct_write(direct_of(&s, &p, &iri(&second))?, resumed)
            .await;
        let wanted = [first.clone(), second.clone()];
        Ok((next, read_back(&mut after, (&s, &p), &wanted).await?))
    })?;
    w.committed = next.is_ok() && read.contains(&first) && read.contains(&second);
    w.seen.facts = vec![format!("{next:?}"), format!("{read:?}")];
    Ok(())
}

#[then("a client reconnects, authenticates, and commits through the session it registered before")]
fn session_kept(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    w.cluster = None;
    Ok(())
}
