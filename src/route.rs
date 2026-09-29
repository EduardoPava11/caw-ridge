//! Routing over the OpenStreetMap road and track network, and profiles along lines.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use crate::dem::SrcDem;
use crate::geo::haversine;
use crate::vector::{Feature, Geom};

#[derive(Clone, Debug)]
pub struct Leg {
    pub kind: String,
    pub name: String,
    pub pts: Vec<(f64, f64)>,
    pub km: f64,
}

#[derive(PartialEq)]
struct St(f64, u32);
impl Eq for St {}
impl Ord for St {
    fn cmp(&self, o: &Self) -> Ordering {
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for St {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

fn key(p: (f64, f64)) -> (i64, i64) {
    ((p.0 * 1e7).round() as i64, (p.1 * 1e7).round() as i64)
}

pub fn kind_of(f: &Feature) -> Option<&'static str> {
    if f.get("access") == "private" {
        return None;
    }
    match f.get("highway") {
        "primary" | "secondary" | "trunk" | "primary_link" => Some("highway"),
        "tertiary" | "unclassified" | "residential" | "service" => Some("gravel"),
        "track" => Some("track"),
        _ => None,
    }
}

/// Shortest drivable route between the network nodes nearest to two points, as legs that
/// change where the kind of road changes.
pub fn drive(osm: &[Feature], from: (f64, f64), to: (f64, f64)) -> Option<Vec<Leg>> {
    let mut ids: HashMap<(i64, i64), u32> = HashMap::new();
    let mut nodes: Vec<(f64, f64)> = Vec::new();
    let mut adj: Vec<Vec<(u32, f64, u32)>> = Vec::new();
    let mut ways: Vec<(&'static str, String)> = Vec::new();
    for f in osm {
        let Some(kind) = kind_of(f) else { continue };
        let Geom::Line(pts) = &f.geom else { continue };
        // Town streets are paved; everything else of this class out here is gravel.
        let kind = if kind == "gravel" && f.get("surface") == "asphalt" { "highway" } else { kind };
        let wid = ways.len() as u32;
        ways.push((kind, f.get("name").to_string()));
        let mut prev: Option<u32> = None;
        for p in pts {
            let id = *ids.entry(key(*p)).or_insert_with(|| {
                nodes.push(*p);
                adj.push(Vec::new());
                nodes.len() as u32 - 1
            });
            if let Some(a) = prev {
                let d = haversine(nodes[a as usize].0, nodes[a as usize].1, p.0, p.1);
                adj[a as usize].push((id, d, wid));
                adj[id as usize].push((a, d, wid));
            }
            prev = Some(id);
        }
    }
    let nearest = |p: (f64, f64)| -> u32 {
        let mut best = (f64::MAX, 0u32);
        for (i, n) in nodes.iter().enumerate() {
            let d = haversine(n.0, n.1, p.0, p.1);
            if d < best.0 {
                best = (d, i as u32);
            }
        }
        best.1
    };
    let (s, t) = (nearest(from), nearest(to));
    let mut dist = vec![f64::INFINITY; nodes.len()];
    let mut prev: Vec<Option<(u32, u32)>> = vec![None; nodes.len()];
    let mut heap = BinaryHeap::new();
    dist[s as usize] = 0.0;
    heap.push(St(0.0, s));
    while let Some(St(d, u)) = heap.pop() {
        if d > dist[u as usize] {
            continue;
        }
        if u == t {
            break;
        }
        for &(v, w, wid) in &adj[u as usize] {
            // Time weighted: a track is slow, so prefer roads where there is a choice.
            let cost = w * match ways[wid as usize].0 {
                "highway" => 1.0,
                "gravel" => 1.6,
                _ => 4.0,
            };
            let nd = d + cost;
            if nd < dist[v as usize] {
                dist[v as usize] = nd;
                prev[v as usize] = Some((u, wid));
                heap.push(St(nd, v));
            }
        }
    }
    if !dist[t as usize].is_finite() {
        return None;
    }
    let mut chain: Vec<(u32, u32)> = Vec::new();
    let mut c = t;
    while let Some((p, wid)) = prev[c as usize] {
        chain.push((c, wid));
        c = p;
    }
    chain.reverse();
    let mut legs: Vec<Leg> = Vec::new();
    let mut last = nodes[s as usize];
    for (n, wid) in chain {
        let (kind, name) = &ways[wid as usize];
        let p = nodes[n as usize];
        let d = haversine(last.0, last.1, p.0, p.1) / 1000.0;
        let same = legs.last().map(|l| l.kind == *kind && (l.name == *name || name.is_empty() || l.name.is_empty())).unwrap_or(false);
        if same {
            let l = legs.last_mut().unwrap();
            l.pts.push(p);
            l.km += d;
            if l.name.is_empty() {
                l.name = name.clone();
            }
        } else {
            legs.push(Leg { kind: kind.to_string(), name: name.clone(), pts: vec![last, p], km: d });
        }
        last = p;
    }
    Some(legs)
}

#[allow(dead_code)]
pub struct Profile {
    /// Distance along the line in kilometres and elevation in metres.
    pub pts: Vec<(f64, f32)>,
    pub gain: f32,
    pub loss: f32,
    pub min: f32,
    pub max: f32,
}

/// Sample the elevation model along a line every `step_m` metres.
pub fn profile(dem: &SrcDem, line: &[(f64, f64)], step_m: f64, smooth_m: f64) -> Profile {
    let mut pts = Vec::new();
    let mut run = 0.0;
    for w in line.windows(2) {
        let d = haversine(w[0].0, w[0].1, w[1].0, w[1].1);
        let n = (d / step_m).ceil().max(1.0) as usize;
        for i in 0..n {
            let t = i as f64 / n as f64;
            let (lon, lat) = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
            pts.push(((run + d * t) / 1000.0, dem.sample(lon, lat)));
        }
        run += d;
    }
    if let Some(l) = line.last() {
        pts.push((run / 1000.0, dem.sample(l.0, l.1)));
    }
    // Smooth before counting climb, so that cell noise is not counted as hills.
    let k = ((smooth_m / step_m) as usize).max(1);
    let sm: Vec<f32> = (0..pts.len())
        .map(|i| {
            let a = i.saturating_sub(k);
            let b = (i + k).min(pts.len() - 1);
            pts[a..=b].iter().map(|p| p.1).sum::<f32>() / (b - a + 1) as f32
        })
        .collect();
    let (mut gain, mut loss) = (0.0, 0.0);
    for w in sm.windows(2) {
        let d = w[1] - w[0];
        if d > 0.0 { gain += d } else { loss -= d }
    }
    let min = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    Profile { pts, gain, loss, min, max }
}
