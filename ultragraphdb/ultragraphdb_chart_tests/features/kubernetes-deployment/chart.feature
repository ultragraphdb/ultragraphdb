# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The chart's objects as Helm renders them
  openspec/changes/stage-5-9-compute-role/specs/kubernetes-deployment/spec.md: its scenarios a rendered chart decides, and
  what its requirements state of the chart's objects, corner case C6.36 among them. The chart is rendered by the Helm of rules_helm's toolchain as the
  release `ultragraphdb` in its namespace, as `scripts/cluster_local.sh` renders it, unless a scenario names another release.
  The scenarios of a running cluster are bound over the built binaries in `ultragraphdb_tests`.

  Scenario: A pod runs unprivileged
    When any role's pod is inspected
    Then it runs as its image's user, 65532 for a role and 65534 for the config service, with a read-only root filesystem, no capability and no privilege escalation

  Scenario: Compute's pod holds no bootstrap secret
    When compute's pod is inspected
    Then it mounts the role secret and no bootstrap secret, and holds no volume claim

  Scenario: No secret is in the repository
    When the files under `deploy/helm/` are searched
    Then none holds a key, a secret or a password

  Scenario: The chart renders the workloads of one cluster
    When the chart is rendered as the release `ultragraphdb`
    Then the config service and the storage instance are StatefulSets of one replica, each holding its data directory on a volume of its own, compute is a StatefulSet of one replica with no volume, each StatefulSet has a headless Service publishing not-ready addresses, and compute's client endpoint has a Service

  Scenario: Each compute instance is a StatefulSet of its own
    When the chart is rendered with the compute instances `compute` and `compute_2`
    Then each is a StatefulSet of one replica with no volume under the headless Service `compute`, its pod mounting the role secret and no bootstrap secret, and one client Service spans every compute pod

  Scenario: Compute's container listens at no role address
    When compute's pod is inspected
    Then its container declares the client endpoint's port and its observability port, and no other

  Scenario: The hosting storage pod holds the system group
    When the storage pod is inspected
    Then its Service and its container declare the port `system.role_address` names, the group's logs lie on its volume, and it alone of every pod mounts the bootstrap secret

  Scenario Outline: Every role reaches the system group at the hosting pod's stable name
    When the chart is rendered as the release `<release>` into the namespace `<namespace>` under the domain `<domain>` with <partitions> partitions
    Then every role's `cluster.system_address` and the host's `system.role_address` name the hosting storage pod's stable name at the system group's port, and the host holds <partitions> partitions

    Examples:
      | release      | namespace    | domain           | partitions |
      | ultragraphdb | ultragraphdb | cluster.local    | 1          |
      | graphs       | graphs       | example.internal | 2          |

  Scenario: Each role receives its bootstrap variables and nothing else
    When storage's and compute's pods are inspected
    Then each one's environment holds the four bootstrap variables of a service over clear text and no other, naming the config service's Service in the release's namespace, the profile's target, the pod's own instance and the read key's file
