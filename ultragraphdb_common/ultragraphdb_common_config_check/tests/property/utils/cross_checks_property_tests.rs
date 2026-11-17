/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The checks only a target's set of instances can show (config-service, "The config service validates every setting before
//! it serves one"): laws L7.1 to L7.6 of `notes/laws.md`. Each verdict is the rule of the specification's table applied to
//! what the test drew; which check refused is read from the setting each defect names.

use hegel::TestCase;

use crate::utils::drawn_target::{
    Instances, SYSTEM_PORT, coin, defects, host, named, of, unhost, valid, within,
};

/// A coin that comes up three times in four.
fn likely(tc: &TestCase) -> bool {
    within(tc, 0, 3) != 0
}

/// A coin that comes up once in four.
fn rare(tc: &TestCase) -> bool {
    !likely(tc)
}

/// Whether a defect is the System check's: it names the `[system]` table, a moved table, or a setting of either.
fn of_the_system_check(defect: &str) -> bool {
    let (_, setting) = named(defect);
    let table = setting
        .split_once('.')
        .map_or(setting.as_str(), |(table, _)| table);
    ["system", "metadata", "sequencer"].contains(&table)
}

/// L7.1: over a target of one frontend, 0 to 3 compute and 1 to 8 storage instances, one or two storage instances drawn
/// holding `[system]`, each with or without each of its two settings, `[metadata]`, `[sequencer]` and a partition or not, and the frontend and each compute
/// instance drawn holding `[metadata]` or `[sequencer]` or not, the System check accepts exactly the targets in which one
/// storage instance holds `[system]` with both settings, both moved tables and a partition, and no other holds a moved table.
#[hegel::test]
fn test_the_system_check_accepts_exactly_one_complete_host(tc: TestCase) {
    let (computes, storages) = (within(&tc, 0, 3), within(&tc, 1, 8));
    let mut instances = valid(computes, storages);
    unhost(of(&mut instances, "storage_1"));
    // One host drawn, a second one now and then: each part of a host kept, and each table elsewhere held, at odds that leave
    // an accepted target likely beside every refused one.
    let first = within(&tc, 1, storages);
    let second = (storages > 1 && rare(&tc)).then(|| first % storages + 1);
    let (mut hosts, mut complete, mut stray) = (0u64, 0u64, false);
    for index in 1..=storages {
        let settings = of(&mut instances, &format!("storage_{index}"));
        let holds = index == first || Some(index) == second;
        let drawn: [bool; 5] = if holds {
            std::array::from_fn(|_| likely(&tc))
        } else {
            [true, true, rare(&tc), rare(&tc), likely(&tc)]
        };
        let [role_address, data_dir, metadata, sequencer, partition] = drawn;
        if holds {
            host(settings, index, storages);
            hosts += 1;
            complete += u64::from(drawn.iter().all(|kept| *kept));
        }
        for (kept, table, key) in [
            (role_address, "system", "role_address"),
            (data_dir, "system", "data_dir"),
        ] {
            if holds && !kept {
                settings.remove(table, key);
            }
        }
        for (kept, table) in [(metadata, "metadata"), (sequencer, "sequencer")] {
            match (holds, kept) {
                (true, false) => settings.drop_table(table),
                (false, true) => {
                    settings.header(table);
                    stray = true;
                }
                (true, true) | (false, false) => {}
            }
        }
        if !partition {
            settings.bare("storage", "partition_count", &0);
        }
    }
    let others: Vec<String> = std::iter::once("frontend".to_string())
        .chain((1..=computes).map(|index| format!("compute_{index}")))
        .collect();
    for name in others {
        for table in ["metadata", "sequencer"] {
            if rare(&tc) {
                of(&mut instances, &name).header(table);
                stray = true;
            }
        }
    }
    let accepted = hosts == 1 && complete == 1 && !stray;
    let refusals: Vec<String> = defects(&instances, None)
        .into_iter()
        .filter(|defect| of_the_system_check(defect))
        .collect();
    assert_eq!(
        refusals.is_empty(),
        accepted,
        "{refusals:?} of {instances:?}"
    );
}

/// One instance's `cluster.system_address`, drawn as the host's role address or one differing in a drawn set of its parts.
struct Varied {
    scheme: bool,
    host: bool,
    port: bool,
    path: bool,
}

/// The address `varied` makes of the host's: a TCP address on `port`, or a Unix socket at `path`.
fn address(tcp: bool, port: u64, path: &str, varied: &Varied) -> String {
    let tcp = tcp != varied.scheme;
    let host = if varied.host {
        "system.svc"
    } else {
        "10.0.2.1"
    };
    let port = if varied.port { port + 1 } else { port };
    let path = if varied.path {
        format!("{path}.other")
    } else {
        path.to_string()
    };
    if tcp {
        format!("tcp://{host}:{port}")
    } else {
        format!("unix://{path}")
    }
}

