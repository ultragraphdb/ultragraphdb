// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"encoding/json"
	"fmt"
	"math"
	"math/rand"
	"os"
	"path/filepath"

	"github.com/golang/geo/s1"
	"github.com/golang/geo/s2"
)

// coverConfig is one RegionCoverer setting.
type s2gCoverConfig struct {
	MinLevel int `json:"min_level"`
	MaxLevel int `json:"max_level"`
	LevelMod int `json:"level_mod"`
	MaxCells int `json:"max_cells"`
}

var s2gCoverConfigs = []s2gCoverConfig{
	{5, 16, 0, 18}, // Dgraph's MinCellLevel, MaxCellLevel, LevelMod 0, MaxCells
	{5, 16, 0, 1}, {5, 16, 0, 4}, {5, 16, 0, 8}, {5, 16, 0, 10}, {5, 16, 0, 15}, {5, 16, 0, 30},
	{0, 30, 1, 8}, {3, 20, 2, 12}, {2, 24, 3, 20}, {0, 10, 1, 4}, {7, 7, 1, 5},
	// A negative MaxCells reaches normalizeCovering's re-covering of the cell union.
	{0, 30, 1, -20000},
}

func (c s2gCoverConfig) coverer() *s2.RegionCoverer {
	return &s2.RegionCoverer{MinLevel: c.MinLevel, MaxLevel: c.MaxLevel, LevelMod: c.LevelMod, MaxCells: c.MaxCells}
}

type s2gCovering struct {
	Config s2gCoverConfig `json:"config"`
	Count  int            `json:"count"`
	Hash   string         `json:"hash"`             // FNV-1a 64 over the tokens, each followed by ','
	Tokens []string       `json:"tokens,omitempty"` // only for coverings of at most 64 cells
}

func s2gCoveringOf(cfg s2gCoverConfig, cu s2.CellUnion) s2gCovering {
	t := s2gTokens(cu)
	h := uint64(14695981039346656037)
	for _, s := range t {
		for _, b := range []byte(s + ",") {
			h ^= uint64(b)
			h *= 1099511628211
		}
	}
	c := s2gCovering{Config: cfg, Count: len(t), Hash: fmt.Sprintf("%016x", h)}
	if len(t) <= 64 {
		c.Tokens = t
	}
	return c
}

// loopSpec says how the Rust test must build the loop, in the same call order as here.
type s2gLoopSpec struct {
	Name string `json:"name"`
	// Ring is the GeoJSON outer ring as [lng, lat] degrees, closing point included.
	Ring [][2]string `json:"ring,omitempty"`
	// Regular loop parameters.
	Center   [3]string `json:"center,omitempty"`
	Radius   string    `json:"radius,omitempty"`
	Vertices int       `json:"num_vertices_in,omitempty"`
	// PoisonCover: run coverings[0] (which builds the index) and then Invert before the rest.
	PoisonCover bool `json:"poison_cover,omitempty"`
}

type s2gLoopCase struct {
	Spec          s2gLoopSpec   `json:"spec"`
	Reversed      bool          `json:"reversed"`
	Inverted      bool          `json:"inverted"`
	NumVertices   int           `json:"num_vertices"`
	NumEdges      int           `json:"num_edges"`
	VertexHash    string        `json:"vertex_hash"`
	Vertex0       [3]string     `json:"vertex0"`
	OriginInside  bool          `json:"origin_inside"`
	RectBound     [4]string     `json:"rect_bound"`
	CapBound      s2gCap        `json:"cap_bound"`
	CapRadiusDeg  string        `json:"cap_radius_deg"`
	Points        [][3]string   `json:"points"`
	ContainsStale []bool        `json:"contains_stale"`
	CellUnionBnd  []string      `json:"cell_union_bound"`
	Coverings     []s2gCovering `json:"coverings"`
	ProbeCells    []string      `json:"probe_cells"`
	ContainsCell  []bool        `json:"contains_cell"`
	IntersectCell []bool        `json:"intersects_cell"`
	ContainsFresh []bool        `json:"contains_fresh"`
}

func s2gVertexHash(l *s2.Loop) string {
	h := uint64(14695981039346656037)
	for _, v := range l.Vertices() {
		for _, x := range []float64{v.X, v.Y, v.Z} {
			b := math.Float64bits(x)
			for k := 0; k < 8; k++ {
				h ^= (b >> (8 * k)) & 0xff
				h *= 1099511628211
			}
		}
	}
	return fmt.Sprintf("%016x", h)
}

