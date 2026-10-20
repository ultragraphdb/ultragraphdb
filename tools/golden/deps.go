// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// The Dgraph packages stage 1 ports, imported once so go.mod and go.sum hold their whole module graph and the
// per-package generator files never need `go mod tidy`.
import (
	_ "github.com/dgraph-io/dgraph/v25/algo"
	_ "github.com/dgraph-io/dgraph/v25/chunker"
	_ "github.com/dgraph-io/dgraph/v25/codec"
	_ "github.com/dgraph-io/dgraph/v25/dql"
	_ "github.com/dgraph-io/dgraph/v25/lex"
	_ "github.com/dgraph-io/dgraph/v25/protos/pb"
	_ "github.com/dgraph-io/dgraph/v25/schema"
	_ "github.com/dgraph-io/dgraph/v25/tok"
	_ "github.com/dgraph-io/dgraph/v25/types"
	_ "github.com/dgraph-io/dgraph/v25/types/facets"
	_ "github.com/dgraph-io/dgraph/v25/x"
)