/// L7.2: the host's `system.role_address` drawn on TCP or a Unix socket, and each instance's `cluster.system_address` equal to
/// it but one drawn instance's, or none's, differing in a drawn set of its scheme, host, port and Unix path: the System address check accepts exactly the
/// targets in which every address agrees with the host's in scheme, port and Unix path, a host alone free to differ.
#[hegel::test]
fn test_the_system_address_check_accepts_exactly_the_agreeing_targets(tc: TestCase) {
    let (computes, storages) = (within(&tc, 0, 3), within(&tc, 1, 8));
    let mut instances = valid(computes, storages);
    let tcp = coin(&tc);
    let path = "/run/ultragraph/system.sock";
    let role_address = if tcp {
        format!("tcp://10.0.2.1:{SYSTEM_PORT}")
    } else {
        format!("unix://{path}")
    };
    of(&mut instances, "storage_1").text("system", "role_address", &role_address);
    let names: Vec<String> = instances.keys().cloned().collect();
    // One drawn instance's address varies in a drawn set of its parts, or none does: varying every address at once would
    // leave an agreeing target, and a host alone differing, too rare to be drawn.
    let count = u64::try_from(names.len()).unwrap_or_default();
    let at = usize::try_from(within(&tc, 0, count)).unwrap_or_default();
    let varied = Varied {
        scheme: coin(&tc),
        host: coin(&tc),
        port: coin(&tc),
        path: coin(&tc),
    };
    // The rule of the specification's table: a scheme, then a port on TCP or a path on a Unix socket.
    let agreeing = names.get(at).is_none()
        || (!varied.scheme && if tcp { !varied.port } else { !varied.path });
    let equal = Varied {
        scheme: false,
        host: false,
        port: false,
        path: false,
    };
    for (index, name) in names.iter().enumerate() {
        let parts = if index == at { &varied } else { &equal };
        let drawn = address(tcp, SYSTEM_PORT, path, parts);
        of(&mut instances, name).text("cluster", "system_address", &drawn);
    }
    let refusals: Vec<String> = defects(&instances, None)
        .into_iter()
        .filter(|defect| named(defect).1 == "cluster.system_address")
        .collect();
    assert_eq!(
        refusals.is_empty(),
        agreeing,
        "{refusals:?} of {instances:?}"
    );
}

/// A directory of 1 to 4 components, each one of a few names so that drawn directories often share a path, written with a
/// trailing separator or not.
fn directory(tc: &TestCase) -> String {
    let names = ["data", "system", "storage"];
    let components: String = (0..within(tc, 1, 4))
        .map(|_| {
            let at = usize::try_from(within(tc, 0, 2)).unwrap_or_default();
            format!("/{}", names.get(at).copied().unwrap_or("data"))
        })
        .collect();
    if coin(tc) {
        format!("{components}/")
    } else {
        components
    }
}

/// Whether two directories share a path, by their text: equal, or one followed by a separator beginning the other, a
/// trailing separator ignored.
fn share_a_path(first: &str, second: &str) -> bool {
    let (first, second) = (first.trim_end_matches('/'), second.trim_end_matches('/'));
    first == second
        || first.starts_with(&format!("{second}/"))
        || second.starts_with(&format!("{first}/"))
}

/// L7.3: `system.data_dir` and every storage instance's `data_dir` drawn as directories of 1 to 4 components, each storage
/// instance but the host on the host's machine or a machine of its own: the Data check refuses `system.data_dir` exactly
/// when it is equal to, inside or containing a `data_dir` of an instance on its host, the host's own included.
#[hegel::test]
fn test_the_data_check_refuses_exactly_a_shared_path_on_the_host(tc: TestCase) {
    let (computes, storages) = (within(&tc, 0, 3), within(&tc, 1, 8));
    let mut instances = valid(computes, storages);
    let system = directory(&tc);
    of(&mut instances, "storage_1").text("system", "data_dir", &system);
    let mut shared = false;
    for index in 1..=storages {
        let settings = of(&mut instances, &format!("storage_{index}"));
        let data_dir = directory(&tc);
        settings.text("storage", "data_dir", &data_dir);
        let beside = index == 1 || coin(&tc);
        if beside {
            // On the host's machine, at ports of its own.
            let port = 7000 + 10 * index;
            settings.text("storage", "role_address", &format!("tcp://10.0.2.1:{port}"));
            settings.text(
                "observability",
                "address",
                &format!("tcp://10.0.2.1:{}", 8000 + index),
            );
            shared |= share_a_path(&system, &data_dir);
        }
    }
    let refusals: Vec<String> = defects(&instances, None)
        .into_iter()
        .filter(|defect| named(defect).1 == "system.data_dir")
        .collect();
    assert_eq!(
        !refusals.is_empty(),
        shared,
        "{refusals:?} of {instances:?}"
    );
}

