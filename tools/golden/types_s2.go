// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_types' port of golang/geo's S2 (src/s2/): the generators in
// types_s2_*.go call golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, the version Dgraph uses. Floats are the 16 hex
// digits of their IEEE-754 bits.
//
// Run with GOFLAGS=-gcflags=all=-d=fmahash=n: on arm64 Go fuses a*b+c into one instruction, which changes the
// last bit of S2's arithmetic; the flag turns that off in every package (std math included), so the vectors are
// what Go computes on amd64, which the Rust port reproduces.

import (
	"path/filepath"
	"sync"
)

var (
	s2gOutputs = map[string]any{}
	s2gOnce    sync.Once
)

func s2gRun(file string) func() (any, error) {
	return func() (any, error) {
		s2gOnce.Do(func() {
			testdata := filepath.Join(forkDir(), "types", "testdata")
			s2gGenMath("")
			s2gGenLatLng("")
			s2gGenCellID("")
			s2gGenPredicates("")
			s2gGenCaps("")
			s2gGenLoops("", testdata)
		})
		return []any{s2gOutputs[file]}, nil // one case: the file holds a slice of cases
	}
}

func init() {
	for _, n := range []string{"gomath", "latlng", "cellid", "predicates", "caps", "rects", "loops_testdata",
		"loops_us", "loops_random"} {
		register("types_s2_"+n, "ultragraphdb_core_types", s2gRun(n+".json"))
	}
}
