// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"math"
	"math/rand"

	"github.com/golang/geo/r3"
	"github.com/golang/geo/s2"
)

type s2gSignCase struct {
	A      [3]string `json:"a"`
	B      [3]string `json:"b"`
	C      [3]string `json:"c"`
	Sign   bool      `json:"sign"`
	Robust int       `json:"robust"`
	ACV    bool      `json:"angle_contains_vertex"` // AngleContainsVertex(a, b, c), only meaningful if a != b && b != c
}

type s2gCrossCase struct {
	A        [3]string `json:"a"`
	B        [3]string `json:"b"`
	C        [3]string `json:"c"`
	D        [3]string `json:"d"`
	Crossing int       `json:"crossing"`
	Vertex   bool      `json:"vertex"`
	EdgeOrV  bool      `json:"edge_or_vertex"`
	Ordered  bool      `json:"ordered_ccw"` // OrderedCCW(a, b, c, d)
}

type s2gChainCase struct {
	A       [3]string   `json:"a"`
	B       [3]string   `json:"b"`
	Chain   [][3]string `json:"chain"`
	Sign    []int       `json:"chain_crossing_sign"`
	EdgeOrV []bool      `json:"edge_or_vertex_chain"`
	// Calls on a third crosser alternating CrossingSign(c, d) and EdgeOrVertexCrossing(c, d)
	// over consecutive pairs of the chain, with occasional explicit RestartAt.
	Mixed []int `json:"mixed"`
}

type s2gPredGolden struct {
	Signs    []s2gSignCase  `json:"signs"`
	Crossing []s2gCrossCase `json:"crossings"`
	Chains   []s2gChainCase `json:"chains"`
}

func s2gSignOf(a, b, c s2.Point) s2gSignCase {
	return s2gSignCase{s2gP(a), s2gP(b), s2gP(c), s2.Sign(a, b, c), int(s2.RobustSign(a, b, c)), s2.AngleContainsVertex(a, b, c)}
}

func s2gCrossOf(a, b, c, d s2.Point) s2gCrossCase {
	return s2gCrossCase{s2gP(a), s2gP(b), s2gP(c), s2gP(d), int(s2.CrossingSign(a, b, c, d)), s2.VertexCrossing(a, b, c, d),
		s2.EdgeOrVertexCrossing(a, b, c, d), s2.OrderedCCW(a, b, c, d)}
}

func s2gPt(x, y, z float64) s2.Point { return s2.Point{Vector: r3.Vector{X: x, Y: y, Z: z}} }

