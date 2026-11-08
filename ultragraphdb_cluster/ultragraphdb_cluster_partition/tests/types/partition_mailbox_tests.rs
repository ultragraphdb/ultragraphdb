/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{PartitionHost, PartitionMailbox, RemoteMessage};
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};

use crate::utils::cluster::{Cluster, block_on, changeset, epoch, ex, insert, node, pid};
use crate::utils::hosts::sim_log;

/// A mesh gives each partition a mailbox naming it, with a sender to every partition and none to another.
#[test]
fn test_a_mesh_connects_every_partition_and_no_other() -> TestResult {
    let ids = [PartitionId::new(1)?, PartitionId::new(2)?];
    let mesh = PartitionMailbox::mesh(&ids);
    assert_eq!(
        mesh.iter().map(PartitionMailbox::id).collect::<Vec<_>>(),
        ids
    );
    let first = mesh.first().context("a mailbox")?;
    assert!(ids.iter().all(|id| first.sender(*id).is_some()));
    assert!(first.sender(PartitionId::new(3)?).is_none());
    Ok(())
}

/// A registration for a home outside the mesh goes to the shell, naming the home, the asking write and its references.
#[test]
fn test_a_registration_for_a_home_outside_the_mesh_goes_to_the_shell() -> TestResult {
    let names = Cluster::even(2)?;
    let (a, knows, b) = (
        names.name_in(1, "a")?,
        names.name_in(1, "knows")?,
        names.name_in(2, "b")?,
    );
    let placement = PlacementMap::even(2, NodeId::new(1))?;
    let id = pid(1)?;
    let part = node(id, &placement, epoch(1)?)?;
    let pipeline = Pipeline::new(
        part.applier(),
        sim_log(id)?,
        RoundBounds::new(u32::MAX, u32::MAX),
    )?
    .with_partition(id.get(), epoch(1)?, CloseRequest::default(), &[])
    .bound(id, part.placement());
    let (shell, remote) = std::sync::mpsc::channel();
    let mailbox = PartitionMailbox::mesh(&[id])
        .into_iter()
        .next()
        .context("a mailbox")?
        .with_remote(shell);
    let mut host = PartitionHost::new(&part, pipeline, mailbox);
    let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
    block_on(host.turn(0))??;
    let sent: Vec<RemoteMessage> = remote.try_iter().collect();
    let [RemoteMessage::Register { home, write, batch }] = sent.as_slice() else {
        return Err(TestError::new(format!("not one registration: {sent:?}")));
    };
    assert_eq!((*home, *write), (pid(2)?, ticket));
    let iris: Vec<&str> = batch.iter().map(|(_, r)| r.iri()).collect();
    assert!(iris.contains(&ex(&b).as_str()), "{iris:?}");
    Ok(())
}
