# The partition's side of the sequencer capability, verbatim from
# openspec/changes/stage-5-5-three-role-refactor/specs/sequencer/spec.md: the closes a partition is delivered reach its
# commit step as inputs, which stamps its commits with the open epoch and reports the epochs it has closed. The
# sequencer's own catch-up is `ultragraphdb_cluster_sequencer`'s; here a reconnecting partition takes the closes it
# missed in order and refuses one past a gap.

Feature: Partitions learn each closed epoch

  # Requirement: Partitions learn each closed epoch and stamp the open one

  Scenario: A partition stamps the open epoch
    When epochs 1 to 7 are delivered to a partition and it commits a transaction
    Then the commit's version carries epoch 8

  Scenario: A partition stamps no epoch ahead of its closes
    When epochs 1 to 7 are delivered to a partition, the sequencer has closed epoch 8, and its close has not yet reached the partition
    Then the partition's commits still carry epoch 8, and its first commit after the close of 8 arrives carries epoch 9

  Scenario: A partition closes an epoch once its rounds are applied
    When a partition holds an unapplied round of epoch 8 and the close of epoch 8 is delivered
    Then the partition reports epoch 8 closed only after that round is applied, and a snapshot read at epoch 8 waits until then

  Scenario: A reconnecting partition catches up
    When a partition loses its connection after epoch 3 and reconnects after epoch 9 closed
    Then it receives epochs 4 to 9 in order before any later close
