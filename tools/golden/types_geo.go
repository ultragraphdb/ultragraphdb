// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_types' geo values (types/s2.go, s2index.go): index tokens and GeoJSON
// decoding, through the fork's exported functions. Parent tokens come out of a Go map, so every token list is sorted.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"

	geom "github.com/twpayne/go-geom"
	"github.com/twpayne/go-geom/encoding/geojson"

	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("types_geo_index", "ultragraphdb_core_types", genTypesGeoIndex)
	register("types_geojson", "ultragraphdb_core_types", genTypesGeoJSON)
}

func goldenGeoms() []geom.T {
	return []geom.T{
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-122.082506, 37.4249518}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1, 2}),
		geom.NewPoint(geom.XYZ).MustSetCoords(geom.Coord{1, 2, 3}),
		geom.NewPoint(geom.XYM).MustSetCoords(geom.Coord{1, 2, 4}),
		geom.NewPoint(geom.XYZM).MustSetCoords(geom.Coord{1, 2, 3, 4}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1e-7, 1e21}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{0.1, -0.000001}),
		geom.NewLineString(geom.XY).MustSetCoords([]geom.Coord{{0, 0}, {1, 1}, {2, 0.5}}),
		geom.NewLineString(geom.XY),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{
			{{-122, 37}, {-123, 37}, {-123, 38}, {-122, 38}, {-122, 37}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{
			{{0, 0}, {10, 0}, {10, 10}, {0, 10}, {0, 0}}, {{2, 2}, {2, 4}, {4, 4}, {2, 2}}}),
		geom.NewMultiPoint(geom.XY).MustSetCoords([]geom.Coord{{1, 2}, {3, 4}}),
		geom.NewMultiLineString(geom.XYZ).MustSetCoords([][]geom.Coord{{{1, 2, 3}, {4, 5, 6}, {7, 8, 9}, {1, 2, 3}}}),
		geom.NewMultiPolygon(geom.XY).MustSetCoords([][][]geom.Coord{
			{{{-112, 39}, {-113, 39}, {-113, 40}, {-112, 40}, {-112, 39}}},
			{{{71.09, 42.35}, {72.09, 42.35}, {72.09, 41.35}, {71.09, 41.35}, {71.09, 42.35}}}}),
		geom.NewGeometryCollection().MustPush(geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{1, 2}),
			geom.NewLineString(geom.XY).MustSetCoords([]geom.Coord{{0, 0}, {1, 1}})),
	}
}

func loadFeature(name string) (geom.T, error) {
	b, err := os.ReadFile(filepath.Join(forkDir(), "types", "testdata", name))
	if err != nil {
		return nil, err
	}
	var f geojson.Feature
	if err := f.UnmarshalJSON(b); err != nil {
		return nil, err
	}
	return f.Geometry, nil
}

func forkDir() string {
	if d := os.Getenv("DGRAPH_SRC"); d != "" {
		return d
	}
	root, _ := filepath.Abs("../..")
	return filepath.Join(root, "..", "..", "fork", "dgraph")
}

func sorted(t []string) []string {
	s := append([]string(nil), t...)
	sort.Strings(s)
	return s
}

type geoIndexCase struct {
	Name   string   `json:"name"`
	Wkb    string   `json:"wkb"`
	Tokens []string `json:"tokens,omitempty"`
	Err    string   `json:"err,omitempty"`
}

func indexInputs() ([]string, []geom.T, error) {
	names := []string{}
	geoms := []geom.T{}
	for _, f := range []string{"zip.json", "aruba.json", "sudan.json", "us.json"} {
		g, err := loadFeature(f)
		if err != nil {
			return nil, nil, err
		}
		names = append(names, f)
		geoms = append(geoms, g)
		if mp, ok := g.(*geom.MultiPolygon); ok {
			for i := 0; i < mp.NumPolygons(); i += 7 {
				names = append(names, f+"#"+itoa(i))
				geoms = append(geoms, mp.Polygon(i))
			}
		}
	}
	for i, g := range goldenGeoms() {
		names = append(names, "golden#"+itoa(i))
		geoms = append(geoms, g)
	}
	extra := []geom.T{
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 37}, {-122, 38}, {-123, 38}, {-123, 37}, {-122, 37}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-122, 37}, {-123, 37}, {-123, 38}, {-122, 38}, {-122, 38}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{1, 1}, {2, 2}, {1, 1}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-179, -10}, {179, -10}, {179, 10}, {-179, 10}, {-179, -10}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{0, 80}, {120, 80}, {-120, 80}, {0, 80}}}),
		geom.NewPolygon(geom.XY).MustSetCoords([][]geom.Coord{{{-10, -10}, {10, -10}, {10, 10}, {-10, 10}, {-10, -10}}}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{180, 90}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-180, -90}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{0, 0}),
		geom.NewPoint(geom.XY).MustSetCoords(geom.Coord{-157.9197, 21.33}),
	}
	for i, g := range extra {
		names = append(names, "extra#"+itoa(i))
		geoms = append(geoms, g)
	}
	return names, geoms, nil
}

func itoa(i int) string {
	b, _ := json.Marshal(i)
	return string(b)
}

