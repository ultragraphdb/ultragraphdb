// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

// Command golden writes golden vectors from Dgraph's Go source for the Rust port's tests.
//
// Each package registers its generators with register() in a file of its own (keys.go, codec.go, ...). A generator
// returns its cases; this command wraps them with the Dgraph commit and writes
// <crate>/testdata/golden/<name>.json under the repository root.
//
// Usage, from the repository root: scripts/golden.sh [name ...]   (all generators when none is named)
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

type generator struct {
	crate string
	fn    func() (any, error)
}

var generators = map[string]generator{}

// register adds a generator writing <crate>/testdata/golden/<name>.json.
func register(name, crate string, fn func() (any, error)) {
	if _, dup := generators[name]; dup {
		panic("duplicate golden generator " + name)
	}
	generators[name] = generator{crate: crate, fn: fn}
}

type file struct {
	DgraphCommit string `json:"dgraph_commit"`
	Generator    string `json:"generator"`
	Cases        any    `json:"cases"`
}

func main() {
	root, err := filepath.Abs("../..")
	check(err)
	fork := os.Getenv("DGRAPH_SRC")
	if fork == "" {
		fork = filepath.Join(root, "..", "..", "fork", "dgraph")
	}
	out, err := exec.Command("git", "-C", fork, "rev-parse", "HEAD").Output()
	check(err)
	commit := strings.TrimSpace(string(out))

	names := os.Args[1:]
	if len(names) == 0 {
		for n := range generators {
			names = append(names, n)
		}
		sort.Strings(names)
	}
	for _, name := range names {
		g, ok := generators[name]
		if !ok {
			check(fmt.Errorf("no golden generator named %q", name))
		}
		cases, err := g.fn()
		check(err)
		dir := filepath.Join(root, g.crate, "testdata", "golden")
		check(os.MkdirAll(dir, 0o755))
		// Compact, not indented: the vectors run to tens of megabytes, and indentation more than doubles them.
		data, err := json.Marshal(file{DgraphCommit: commit, Generator: "tools/golden " + name, Cases: cases})
		check(err)
		path := filepath.Join(dir, name+".json")
		check(os.WriteFile(path, append(data, '\n'), 0o644))
		fmt.Printf("wrote %s\n", path)
	}
}

func check(err error) {
	if err != nil {
		fmt.Fprintln(os.Stderr, "golden:", err)
		os.Exit(1)
	}
}