// isClockwise is Dgraph's planar shoelace test (types/s2index.go).
func s2gIsClockwise(ring [][2]float64) bool {
	var a float64
	n := len(ring)
	for i := 0; i < n; i++ {
		p1 := ring[i]
		p2 := ring[(i+1)%n]
		a += (p2[0] - p1[0]) * (p1[1] + p2[1])
	}
	return a > 0
}

// loopFromRing is Dgraph's loopFromPolygon (types/s2index.go) for the outer ring.
func s2gLoopFromRing(ring [][2]float64) (l *s2.Loop, reversed, inverted bool) {
	reversed = s2gIsClockwise(ring)
	n := len(ring)
	pts := make([]s2.Point, n-1)
	for i := 0; i < n-1; i++ {
		c := ring[i]
		if reversed {
			c = ring[n-1-i]
		}
		pts[i] = s2.PointFromLatLng(s2.LatLngFromDegrees(c[1], c[0]))
	}
	l = s2.LoopFromPoints(pts)
	if l.CapBound().Radius().Degrees() > 90 {
		l.Invert()
		inverted = true
	}
	return l, reversed, inverted
}

func s2gQueryPoints(r *rand.Rand, l *s2.Loop) []s2.Point {
	var pts []s2.Point
	c := l.CapBound()
	center := c.Center()
	for i := 0; i < 12; i++ {
		pts = append(pts, s2gInterpolate(center, s2gRandomPoint(r), r.Float64()*math.Min(1, 1.3*c.Radius().Radians())))
	}
	vs := l.Vertices()
	if len(vs) <= 48 {
		pts = append(pts, vs...)
	} else {
		for i := 0; i < 16; i++ {
			pts = append(pts, vs[r.Intn(len(vs))])
		}
	}
	for i := 0; i < 8; i++ {
		k := r.Intn(len(vs))
		pts = append(pts, s2gInterpolate(l.Vertex(k), l.Vertex(k+1), r.Float64()))
		pts = append(pts, s2gInterpolate(l.Vertex(k), l.Vertex(k+1), 0.5))
		pts = append(pts, s2gPerturb(r, vs[r.Intn(len(vs))], 3))
	}
	for i := 0; i < 6; i++ {
		pts = append(pts, s2gRandomPoint(r))
	}
	pts = append(pts, s2gPt(0, 0, 1), s2gPt(0, 0, -1), s2.OriginPoint(), center)
	return pts
}

func s2gProbeCells(r *rand.Rand, dgraphCover s2.CellUnion, l *s2.Loop) []s2.CellID {
	var cells []s2.CellID
	for i, c := range dgraphCover {
		if i >= 12 {
			break
		}
		cells = append(cells, c)
		if c.Level() > 0 {
			cells = append(cells, c.Parent(c.Level()-1))
		}
		if !c.IsLeaf() {
			ch := c.Children()
			cells = append(cells, ch[:]...)
			cells = append(cells, c.ChildBeginAtLevel(min(c.Level()+4, 30)))
		}
		for _, n := range c.EdgeNeighbors() {
			cells = append(cells, n)
		}
	}
	for i := 0; i < 8; i++ {
		v := l.Vertex(r.Intn(l.NumVertices()))
		cells = append(cells, s2.CellIDFromLatLng(s2.LatLngFromPoint(v)).Parent(r.Intn(31)))
	}
	for f := 0; f < 6; f++ {
		cells = append(cells, s2.CellIDFromFace(f))
	}
	for i := 0; i < 6; i++ {
		cells = append(cells, s2gRandomCellID(r, r.Intn(12)))
	}
	return cells
}