func s2gGenPredicates(dir string) {
	r := rand.New(rand.NewSource(4))
	var g s2gPredGolden

	// Exactly collinear and symbolic-perturbation cases, as in golang/geo's predicates_test.
	exact := [][3]s2.Point{
		{s2gPt(1, 0, 0), s2gPt(0, 1, 0), s2gPt(0, 0, 1)},
		{s2gPt(1, 0, 0), s2gPt(0, 0, 1), s2gPt(0, 1, 0)},
		{s2gPt(0, 0, 1), s2gPt(1, 0, 0), s2gPt(0, 1, 0)},
		{s2gPt(1, 0, 0), s2gPt(1, 0, 0), s2gPt(0, 1, 0)},
		{s2gPt(1, 0, 0), s2gPt(-1, 0, 0), s2gPt(0, 1, 0)},
		{s2gPt(1, 0, 0), s2gPt(0, 1, 0), s2gPt(-1, 0, 0)},
		{s2gPt(1, 0, 0), s2gPt(0, 1, 0), s2gPt(0, -1, 0)},
		{s2gPt(0, 0, 1), s2gPt(0, 0, -1), s2gPt(1, 0, 0)},
		{s2gPt(-3, -1, 4), s2gPt(2, -1, -3), s2gPt(1, -2, 0)},
		{s2gPt(-6, 3, 3), s2gPt(-4, 2, -1), s2gPt(-2, 1, 4)},
		{s2gPt(0, -1, -1), s2gPt(0, 1, -2), s2gPt(0, 2, 1)},
		{s2gPt(-1, 2, 7), s2gPt(2, 1, -4), s2gPt(4, 2, -8)},
		{s2gPt(-4, -2, 7), s2gPt(2, 1, -4), s2gPt(4, 2, -8)},
		{s2gPt(0, -5, 7), s2gPt(0, -4, 8), s2gPt(0, -2, 4)},
		{s2gPt(-5, -2, 7), s2gPt(0, 0, -2), s2gPt(0, 0, -1)},
		{s2gPt(0, -2, 7), s2gPt(0, 0, 1), s2gPt(0, 0, 2)},
		{s2gPt(1, 1, 0), s2gPt(-1, -1, 0), s2gPt(1, 1, 0)},
		{s2gPt(0, 0, 0), s2gPt(0, 1, 0), s2gPt(0, 0, 1)},
	}
	for _, t := range exact {
		g.Signs = append(g.Signs, s2gSignOf(t[0], t[1], t[2]))
		g.Signs = append(g.Signs, s2gSignOf(t[1], t[2], t[0]))
		g.Signs = append(g.Signs, s2gSignOf(t[2], t[1], t[0]))
	}
	for i := 0; i < 3000; i++ {
		var a, b, c s2.Point
		switch i % 8 {
		case 0, 1:
			a, b, c = s2gRandomPoint(r), s2gRandomPoint(r), s2gRandomPoint(r)
		case 2:
			// Nearly collinear: c on the chord between a and b.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c = s2gInterpolate(a, b, r.Float64())
		case 3:
			// Collinear points very close together.
			a = s2gRandomPoint(r)
			b = s2gPerturb(r, a, 1000)
			b = s2.Point{Vector: b.Normalize()}
			c = s2gInterpolate(a, b, r.Float64()*3-1)
		case 4:
			// Shared vertices.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c = a
			if r.Intn(2) == 0 {
				c = b
			}
		case 5:
			// Points on the equator or a meridian: exact determinant zero.
			t1, t2, t3 := r.Float64()*2*math.Pi, r.Float64()*2*math.Pi, r.Float64()*2*math.Pi
			if r.Intn(2) == 0 {
				a, b, c = s2gPt(math.Cos(t1), math.Sin(t1), 0), s2gPt(math.Cos(t2), math.Sin(t2), 0), s2gPt(math.Cos(t3), math.Sin(t3), 0)
			} else {
				a, b, c = s2gPt(math.Cos(t1), 0, math.Sin(t1)), s2gPt(math.Cos(t2), 0, math.Sin(t2)), s2gPt(math.Cos(t3), 0, math.Sin(t3))
			}
		case 6:
			// Antipodal pairs.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c = s2.Point{Vector: a.Mul(-1)}
		default:
			// Tiny perturbations of a collinear triple.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c = s2gPerturb(r, s2gInterpolate(a, b, 0.5), 2)
		}
		g.Signs = append(g.Signs, s2gSignOf(a, b, c))
	}

	for i := 0; i < 3000; i++ {
		var a, b, c, d s2.Point
		switch i % 8 {
		case 0, 1:
			a, b, c, d = s2gRandomPoint(r), s2gRandomPoint(r), s2gRandomPoint(r), s2gRandomPoint(r)
		case 2:
			// Short crossing edges.
			m := s2gRandomPoint(r)
			a, b = s2gPerturb(r, m, 1<<20), s2gPerturb(r, m, 1<<20)
			c, d = s2gPerturb(r, m, 1<<20), s2gPerturb(r, m, 1<<20)
			a, b, c, d = s2.Point{Vector: a.Normalize()}, s2.Point{Vector: b.Normalize()}, s2.Point{Vector: c.Normalize()}, s2.Point{Vector: d.Normalize()}
		case 3:
			// Shared vertex.
			a, b, c = s2gRandomPoint(r), s2gRandomPoint(r), s2gRandomPoint(r)
			switch r.Intn(4) {
			case 0:
				d = a
			case 1:
				d = b
			case 2:
				c, d = a, s2gRandomPoint(r)
			default:
				c, d = b, a
			}
		case 4:
			// Collinear on the equator.
			t := make([]float64, 4)
			for k := range t {
				t[k] = r.Float64() * math.Pi
			}
			a, b, c, d = s2gPt(math.Cos(t[0]), math.Sin(t[0]), 0), s2gPt(math.Cos(t[1]), math.Sin(t[1]), 0), s2gPt(math.Cos(t[2]), math.Sin(t[2]), 0), s2gPt(math.Cos(t[3]), math.Sin(t[3]), 0)
		case 5:
			// Degenerate edges.
			a, c, d = s2gRandomPoint(r), s2gRandomPoint(r), s2gRandomPoint(r)
			b = a
			if r.Intn(2) == 0 {
				b, d = s2gRandomPoint(r), c
			}
		case 6:
			// Crossing at a tiny angle.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c = s2gPerturb(r, s2gInterpolate(a, b, -0.1), 4)
			d = s2gPerturb(r, s2gInterpolate(a, b, 1.1), 4)
		default:
			// Identical edges, both directions.
			a, b = s2gRandomPoint(r), s2gRandomPoint(r)
			c, d = a, b
			if r.Intn(2) == 0 {
				c, d = b, a
			}
		}
		g.Crossing = append(g.Crossing, s2gCrossOf(a, b, c, d))
	}

	for i := 0; i < 600; i++ {
		var a, b s2.Point
		a, b = s2gRandomPoint(r), s2gRandomPoint(r)
		if i%5 == 0 {
			b = s2.Point{Vector: s2gPerturb(r, a, 1<<30).Normalize()}
		}
		n := 2 + r.Intn(30)
		var chain []s2.Point
		center := s2gInterpolate(a, b, 0.5)
		for k := 0; k < n; k++ {
			var p s2.Point
			switch r.Intn(6) {
			case 0:
				p = a
			case 1:
				p = b
			case 2:
				if len(chain) > 0 {
					p = chain[r.Intn(len(chain))]
				} else {
					p = s2gRandomPoint(r)
				}
			case 3:
				p = s2gInterpolate(a, b, r.Float64()*1.4-0.2)
			case 4:
				p = s2.Point{Vector: s2gPerturb(r, center, 1<<40).Normalize()}
			default:
				p = s2gRandomPoint(r)
			}
			chain = append(chain, p)
		}
		cc := s2gChainCase{A: s2gP(a), B: s2gP(b)}
		for _, p := range chain {
			cc.Chain = append(cc.Chain, s2gP(p))
		}
		e1 := s2.NewChainEdgeCrosser(a, b, chain[0])
		e2 := s2.NewChainEdgeCrosser(a, b, chain[0])
		for _, p := range chain[1:] {
			cc.Sign = append(cc.Sign, int(e1.ChainCrossingSign(p)))
			cc.EdgeOrV = append(cc.EdgeOrV, e2.EdgeOrVertexChainCrossing(p))
		}
		e3 := s2.NewEdgeCrosser(a, b)
		for k := 0; k+1 < len(chain); k++ {
			if k%7 == 3 {
				e3.RestartAt(chain[k])
			}
			if k%2 == 0 {
				cc.Mixed = append(cc.Mixed, int(e3.CrossingSign(chain[k], chain[k+1])))
			} else if e3.EdgeOrVertexCrossing(chain[k], chain[k+1]) {
				cc.Mixed = append(cc.Mixed, 1)
			} else {
				cc.Mixed = append(cc.Mixed, 0)
			}
		}
		g.Chains = append(g.Chains, cc)
	}
	s2gWriteJSON(dir, "predicates.json", g)
}
