/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_cluster_sequencer::Sequencer;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::harness::{Log, close_through, config, drain, epoch, fresh, pid};

fn epochs(closes: &[ultragraphdb_cluster_sequencer::Close]) -> Vec<u64> {
    closes.iter().map(|c| c.epoch().get()).collect()
}

#[test]
fn test_every_subscriber_receives_every_close_in_order() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    sequencer.subscribe(pid(2)?, epoch(0)?)?;
    let _ms = close_through(&mut sequencer, 3)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(1)?)?), vec![1, 2, 3]);
    let _ms_2 = close_through(&mut sequencer, 5)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(1)?)?), vec![4, 5]);
    assert_eq!(
        epochs(&drain(&mut sequencer, pid(2)?)?),
        vec![1, 2, 3, 4, 5]
    );
    Ok(())
}

#[test]
fn test_a_subscriber_after_an_epoch_receives_the_closes_after_it() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 9)?;
    sequencer.subscribe(pid(4)?, epoch(3)?)?;
    assert_eq!(
        epochs(&drain(&mut sequencer, pid(4)?)?),
        vec![4, 5, 6, 7, 8, 9]
    );
    sequencer.subscribe(pid(5)?, epoch(9)?)?;
    assert_eq!(sequencer.take_delivery(pid(5)?)?, None);
    Ok(())
}

#[test]
fn test_subscribing_again_moves_the_subscription() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 4)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(1)?)?), vec![1, 2, 3, 4]);
    sequencer.subscribe(pid(1)?, epoch(2)?)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(1)?)?), vec![3, 4]);
    Ok(())
}

#[test]
fn test_a_subscription_past_the_last_durable_close_is_refused() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    let _ms = close_through(&mut sequencer, 2)?;
    let refusal = sequencer
        .subscribe(pid(1)?, epoch(3)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::EpochAhead {
            epoch: 3,
            latest: 2
        }
    );
    let unsubscribed = sequencer
        .take_delivery(pid(1)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        unsubscribed.0,
        ClusterErrorEnum::NotSubscribed { partition: 1 }
    );
    Ok(())
}

#[test]
fn test_an_unsubscribed_partition_receives_nothing() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    sequencer.unsubscribe(pid(1)?);
    sequencer.unsubscribe(pid(2)?);
    let _ms = close_through(&mut sequencer, 1)?;
    let refusal = sequencer
        .take_delivery(pid(1)?)
        .err()
        .context("a refusal")?;
    assert_eq!(refusal.0, ClusterErrorEnum::NotSubscribed { partition: 1 });
    Ok(())
}

#[test]
fn test_at_most_1024_partitions_subscribe() -> TestResult {
    let (_, mut sequencer) = fresh(config(1, 2)?)?;
    assert_eq!(Sequencer::<Log>::SUBSCRIBERS_MAX, 1_024);
    for partition in 1..=1_024 {
        sequencer.subscribe(pid(partition)?, epoch(0)?)?;
    }
    let refusal = sequencer
        .subscribe(pid(1_025)?, epoch(0)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::Full {
            what: "subscribers",
            capacity: 1_024
        }
    );
    // A partition already subscribed moves; one leaving makes room.
    sequencer.subscribe(pid(1_024)?, epoch(0)?)?;
    sequencer.unsubscribe(pid(1)?);
    sequencer.subscribe(pid(1_025)?, epoch(0)?)?;
    Ok(())
}

#[test]
fn test_a_subscriber_behind_the_held_closes_is_refused() -> TestResult {
    let three = NonZeroU32::new(3).context("3")?;
    let (_, mut sequencer) = fresh(config(1, 2)?.with_closes_retained(three))?;
    sequencer.subscribe(pid(1)?, epoch(0)?)?;
    sequencer.subscribe(pid(2)?, epoch(0)?)?;
    let _ms = close_through(&mut sequencer, 2)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(2)?)?), vec![1, 2]);
    let _ms_2 = close_through(&mut sequencer, 5)?;
    // Closes 3 to 5 are held: partition 2 catches up, partition 1, after 0, cannot.
    assert_eq!(epochs(&drain(&mut sequencer, pid(2)?)?), vec![3, 4, 5]);
    let refusal = sequencer
        .take_delivery(pid(1)?)
        .err()
        .context("a refusal")?;
    assert_eq!(
        refusal.0,
        ClusterErrorEnum::CatchUpTrimmed {
            after: 0,
            oldest: 3
        }
    );
    sequencer.subscribe(pid(1)?, epoch(2)?)?;
    assert_eq!(epochs(&drain(&mut sequencer, pid(1)?)?), vec![3, 4, 5]);
    Ok(())
}