func s2gRunLoop(r *rand.Rand, spec s2gLoopSpec, l *s2.Loop, reversed, inverted bool) s2gLoopCase {
	if spec.PoisonCover {
		s2gCoverConfigs[0].coverer().Covering(l)
		l.Invert()
		inverted = !inverted
	}
	lc := s2gLoopCase{Spec: spec, Reversed: reversed, Inverted: inverted}
	lc.NumVertices = l.NumVertices()
	lc.NumEdges = l.NumEdges()
	lc.VertexHash = s2gVertexHash(l)
	lc.Vertex0 = s2gP(l.Vertex(0))
	lc.OriginInside = l.ContainsOrigin()
	lc.RectBound = s2gR(l.RectBound())
	lc.CapBound = s2gCapOf(l.CapBound())
	lc.CapRadiusDeg = s2gF(l.CapBound().Radius().Degrees())
	pts := s2gQueryPoints(r, l)
	for _, p := range pts {
		lc.Points = append(lc.Points, s2gP(p))
		lc.ContainsStale = append(lc.ContainsStale, l.ContainsPoint(p))
	}
	lc.CellUnionBnd = s2gIds(l.CellUnionBound())
	var dgraphCover s2.CellUnion
	for i, cfg := range s2gCoverConfigs {
		cu := cfg.coverer().Covering(l)
		if i == 0 {
			dgraphCover = cu
		}
		lc.Coverings = append(lc.Coverings, s2gCoveringOf(cfg, cu))
	}
	for _, c := range s2gProbeCells(r, dgraphCover, l) {
		cell := s2.CellFromCellID(c)
		lc.ProbeCells = append(lc.ProbeCells, s2gC(c))
		lc.ContainsCell = append(lc.ContainsCell, l.ContainsCell(cell))
		lc.IntersectCell = append(lc.IntersectCell, l.IntersectsCell(cell))
	}
	for _, p := range pts {
		lc.ContainsFresh = append(lc.ContainsFresh, l.ContainsPoint(p))
	}
	return lc
}

func s2gRingSpec(name string, ring [][2]float64) s2gLoopSpec {
	s := s2gLoopSpec{Name: name}
	for _, c := range ring {
		s.Ring = append(s.Ring, [2]string{s2gF(c[0]), s2gF(c[1])})
	}
	return s
}

func s2gRingLoop(r *rand.Rand, name string, ring [][2]float64, poison bool) s2gLoopCase {
	spec := s2gRingSpec(name, ring)
	spec.PoisonCover = poison
	l, rev, inv := s2gLoopFromRing(ring)
	return s2gRunLoop(r, spec, l, rev, inv)
}

func s2gRegularLoop(r *rand.Rand, name string, center s2.Point, radius s1.Angle, n int, poison bool) s2gLoopCase {
	spec := s2gLoopSpec{Name: name, Center: s2gP(center), Radius: s2gF(radius.Radians()), Vertices: n, PoisonCover: poison}
	return s2gRunLoop(r, spec, s2.RegularLoop(center, radius, n), false, false)
}

type s2gGeoJSON struct {
	Geometry struct {
		Type        string          `json:"type"`
		Coordinates json.RawMessage `json:"coordinates"`
	} `json:"geometry"`
}

func s2gReadRings(path string) [][][2]float64 {
	b, err := os.ReadFile(path)
	if err != nil {
		panic(err)
	}
	var g s2gGeoJSON
	if err := json.Unmarshal(b, &g); err != nil {
		panic(err)
	}
	var rings [][][2]float64
	switch g.Geometry.Type {
	case "Polygon":
		var c [][][2]float64
		if err := json.Unmarshal(g.Geometry.Coordinates, &c); err != nil {
			panic(err)
		}
		rings = append(rings, c[0])
	case "MultiPolygon":
		var c [][][][2]float64
		if err := json.Unmarshal(g.Geometry.Coordinates, &c); err != nil {
			panic(err)
		}
		for _, p := range c {
			rings = append(rings, p[0])
		}
	}
	return rings
}

// starRing returns a closed planar star-shaped ring around (lng, lat) in degrees.
func s2gStarRing(r *rand.Rand, lng, lat, size float64, n int, clockwise bool) [][2]float64 {
	var ring [][2]float64
	for i := 0; i < n; i++ {
		t := 2 * math.Pi * (float64(i) + 0.8*r.Float64()) / float64(n)
		if clockwise {
			t = -t
		}
		rad := size * (0.3 + 0.7*r.Float64())
		x := lng + rad*math.Cos(t)
		y := math.Max(-89.9, math.Min(89.9, lat+rad*math.Sin(t)))
		ring = append(ring, [2]float64{x, y})
	}
	return append(ring, ring[0])
}

