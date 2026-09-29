//! Contour lines by marching squares, joined into polylines and thinned.

use std::collections::HashMap;

pub struct Contour {
    pub level: f32,
    /// Pixel coordinates in the grid the contour was traced on (cell centres at +0.5).
    pub pts: Vec<(f32, f32)>,
    pub closed: bool,
}

/// Trace every multiple of `interval` through the grid.
pub fn trace(z: &[f32], w: usize, h: usize, interval: f32, tolerance: f32) -> Vec<Contour> {
    // Segments per level, each end identified by the grid edge it sits on.
    let mut per_level: HashMap<i32, Vec<(u64, u64, (f32, f32), (f32, f32))>> = HashMap::new();
    let hedge = |x: usize, y: usize| -> u64 { 2 * (y * w + x) as u64 };
    let vedge = |x: usize, y: usize| -> u64 { 2 * (y * w + x) as u64 + 1 };
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let a = z[y * w + x];
            let b = z[y * w + x + 1];
            let c = z[(y + 1) * w + x + 1];
            let d = z[(y + 1) * w + x];
            if a.is_nan() || b.is_nan() || c.is_nan() || d.is_nan() {
                continue;
            }
            let lo = a.min(b).min(c).min(d);
            let hi = a.max(b).max(c).max(d);
            let k0 = (lo / interval).ceil() as i32;
            let k1 = (hi / interval).floor() as i32;
            for k in k0..=k1 {
                // Nudge the level off exact grid values so no corner sits on the line.
                let lv = k as f32 * interval + 0.0137;
                let (ia, ib, ic, id) = (a > lv, b > lv, c > lv, d > lv);
                let case = (ia as u8) | (ib as u8) << 1 | (ic as u8) << 2 | (id as u8) << 3;
                if case == 0 || case == 15 {
                    continue;
                }
                let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
                let top = (hedge(x, y), (xf + (lv - a) / (b - a), yf));
                let right = (vedge(x + 1, y), (xf + 1.0, yf + (lv - b) / (c - b)));
                let bottom = (hedge(x, y + 1), (xf + (lv - d) / (c - d), yf + 1.0));
                let left = (vedge(x, y), (xf, yf + (lv - a) / (d - a)));
                let mut push = |p: (u64, (f32, f32)), q: (u64, (f32, f32))| {
                    per_level.entry(k).or_default().push((p.0, q.0, p.1, q.1));
                };
                match case {
                    1 | 14 => push(left, top),
                    2 | 13 => push(top, right),
                    3 | 12 => push(left, right),
                    4 | 11 => push(right, bottom),
                    6 | 9 => push(top, bottom),
                    7 | 8 => push(left, bottom),
                    5 | 10 => {
                        let centre = (a + b + c + d) / 4.0 > lv;
                        if (case == 5) == centre {
                            push(left, bottom);
                            push(top, right);
                        } else {
                            push(left, top);
                            push(right, bottom);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let mut out = Vec::new();
    let mut keys: Vec<i32> = per_level.keys().cloned().collect();
    keys.sort();
    for k in keys {
        let segs = &per_level[&k];
        let mut at: HashMap<u64, Vec<usize>> = HashMap::with_capacity(segs.len() * 2);
        for (i, s) in segs.iter().enumerate() {
            at.entry(s.0).or_default().push(i);
            at.entry(s.1).or_default().push(i);
        }
        let mut used = vec![false; segs.len()];
        let walk = |start: usize, from_edge: u64, used: &mut Vec<bool>| -> (Vec<(f32, f32)>, bool) {
            let mut pts = Vec::new();
            let mut cur = start;
            let mut edge = from_edge;
            let s = segs[cur];
            pts.push(if s.0 == edge { s.2 } else { s.3 });
            loop {
                used[cur] = true;
                let s = segs[cur];
                let (next_edge, p) = if s.0 == edge { (s.1, s.3) } else { (s.0, s.2) };
                pts.push(p);
                edge = next_edge;
                let nxt = at[&edge].iter().cloned().find(|&j| !used[j]);
                match nxt {
                    Some(j) => cur = j,
                    None => return (pts, edge == from_edge),
                }
            }
        };
        // Open lines first: they start on an edge that only one segment touches.
        for i in 0..segs.len() {
            if used[i] {
                continue;
            }
            let s = segs[i];
            let start_edge = if at[&s.0].len() == 1 {
                Some(s.0)
            } else if at[&s.1].len() == 1 {
                Some(s.1)
            } else {
                None
            };
            if let Some(e) = start_edge {
                let (pts, _) = walk(i, e, &mut used);
                out.push(Contour { level: k as f32 * interval, pts: simplify(&pts, tolerance), closed: false });
            }
        }
        for i in 0..segs.len() {
            if used[i] {
                continue;
            }
            let (pts, closed) = walk(i, segs[i].0, &mut used);
            out.push(Contour { level: k as f32 * interval, pts: simplify(&pts, tolerance), closed });
        }
    }
    out.retain(|c| c.pts.len() >= 2);
    out
}

/// Douglas-Peucker thinning.
pub fn simplify(pts: &[(f32, f32)], tol: f32) -> Vec<(f32, f32)> {
    if pts.len() < 3 || tol <= 0.0 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (ax, ay) = pts[a];
        let (bx, by) = pts[b];
        let (dx, dy) = (bx - ax, by - ay);
        let l = (dx * dx + dy * dy).sqrt();
        let mut worst = (0f32, a);
        for i in a + 1..b {
            let (px, py) = pts[i];
            let d = if l < 1e-6 { ((px - ax).powi(2) + (py - ay).powi(2)).sqrt() } else { ((px - ax) * dy - (py - ay) * dx).abs() / l };
            if d > worst.0 {
                worst = (d, i);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

pub fn length(pts: &[(f32, f32)]) -> f32 {
    pts.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum()
}
