#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# Make will use bash instead of sh
SHELL := /usr/bin/env bash

.PHONY: help
help:
	@echo ' Development:'
	@echo '    make build   	Builds the code base incrementally (fast) for dev.'
	@echo '    make check   	Checks the code base for security vulnerabilities.'
	@echo '    make count   	Count LoC across the project.'
	@echo '    make doc   		Builds documentation for the project.'
	@echo '    make fix   		Fixes linting issues as reported by clippy.'
	@echo '    make format   	Formats all code according to cargo fmt style.'
	@echo '    make update   	Update rust, pulls from git remote.'
	@echo '    make test   	Tests across all crates.'
	@echo ''
	@echo ' Local Kubernetes (Apple container, macOS on Apple silicon):'
	@echo '    make cluster-local   	Creates the cluster, loads the images and deploys deploy/helm/ultragraphdb (CPUS=12 MEMORY=16g PARTITIONS=1).'
	@echo '    make cluster-local-bench	Runs client_bench as a Job in the cluster (BENCH="--op direct --connections 16 --seconds 20").'
	@echo '    make cluster-local-status	Shows the cluster, its pods and the I/O driver each role runs on.'
	@echo '    make cluster-local-down	Deletes the cluster.'

.PHONY: build
build:
	@source scripts/build.sh

.PHONY: check
check:
	@source scripts/check.sh

.PHONY: count
count:
	@source scripts/count.sh

.PHONY: doc
doc:
	@source scripts/doc.sh

.PHONY: fix
fix:
	@source scripts/fix.sh

.PHONY: format
format:
	@source scripts/format.sh

.PHONY: test
test:
	@source scripts/test.sh

.PHONY: update
update:
	@source scripts/update.sh

CPUS ?= 12
MEMORY ?= 16g
PARTITIONS ?= 1
BENCH ?=

.PHONY: cluster-local
cluster-local:
	@source scripts/cluster_local.sh up --cpus $(CPUS) --memory $(MEMORY) --partitions $(PARTITIONS)

.PHONY: cluster-local-bench
cluster-local-bench:
	@source scripts/cluster_local.sh bench $(BENCH)

.PHONY: cluster-local-status
cluster-local-status:
	@source scripts/cluster_local.sh status

.PHONY: cluster-local-down
cluster-local-down:
	@source scripts/cluster_local.sh down