func s2gGenLoops(dir, testdata string) {
	r := rand.New(rand.NewSource(5))
	var dg []s2gLoopCase
	for _, name := range []string{"zip", "aruba", "sudan"} {
		rings := s2gReadRings(filepath.Join(testdata, name+".json"))
		dg = append(dg, s2gRingLoop(r, name, rings[0], false))
	}
	dg = append(dg, s2gRingLoop(r, "aruba-poison", s2gReadRings(filepath.Join(testdata, "aruba.json"))[0], true))
	dg = append(dg, s2gRingLoop(r, "zip-poison", s2gReadRings(filepath.Join(testdata, "zip.json"))[0], true))
	// Dgraph's near query: point (-122.082506, 37.4249518), 1000 m, 100 vertices.
	near := s2.PointFromLatLng(s2.LatLngFromDegrees(37.4249518, -122.082506))
	dg = append(dg, s2gRegularLoop(r, "near-1000m", near, s1.Angle(1000.0/6371000.0), 100, false))
	dg = append(dg, s2gRegularLoop(r, "near-50km", near, s1.Angle(50000.0/6371000.0), 100, false))
	s2gWriteJSON(dir, "loops_testdata.json", dg)

	var us []s2gLoopCase
	for i, ring := range s2gReadRings(filepath.Join(testdata, "us.json")) {
		us = append(us, s2gRingLoop(r, fmt.Sprintf("us-%d", i), ring, false))
	}
	s2gWriteJSON(dir, "loops_us.json", us)

	var rnd []s2gLoopCase
	for i := 0; i < 160; i++ {
		var center s2.Point
		switch i % 10 {
		case 0:
			center = s2gPt(0, 0, 1)
		case 1:
			center = s2gPt(0, 0, -1)
		case 2:
			center = s2.PointFromLatLng(s2.LatLngFromDegrees(r.Float64()*20-10, 180))
		default:
			center = s2gRandomPoint(r)
		}
		var radius float64
		switch i % 4 {
		case 0:
			radius = math.Ldexp(r.Float64(), -10-r.Intn(15))
		case 1:
			radius = r.Float64() * 0.2
		case 2:
			radius = r.Float64() * math.Pi / 2
		default:
			radius = r.Float64() * 3
		}
		n := 3 + r.Intn(120)
		rnd = append(rnd, s2gRegularLoop(r, fmt.Sprintf("regular-%d", i), center, s1.Angle(radius), n, i%23 == 7))
	}
	for i := 0; i < 160; i++ {
		lng := r.Float64()*340 - 170
		lat := r.Float64()*160 - 80
		size := []float64{0.001, 0.05, 1, 10, 40}[i%5]
		n := 3 + r.Intn(60)
		rnd = append(rnd, s2gRingLoop(r, fmt.Sprintf("star-%d", i), s2gStarRing(r, lng, lat, size, n, i%3 == 0), i%29 == 11))
	}
	// Large lat/lng boxes, some of which Dgraph inverts.
	boxes := [][4]float64{{-80, 80, -170, 170}, {-10, 10, -179, 179}, {0, 89, -180, 180}, {-89, 89, -100, 100}, {20, 30, 170, 190}}
	for i, b := range boxes {
		ring := [][2]float64{{b[2], b[0]}, {b[3], b[0]}, {b[3], b[1]}, {b[2], b[1]}, {b[2], b[0]}}
		rnd = append(rnd, s2gRingLoop(r, fmt.Sprintf("box-%d", i), ring, false))
		rev := [][2]float64{{b[2], b[0]}, {b[2], b[1]}, {b[3], b[1]}, {b[3], b[0]}, {b[2], b[0]}}
		rnd = append(rnd, s2gRingLoop(r, fmt.Sprintf("box-rev-%d", i), rev, false))
	}
	s2gWriteJSON(dir, "loops_random.json", rnd)

	s2gGenRects(dir, r, append(append(dg, rnd...), us[:40]...))
}

type s2gRectCase struct {
	A         [4]string `json:"a"`
	B         [4]string `json:"b"`
	Intersect bool      `json:"intersects"`
	Contains  bool      `json:"contains"`
	Union     [4]string `json:"union"`
	UnionFull bool      `json:"union_is_full"`
}

type s2gRectSingle struct {
	R          [4]string   `json:"r"`
	IsEmpty    bool        `json:"is_empty"`
	IsFull     bool        `json:"is_full"`
	CapBound   s2gCap      `json:"cap_bound"`
	Expanded   [4]string   `json:"expand_for_subregions"`
	Points     [][3]string `json:"points"`
	ContainsPt []bool      `json:"contains_point"`
	CellUnion  []string    `json:"cell_union_bound"`
}

type s2gRectGolden struct {
	Pairs   []s2gRectCase   `json:"pairs"`
	Singles []s2gRectSingle `json:"singles"`
}

func s2gParseHex(s string) float64 {
	var u uint64
	fmt.Sscanf(s, "%x", &u)
	return math.Float64frombits(u)
}

func s2gRectFrom(h [4]string) s2.Rect {
	var r s2.Rect
	r.Lat.Lo, r.Lat.Hi, r.Lng.Lo, r.Lng.Hi = s2gParseHex(h[0]), s2gParseHex(h[1]), s2gParseHex(h[2]), s2gParseHex(h[3])
	return r
}

