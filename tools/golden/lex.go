// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_lex: an item's String (Go's %q quoting of its value), the line and column
// every emitted item carries and the error texts of the lexer, IRIRef and LexQuotedString.

import (
	"github.com/dgraph-io/dgraph/v25/lex"
)

func init() {
	register("lex_item_string", "ultragraphdb_core_lex", genLexItemString)
	register("lex_positions", "ultragraphdb_core_lex", genLexPositions)
	register("lex_iri_ref", "ultragraphdb_core_lex", genLexIRIRef)
	register("lex_quoted_string", "ultragraphdb_core_lex", genLexQuotedString)
}

type lexItemStringCase struct {
	Typ    int    `json:"typ"`
	Val    string `json:"val"`
	String string `json:"string"`
}

// genLexItemString emits each value as one item of each type and records the item's String.
func genLexItemString() (any, error) {
	vals := []string{
		"", "abc", "a\"b", "back\\slash", "'single'", "tab\there", "nl\nx", "\r", "\a\b\f\v",
		"\x00\x01\x1f\x7f", "名前", "émoji 😀", "\U0001F600", "\u00a0nbsp", "\u0085", "\u00ad", "\u200b",
		"\u2028", "\u3000", "\ufeff", "\ue000", "\U000F0000",
	}
	var cases []lexItemStringCase
	for _, typ := range []lex.ItemType{lex.ItemEOF, lex.ItemError, 5, 22} {
		for _, v := range vals {
			var l lex.Lexer
			l.Reset(v)
			for l.Next() != lex.EOF {
			}
			l.Emit(typ)
			it := l.NewIterator()
			it.Next()
			cases = append(cases, lexItemStringCase{Typ: int(typ), Val: v, String: it.Item().String()})
		}
	}
	return cases, nil
}

type lexPositionItem struct {
	Typ    int    `json:"typ"`
	Val    string `json:"val"`
	String string `json:"string"`
	Errorf string `json:"errorf"`
}

type lexPositionCase struct {
	Input    string            `json:"input"`
	Items    []lexPositionItem `json:"items"`
	Validate string            `json:"validate"`
}

// lexPerRune emits every rune as an item of its own, ignores spaces and fails on '!'.
func lexPerRune(l *lex.Lexer) lex.StateFn {
	for {
		switch r := l.Next(); {
		case r == lex.EOF:
			l.Emit(lex.ItemEOF)
			return nil
		case r == ' ':
			l.Ignore()
		case r == '!':
			return l.Errorf("Unexpected %q at depth %d", r, l.Depth)
		default:
			l.Emit(5)
		}
	}
}

// genLexPositions runs lexPerRune and records every item with its String and Errorf, which carry its line and column.
func genLexPositions() (any, error) {
	inputs := []string{
		"", "a b", "ab\ncd", "a\r\nb", "\n\n x", "x\r\ry", "tab\tz", "名前 x\ny", "😀\n😀", "é\n\nü",
		"ab\n!c", "名\n 前!", "!",
	}
	var cases []lexPositionCase
	for _, in := range inputs {
		var l lex.Lexer
		l.Reset(in)
		l.Run(lexPerRune)
		c := lexPositionCase{Input: in, Items: []lexPositionItem{}}
		it := l.NewIterator()
		for it.Next() {
			item := it.Item()
			c.Items = append(c.Items, lexPositionItem{
				Typ:    int(item.Typ),
				Val:    item.Val,
				String: item.String(),
				Errorf: item.Errorf("Unexpected %v here", "x").Error(),
			})
		}
		if err := l.ValidateResult(); err != nil {
			c.Validate = err.Error()
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type lexScanCase struct {
	Input   string   `json:"input"`
	Emitted []string `json:"emitted"`
	Error   string   `json:"error"`
	Pos     int      `json:"pos"`
}

// genLexIRIRef consumes the first rune, as the state functions do before they call IRIRef, and records what IRIRef
// emits, its error and where it leaves the lexer.
func genLexIRIRef() (any, error) {
	inputs := []string{
		"<abc>", "<>", "<abc", "<", "<a b>", "<a\"b>", "<a{b>", "<a}b>", "<a|b>", "<a^b>", "<a`b>", "<a<b>",
		"<\x01>", "<\t>", "<a\x7fb>", "<名前>", "<a\\u1234b>", "<a\\U0001F600>", "<a\\u12>", "<a\\u12g>",
		"<a\\x41>", "<a\\ué>", "<a\\", "<a\\u", "<http://example.org/a#b?c=d>", "<a>rest",
	}
	var cases []lexScanCase
	for _, in := range inputs {
		var l lex.Lexer
		l.Reset(in)
		l.Next()
		err := lex.IRIRef(&l, 5)
		c := lexScanCase{Input: in, Emitted: []string{}, Pos: l.Pos}
		if err != nil {
			c.Error = err.Error()
		}
		it := l.NewIterator()
		for it.Next() {
			c.Emitted = append(c.Emitted, it.Item().Val)
		}
		cases = append(cases, c)
	}
	return cases, nil
}

// genLexQuotedString consumes the first rune, as the state functions do before they call LexQuotedString, and records
// its error and where it leaves the lexer.
func genLexQuotedString() (any, error) {
	inputs := []string{
		`"abc"`, `""`, `"abc`, `"`, `abc"`, ``, `"a\"b"`, `"a\nb"`, `"a\\b"`, `"\u\v\t\b\n\r\f\'"`, `"a\qb"`,
		`"a\`, `"é\é"`, `"名前"rest`, "\"a\nb\"",
	}
	var cases []lexScanCase
	for _, in := range inputs {
		var l lex.Lexer
		l.Reset(in)
		l.Next()
		err := l.LexQuotedString()
		c := lexScanCase{Input: in, Emitted: []string{}, Pos: l.Pos}
		if err != nil {
			c.Error = err.Error()
		}
		cases = append(cases, c)
	}
	return cases, nil
}