func genTypesGeoIndex() (any, error) {
	names, geoms, err := indexInputs()
	if err != nil {
		return nil, err
	}
	var cases []geoIndexCase
	for i, g := range geoms {
		c := geoIndexCase{Name: names[i], Wkb: h(wkbOf(g))}
		toks, err := types.IndexGeoTokens(g)
		if err != nil {
			c.Err = err.Error()
		} else {
			c.Tokens = sorted(toks)
		}
		cases = append(cases, c)
	}
	return cases, nil
}

type geoJSONCase struct {
	In  string `json:"in"` // hex: some inputs are not UTF-8
	Err string `json:"err,omitempty"`
	Nil bool   `json:"nil,omitempty"`
	Out string `json:"out,omitempty"` // the geometry as geojson.Marshal writes it
}

// genTypesGeoJSON records go-geom's geojson.Unmarshal, Go's json.Unmarshal into geojson.Geometry, of valid
// geometries, every syntax error context of encoding/json and the type errors of the struct's fields.
func genTypesGeoJSON() (any, error) {
	inputs := []string{
		`{"type":"Point","coordinates":[1,2]}`,
		`{"type":"Point","coordinates":[1.5,-2.25,3]}`,
		`{"type":"Point","coordinates":[]}`,
		`{"type":"Point"}`,
		`{"type":"Point","coordinates":null}`,
		`{"type":"LineString","coordinates":[[0,0],[1,1]]}`,
		`{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,0]]]}`,
		`{"type":"MultiPoint","coordinates":[[1,2],null,[3,4]]}`,
		`{"type":"MultiLineString","coordinates":[[[1,2],[3,4]]]}`,
		`{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,0]]]]}`,
		`{"type":"GeometryCollection","geometries":[{"type":"Point","coordinates":[1,2]},null]}`,
		`{"TYPE":"Point","Coordinates":[3,4]}`,
		`{"type":"Point","type":"LineString","coordinates":[[0,0],[1,1]]}`,
		`{"type":"Point","coordinates":[1,2],"bbox":[0,0,1,1],"crs":{"type":"name"},"extra":1}`,
		`null`,
		` {"type":"Point","coordinates":[1,2]} `,
		// Type errors.
		`"Point"`,
		`[1,2]`,
		`{"type":1,"coordinates":[1,2]}`,
		`{"type":1,"crs":2}`,
		`{"crs":2,"type":1}`,
		`{"type":1,"type":"Point","coordinates":[1,2]}`,
		`{"type":"Point","crs":"x"}`,
		`{"type":"Point","crs":{"type":1}}`,
		`{"type":"Point","crs":{"properties":1}}`,
		`{"type":"Point","coordinates":"x"}`,
		`{"type":"Point","coordinates":[1,"x"]}`,
		`{"type":"Point","coordinates":[1,1e400]}`,
		`{"type":"LineString","coordinates":[[0,0],"x"]}`,
		`{"type":"Polygon","coordinates":[[[0,0],[1,true]]]}`,
		`{"type":"GeometryCollection","geometries":"x"}`,
		`{"type":"GeometryCollection","geometries":[1]}`,
		`{"type":"GeometryCollection","geometries":[{"type":1}]}`,
		`{"type":"Curve","coordinates":[1,2]}`,
		`{"type":"Point","coordinates":[1]}`,
		`{}`,
		// Syntax errors.
		``,
		` `,
		`x`,
		`{`,
		`{"type"`,
		`{"type":`,
		`{"type":"Point"`,
		`{"type":"Point",`,
		`{1}`,
		`{"type" 1}`,
		`{"type":"Point" "x"}`,
		`{"type":"Point"]`,
		`{"type":"Point",}`,
		`{"type":"Point","coordinates":[1 2]}`,
		`{"type":"Point","coordinates":[1,]}`,
		`{"type":"Point","coordinates":[1,2}`,
		`{"type":"Point","coordinates":[1.]}`,
		`{"type":"Point","coordinates":[1.x]}`,
		`{"type":"Point","coordinates":[1e]}`,
		`{"type":"Point","coordinates":[-]}`,
		`{"type":"Point","coordinates":[01]}`,
		`{"type":tru}`,
		`{"type":"\x"}`,
		`{"type":"\u12x4"}`,
		"{\"type\":\"a\x01\"}",
		"{\"type\":\"a\xffb\"}",
		"\xff",
		"\xef\xbb\xbf{}",
		`{"type":"Point","coordinates":[1,2]}x`,
		`{"type":"Point","coordinates":[1,2]} {}`,
		strings.Repeat("[", 10001) + strings.Repeat("]", 10001),
		`{"type":"Point","coordinates":` + strings.Repeat("[", 10000) + strings.Repeat("]", 10000) + `}`,
		`{"type":"Point","coordinates":` + strings.Repeat("[", 9999) + strings.Repeat("]", 9999) + `}`,
	}
	var cases []geoJSONCase
	for _, in := range inputs {
		c := geoJSONCase{In: h([]byte(in))}
		var g geom.T
		if err := geojson.Unmarshal([]byte(in), &g); err != nil {
			c.Err = err.Error()
		} else if g == nil {
			c.Nil = true
		} else {
			b, err := geojson.Marshal(g)
			if err != nil {
				return nil, err
			}
			c.Out = string(b)
		}
		cases = append(cases, c)
	}
	return cases, nil
}