func s2gGenRects(dir string, r *rand.Rand, loops []s2gLoopCase) {
	var rects []s2.Rect
	for _, l := range loops {
		rects = append(rects, s2gRectFrom(l.RectBound))
	}
	rects = append(rects, s2.FullRect(), s2.EmptyRect(),
		s2.RectFromLatLng(s2.LatLngFromDegrees(10, 20)),
		s2.RectFromLatLng(s2.LatLngFromDegrees(90, 0)),
		s2.Rect{Lat: rects[0].Lat, Lng: s1.Interval{Lo: 3, Hi: -3}})
	var g s2gRectGolden
	for i := 0; i < 2000; i++ {
		a := rects[r.Intn(len(rects))]
		b := rects[r.Intn(len(rects))]
		if i%10 == 0 {
			b = a
		}
		u := a.Union(b)
		g.Pairs = append(g.Pairs, s2gRectCase{s2gR(a), s2gR(b), a.Intersects(b), a.Contains(b), s2gR(u), u.IsFull()})
	}
	for _, a := range rects {
		s := s2gRectSingle{R: s2gR(a), IsEmpty: a.IsEmpty(), IsFull: a.IsFull(), CapBound: s2gCapOf(a.CapBound()),
			Expanded: s2gR(s2.ExpandForSubregions(a)), CellUnion: s2gIds(a.CellUnionBound())}
		for k := 0; k < 8; k++ {
			var p s2.Point
			if k < 4 && !a.IsEmpty() {
				p = s2.PointFromLatLng(a.Vertex(k))
			} else {
				p = s2gRandomPoint(r)
			}
			s.Points = append(s.Points, s2gP(p))
			s.ContainsPt = append(s.ContainsPt, a.ContainsPoint(p))
		}
		g.Singles = append(g.Singles, s)
	}
	s2gWriteJSON(dir, "rects.json", g)
}

type s2gCapCase struct {
	Center        [3]string   `json:"center"`
	Angle         string      `json:"angle"`
	Cap           s2gCap      `json:"cap"`
	CellUnionBnd  []string    `json:"cell_union_bound"`
	Points        [][3]string `json:"points"`
	ContainsPoint []bool      `json:"contains_point"`
	AddPoint      []s2gCap    `json:"add_point"` // cumulative AddPoint over Points
	Other         s2gCap      `json:"other"`
	AddCap        s2gCap      `json:"add_cap"`
	Complement    s2gCap      `json:"complement"`
}

func s2gGenCaps(dir string) {
	r := rand.New(rand.NewSource(6))
	var caps []s2gCapCase
	for i := 0; i < 300; i++ {
		center := s2gRandomPoint(r)
		if i%17 == 0 {
			center = s2gPt(0, 0, 1)
		}
		var angle float64
		switch i % 6 {
		case 0:
			angle = math.Ldexp(r.Float64(), -5-r.Intn(25))
		case 1:
			angle = r.Float64() * 0.1
		case 2:
			angle = r.Float64() * 1.5
		case 3:
			angle = r.Float64() * math.Pi
		case 4:
			angle = -r.Float64()
		default:
			angle = 1000.0 / 6371000.0 * (1 + 100*r.Float64())
		}
		c := s2.CapFromCenterAngle(center, s1.Angle(angle))
		cc := s2gCapCase{Center: s2gP(center), Angle: s2gF(angle), Cap: s2gCapOf(c), CellUnionBnd: s2gIds(c.CellUnionBound())}
		acc := c
		for k := 0; k < 10; k++ {
			p := s2gInterpolate(center, s2gRandomPoint(r), r.Float64()*math.Min(1, 1.5*math.Abs(angle)))
			cc.Points = append(cc.Points, s2gP(p))
			cc.ContainsPoint = append(cc.ContainsPoint, c.ContainsPoint(p))
			acc = acc.AddPoint(p)
			cc.AddPoint = append(cc.AddPoint, s2gCapOf(acc))
		}
		o := s2.CapFromCenterAngle(s2gRandomPoint(r), s1.Angle(r.Float64()*2-0.2))
		cc.Other = s2gCapOf(o)
		cc.AddCap = s2gCapOf(c.AddCap(o))
		cc.Complement = s2gCapOf(c.Complement())
		caps = append(caps, cc)
	}
	s2gWriteJSON(dir, "caps.json", caps)
}
