/*
 * Copyright (c) 2013, Tom Payne. All rights reserved.
 * SPDX-License-Identifier: BSD-2-Clause
 *
 * Ported from go-geom v1.6.1, xy/cga.go (IsPointInRing), xy/internal/raycrossing/ray-crossing-counter.go and
 * xy/internal/robustdeterminate/robust_determinate.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::ops::ControlFlow;

/// go-geom's `xy.IsPointInRing`: whether `p` lies in the interior or on the boundary of a ring of flat
/// coordinates with the given stride.
pub(crate) fn is_point_in_ring(stride: usize, p: [f64; 2], ring: &[f64]) -> bool {
    let mut counter = RayCrossingCounter {
        p,
        crossing_count: 0,
        on_segment: false,
    };
    if stride < 2 {
        return false;
    }
    // Each point with the one before it; a trailing point short of two coordinates is left out.
    let points = || ring.chunks(stride).filter_map(<[f64]>::first_chunk::<2>);
    for (&p2, &p1) in points().zip(points().skip(1)) {
        counter.count_segment(p1, p2);
        if counter.on_segment {
            return true;
        }
    }
    counter.on_segment || counter.crossing_count % 2 == 1
}

struct RayCrossingCounter {
    p: [f64; 2],
    crossing_count: usize,
    on_segment: bool,
}

impl RayCrossingCounter {
    fn count_segment(&mut self, p1: [f64; 2], p2: [f64; 2]) {
        let p = self.p;
        if p1[0] < p[0] && p2[0] < p[0] {
            return;
        }
        if p[0] == p2[0] && p[1] == p2[1] {
            self.on_segment = true;
            return;
        }
        if p1[1] == p[1] && p2[1] == p[1] {
            let (minx, maxx) = if p1[0] > p2[0] {
                (p2[0], p1[0])
            } else {
                (p1[0], p2[0])
            };
            if p[0] >= minx && p[0] <= maxx {
                self.on_segment = true;
            }
            return;
        }
        if (p1[1] > p[1] && p2[1] <= p[1]) || (p2[1] > p[1] && p1[1] <= p[1]) {
            let (x1, y1) = (p1[0] - p[0], p1[1] - p[1]);
            let (x2, y2) = (p2[0] - p[0], p2[1] - p[1]);
            let mut sign = sign_of_det2x2(x1, y1, x2, y2);
            if sign == 0 {
                self.on_segment = true;
                return;
            }
            if y2 < y1 {
                sign = -sign;
            }
            if sign > 0 {
                self.crossing_count += 1;
            }
        }
    }
}

/// go-geom's `robustdeterminate.SignOfDet2x2`: the exact sign of `x1*y2 - y1*x2`.
fn sign_of_det2x2(x1: f64, y1: f64, x2: f64, y2: f64) -> i32 {
    if let Some(sign) = null_entry_sign(x1, y1, x2, y2) {
        return sign;
    }
    let (sign, x1, y1, x2, y2) = order_by_y(x1, y1, x2, y2);
    match order_by_x(sign, x1, x2) {
        ControlFlow::Break(sign) => sign,
        ControlFlow::Continue((sign, x1, x2)) => reduce(sign, x1, y1, x2, y2),
    }
}

/// The sign where an entry is zero, which needs no arithmetic; `None` where every entry is nonzero.
fn null_entry_sign(x1: f64, y1: f64, x2: f64, y2: f64) -> Option<i32> {
    if x1 == 0.0 || y2 == 0.0 {
        if y1 == 0.0 || x2 == 0.0 {
            return Some(0);
        } else if y1 > 0.0 {
            return Some(if x2 > 0.0 { -1 } else { 1 });
        }
        return Some(if x2 > 0.0 { 1 } else { -1 });
    }
    if y1 == 0.0 || x2 == 0.0 {
        if y2 > 0.0 {
            return Some(if x1 > 0.0 { 1 } else { -1 });
        }
        return Some(if x1 > 0.0 { -1 } else { 1 });
    }
    None
}

/// Makes both y coordinates positive and swaps the rows so that y2 is the larger, with the sign that keeps the
/// determinant's.
fn order_by_y(x1: f64, y1: f64, x2: f64, y2: f64) -> (i32, f64, f64, f64, f64) {
    let (mut x1, mut y1, mut x2, mut y2) = (x1, y1, x2, y2);
    let mut sign = 1;
    if 0.0 < y1 {
        if 0.0 < y2 {
            if y1 > y2 {
                sign = -1;
                std::mem::swap(&mut x1, &mut x2);
                std::mem::swap(&mut y1, &mut y2);
            }
        } else if y1 <= -y2 {
            sign = -1;
            x2 = -x2;
            y2 = -y2;
        } else {
            let swap = x1;
            x1 = -x2;
            x2 = swap;
            let swap_y = y1;
            y1 = -y2;
            y2 = swap_y;
        }
    } else if 0.0 < y2 {
        if -y1 <= y2 {
            sign = -1;
            x1 = -x1;
            y1 = -y1;
        } else {
            let swap = -x1;
            x1 = x2;
            x2 = swap;
            let swap_y = -y1;
            y1 = y2;
            y2 = swap_y;
        }
    } else if y1 >= y2 {
        x1 = -x1;
        y1 = -y1;
        x2 = -x2;
        y2 = -y2;
    } else {
        sign = -1;
        let swap = -x1;
        x1 = -x2;
        x2 = swap;
        let swap_y = -y1;
        y1 = -y2;
        y2 = swap_y;
    }
    (sign, x1, y1, x2, y2)
}

/// Makes both x coordinates positive, or breaks with the sign where the x coordinates alone decide it.
fn order_by_x(sign: i32, x1: f64, x2: f64) -> ControlFlow<i32, (i32, f64, f64)> {
    let (mut sign, mut x1, mut x2) = (sign, x1, x2);
    if 0.0 < x1 {
        if 0.0 < x2 {
            if x1 > x2 {
                return ControlFlow::Break(sign);
            }
        } else {
            return ControlFlow::Break(sign);
        }
    } else {
        if 0.0 < x2 {
            return ControlFlow::Break(-sign);
        }
        if x1 >= x2 {
            sign = -sign;
            x1 = -x1;
            x2 = -x2;
        } else {
            return ControlFlow::Break(-sign);
        }
    }
    ControlFlow::Continue((sign, x1, x2))
}

/// The sign of the determinant of rows with all entries positive, `x1 <= x2` and `y1 <= y2`, by repeated
/// Euclidean reduction.
fn reduce(sign: i32, x1: f64, y1: f64, x2: f64, y2: f64) -> i32 {
    let (mut sign, mut x1, mut y1, mut x2, mut y2) = (sign, x1, y1, x2, y2);
    loop {
        let k = (x2 / x1).floor();
        x2 -= k * x1;
        y2 -= k * y1;
        if y2 < 0.0 {
            return -sign;
        }
        if y2 > y1 {
            return sign;
        }
        if x1 > x2 + x2 {
            if y1 < y2 + y2 {
                return sign;
            }
        } else {
            if y1 > y2 + y2 {
                return -sign;
            }
            x2 = x1 - x2;
            y2 = y1 - y2;
            sign = -sign;
        }
        if y2 == 0.0 {
            return if x2 == 0.0 { 0 } else { -sign };
        }
        if x2 == 0.0 {
            return sign;
        }
        let k1 = (x1 / x2).floor();
        x1 -= k1 * x2;
        y1 -= k1 * y2;
        if y1 < 0.0 {
            return sign;
        }
        if y1 > y2 {
            return -sign;
        }
        if x2 > x1 + x1 {
            if y2 < y1 + y1 {
                return -sign;
            }
        } else {
            if y2 > y1 + y1 {
                return sign;
            }
            x1 = x2 - x1;
            y1 = y2 - y1;
            sign = -sign;
        }
        if y1 == 0.0 {
            return if x1 == 0.0 { 0 } else { sign };
        }
        if x1 == 0.0 {
            return -sign;
        }
    }
}