/// L7.4: an accepted target as the active revision, and a new revision equal to it, its host's `system.role_address`
/// differing in its host alone, its host moved to `storage_2`, or its `system.data_dir` moved: the Fixed check accepts the
/// first two and refuses the others naming the setting.
#[hegel::test]
fn test_the_fixed_check_keeps_the_host_and_its_directory(tc: TestCase) {
    let (computes, storages) = (within(&tc, 0, 3), within(&tc, 2, 8));
    let active = valid(computes, storages);
    let mut changed = active.clone();
    let refused = match within(&tc, 0, 3) {
        0 => None,
        1 => {
            let named_host = format!("tcp://storage-1.ci:{SYSTEM_PORT}");
            of(&mut changed, "storage_1").text("system", "role_address", &named_host);
            None
        }
        2 => {
            unhost(of(&mut changed, "storage_1"));
            host(of(&mut changed, "storage_2"), 2, storages);
            Some("instance storage_2, setting system:")
        }
        _ => {
            of(&mut changed, "storage_1").text("system", "data_dir", "/data/system2");
            Some("instance storage_1, setting system.data_dir:")
        }
    };
    let found = defects(&changed, Some(&active));
    match refused {
        None => assert!(found.is_empty(), "{found:?}"),
        Some(naming) => assert!(
            found
                .iter()
                .any(|defect| defect.contains(naming) && defect.contains("active revision")),
            "{naming} in {found:?}"
        ),
    }
}

/// A defect planted in an accepted target of one frontend, three compute and three storage instances: what it changes, and
/// the instance and setting its refusal names. Each stands in an instance of its own, so no two meet.
const PLANTED: [(&str, &str, &str, &str); 7] = [
    ("compute_1", "compute", "bogus", "1"),
    ("storage_2", "metadata", "sessions_max", "16"),
    (
        "compute_2",
        "cluster",
        "system_address",
        "\"tcp://10.0.2.1:7101\"",
    ),
    ("storage_1", "system", "data_dir", "\"/data/storage\""),
    ("compute_3", "cluster", "id", "8"),
    ("storage_3", "storage", "read_retain_ms", "0"),
    (
        "frontend",
        "cluster",
        "frontend_address",
        "\"tcp://frontend:7081\"",
    ),
];

/// The setting the refusal of `planted` names: the table alone for a moved table held where it does not belong.
fn refused_setting(planted: &(&str, &str, &str, &str)) -> String {
    let (_, table, key, _) = planted;
    if *table == "metadata" {
        (*table).to_string()
    } else {
        format!("{table}.{key}")
    }
}

/// L7.5: 1 to 4 independent defects planted in an accepted target, each of a drawn check (Parsing, System, System address,
/// Data, Identity, Bounds, an old setting): the refusal lists every one, each naming its target, its instance, its setting
/// and its rule, and no other.
#[hegel::test]
fn test_every_planted_defect_is_listed_and_no_other(tc: TestCase) {
    let mut instances: Instances = valid(3, 3);
    let mut chosen: Vec<&(&str, &str, &str, &str)> =
        PLANTED.iter().filter(|_| coin(&tc)).take(4).collect();
    if chosen.is_empty() {
        let at = usize::try_from(within(&tc, 0, 6)).unwrap_or_default();
        chosen.extend(PLANTED.get(at));
    }
    for (instance, table, key, value) in &chosen {
        of(&mut instances, instance).bare(table, key, value);
    }
    let found = defects(&instances, None);
    let mut expected: Vec<(String, String)> = chosen
        .iter()
        .map(|planted| (planted.0.to_string(), refused_setting(planted)))
        .collect();
    let mut listed: Vec<(String, String)> = found.iter().map(|defect| named(defect)).collect();
    expected.sort();
    listed.sort();
    assert_eq!(listed, expected, "{found:?}");
    for defect in &found {
        let rule = defect.rsplit_once("; ").map_or("", |(_, rule)| rule);
        assert!(
            defect.contains("target ci, ") && !rule.is_empty(),
            "{defect}"
        );
    }
}

/// L7.6: an accepted target with `cluster.frontend_address`, `frontend.role_address` or `frontend.data_dir` added to a drawn
/// file: it is refused naming the setting as unknown.
#[hegel::test]
fn test_an_old_setting_is_refused_as_unknown(tc: TestCase) {
    let (computes, storages) = (within(&tc, 0, 3), within(&tc, 1, 8));
    let mut instances = valid(computes, storages);
    let names: Vec<String> = instances.keys().cloned().collect();
    let at = usize::try_from(within(&tc, 0, 99)).unwrap_or_default() % names.len().max(1);
    let name = names.get(at).cloned().unwrap_or_default();
    let (table, key, value) = match within(&tc, 0, 2) {
        0 => ("cluster", "frontend_address", "tcp://frontend:7081"),
        1 => ("frontend", "role_address", "tcp://10.0.0.1:7081"),
        _ => ("frontend", "data_dir", "/data"),
    };
    of(&mut instances, &name).text(table, key, value);
    let found = defects(&instances, None);
    let naming = format!("instance {name}, setting {table}.{key}: no such setting exists");
    assert!(
        found.iter().any(|defect| defect.contains(&naming)),
        "{naming} in {found:?}"
    );
}
