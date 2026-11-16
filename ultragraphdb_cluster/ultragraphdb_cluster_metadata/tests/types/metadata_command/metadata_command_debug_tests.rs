/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataCommand;
use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{TestResult, hides};

const SECRET: [u8; 32] = *b"the unknown names' secret, 32 B.";
const STORED_KEY: [u8; 32] = *b"the stored key, of 32 bytes, ok.";
const SERVER_KEY: [u8; 32] = *b"the server key, of 32 bytes, ok.";

/// A drawn secret's `Debug`, compact or pretty, holds no form of it; a principal's holds its fields and neither key.
#[test]
fn test_a_command_never_prints_a_secret_or_a_key() -> TestResult {
    let drawn = MetadataCommand::DrawSecret { secret: SECRET };
    let created = MetadataCommand::CreatePrincipal {
        id: 9,
        name: "ann".to_string(),
        verifier: ScramVerifier::new(vec![1, 2, 3], 4_096, STORED_KEY, SERVER_KEY),
        administrator: true,
    };
    for command in [&drawn, &created] {
        for shown in [format!("{command:?}"), format!("{command:#?}")] {
            hides(&shown, &SECRET)?;
            hides(&shown, &STORED_KEY)?;
            hides(&shown, &SERVER_KEY)?;
        }
    }
    assert_eq!(format!("{drawn:?}"), "DrawSecret { secret: <redacted> }");
    let printed = format!("{created:?}");
    assert!(
        printed.starts_with("CreatePrincipal { id: 9, name: \"ann\", verifier: ScramVerifier {"),
        "{printed}"
    );
    assert!(printed.ends_with("administrator: true }"), "{printed}");
    Ok(())
}

/// A command holding no secret prints every field as its kind's name and fields, as a derived `Debug` would.
#[test]
fn test_a_command_prints_every_field() -> TestResult {
    let tenant = Tenant::new(3);
    let partition = PartitionId::new(5)?;
    let epoch = Epoch::new(39)?;
    let versions = vec![(tenant, 3)];
    let evictions = vec![14_u128];
    let expected = [
        (
            MetadataCommand::OfferRestoring {
                tenant,
                predecessor: 2,
                restores: 1,
                bytes: b"v3".to_vec(),
            },
            format!(
                "OfferRestoring {{ tenant: {tenant:?}, predecessor: 2, restores: 1, bytes: {:?} }}",
                b"v3".to_vec()
            ),
        ),
        (
            MetadataCommand::LearnClose {
                epoch,
                versions: versions.clone(),
                placement: 2,
                evictions: evictions.clone(),
            },
            format!(
                "LearnClose {{ epoch: {epoch:?}, versions: {versions:?}, placement: 2, evictions: {evictions:?} }}"
            ),
        ),
        (
            MetadataCommand::ReportUntrimmed { partition, epoch },
            format!("ReportUntrimmed {{ partition: {partition:?}, epoch: {epoch:?} }}"),
        ),
    ];
    for (command, printed) in expected {
        assert_eq!(format!("{command:?}"), printed);
    }
    Ok(())
}
