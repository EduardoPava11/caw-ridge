//! Vector data: roads, water, boundaries and names. Three sources, one feature model.

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::Value;

use crate::cog::{agent, Res};
use crate::config::{BBox, OVERPASS};

#[derive(Clone, Debug)]
pub enum Geom {
    Point(f64, f64),
    Line(Vec<(f64, f64)>),
    /// Outer ring first, holes after.
    Poly(Vec<Vec<(f64, f64)>>),
}

#[derive(Clone, Debug)]
pub struct Feature {
    pub geom: Geom,
    pub props: BTreeMap<String, String>,
}

impl Feature {
    pub fn get(&self, k: &str) -> &str {
        self.props.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn has(&self, k: &str) -> bool {
        self.props.contains_key(k)
    }
}

fn props_of(v: &Value) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Some(o) = v.as_object() {
        for (k, val) in o {
            let s = match val {
                Value::String(s) => s.trim().to_string(),
                Value::Null => continue,
                other => other.to_string(),
            };
            if !s.is_empty() {
                m.insert(k.clone(), s);
            }
        }
    }
    m
}

fn ring(v: &Value) -> Vec<(f64, f64)> {
    v.as_array()
        .map(|a| a.iter().filter_map(|p| Some((p[0].as_f64()?, p[1].as_f64()?))).collect())
        .unwrap_or_default()
}

/// GeoJSON, with multi geometries flattened into one feature per part.
pub fn parse_geojson(text: &str) -> Res<Vec<Feature>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    for f in v["features"].as_array().cloned().unwrap_or_default() {
        let props = props_of(&f["properties"]);
        let g = &f["geometry"];
        let c = &g["coordinates"];
        match g["type"].as_str().unwrap_or("") {
            "Point" => out.push(Feature { geom: Geom::Point(c[0].as_f64().unwrap_or(0.0), c[1].as_f64().unwrap_or(0.0)), props }),
            "LineString" => out.push(Feature { geom: Geom::Line(ring(c)), props }),
            "MultiLineString" => {
                for l in c.as_array().cloned().unwrap_or_default() {
                    out.push(Feature { geom: Geom::Line(ring(&l)), props: props.clone() });
                }
            }
            "Polygon" => out.push(Feature { geom: Geom::Poly(c.as_array().map(|a| a.iter().map(ring).collect()).unwrap_or_default()), props }),
            "MultiPolygon" => {
                for p in c.as_array().cloned().unwrap_or_default() {
                    out.push(Feature { geom: Geom::Poly(p.as_array().map(|a| a.iter().map(ring).collect()).unwrap_or_default()), props: props.clone() });
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Overpass JSON written with `out tags geom`.
pub fn parse_overpass(text: &str) -> Res<Vec<Feature>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    for e in v["elements"].as_array().cloned().unwrap_or_default() {
        let mut props = props_of(&e["tags"]);
        props.insert("osm_id".into(), e["id"].to_string());
        match e["type"].as_str().unwrap_or("") {
            "node" => {
                if let (Some(lon), Some(lat)) = (e["lon"].as_f64(), e["lat"].as_f64()) {
                    out.push(Feature { geom: Geom::Point(lon, lat), props });
                }
            }
            "way" => {
                let pts: Vec<(f64, f64)> = e["geometry"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|p| Some((p["lon"].as_f64()?, p["lat"].as_f64()?))).collect())
                    .unwrap_or_default();
                if pts.len() < 2 {
                    continue;
                }
                let closed = pts.len() > 3 && pts[0] == pts[pts.len() - 1];
                let area = closed && (props.contains_key("landuse") || props.get("natural").map(|s| s == "water").unwrap_or(false));
                out.push(Feature { geom: if area { Geom::Poly(vec![pts]) } else { Geom::Line(pts) }, props });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The federal geographical names service.
pub fn parse_names(text: &str) -> Res<Vec<Feature>> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = Vec::new();
    for i in v["items"].as_array().cloned().unwrap_or_default() {
        let (Some(lat), Some(lon)) = (i["latitude"].as_f64(), i["longitude"].as_f64()) else { continue };
        let mut props = BTreeMap::new();
        props.insert("name".to_string(), i["name"].as_str().unwrap_or("").to_string());
        props.insert("code".to_string(), i["concise"]["code"].as_str().unwrap_or("").to_string());
        props.insert("generic".to_string(), i["generic"]["code"].as_str().unwrap_or("").to_string());
        props.insert("id".to_string(), i["id"].as_str().unwrap_or("").to_string());
        out.push(Feature { geom: Geom::Point(lon, lat), props });
    }
    Ok(out)
}

pub fn fetch_arcgis(layer_url: &str, b: BBox) -> Res<String> {
    let url = format!(
        "{layer_url}/query?geometry={},{},{},{}&geometryType=esriGeometryEnvelope&inSR=4326&spatialRel=esriSpatialRelIntersects&outFields=*&outSR=4326&f=geojson&geometryPrecision=5",
        b.w, b.s, b.e, b.n
    );
    let v = crate::cog::get(&url)?;
    let s = String::from_utf8(v)?;
    if !s.contains("\"features\"") {
        return Err(format!("no features in reply from {layer_url}").into());
    }
    Ok(s)
}

pub fn fetch_names(b: BBox) -> Res<String> {
    let url = format!("https://geogratis.gc.ca/services/geoname/en/geonames.json?bbox={},{},{},{}&num=1000", b.w, b.s, b.e, b.n);
    Ok(String::from_utf8(crate::cog::get(&url)?)?)
}

pub fn fetch_overpass(b: BBox) -> Res<String> {
    let bb = format!("({},{},{},{})", b.s, b.w, b.n, b.e);
    let mut q = String::from("[out:json][timeout:120];(");
    for sel in [
        "way[\"highway\"]", "way[\"waterway\"]", "way[\"natural\"=\"water\"]", "node[\"place\"]",
        "node[\"natural\"=\"peak\"]", "way[\"landuse\"]", "node[\"tourism\"]", "way[\"railway\"]",
        "way[\"power\"=\"line\"]", "way[\"aeroway\"]", "node[\"man_made\"=\"tower\"]", "node[\"amenity\"=\"fuel\"]",
        "node[\"amenity\"=\"hospital\"]", "way[\"amenity\"=\"hospital\"]", "node[\"barrier\"=\"gate\"]",
        "node[\"ford\"]", "way[\"bridge\"]", "node[\"mountain_pass\"]",
    ] {
        q.push_str(sel);
        q.push_str(&bb);
        q.push(';');
    }
    q.push_str(");out tags geom;");
    let mut last: Option<Box<dyn std::error::Error + Send + Sync>> = None;
    for round in 0..3 {
        for host in OVERPASS {
            eprintln!("  asking {host}");
            match agent().post(host).send_form([("data", q.as_str())]) {
                Ok(mut r) => match r.body_mut().with_config().limit(1 << 28).read_to_string() {
                    Ok(s) if s.trim_start().starts_with('{') => return Ok(s),
                    Ok(_) => last = Some("reply was not JSON".into()),
                    Err(e) => last = Some(Box::new(e)),
                },
                Err(e) => last = Some(Box::new(e)),
            }
            std::thread::sleep(Duration::from_secs(5 + 10 * round));
        }
    }
    Err(last.unwrap())
}

/// Even-odd point in polygon, holes included.
pub fn in_poly(rings: &[Vec<(f64, f64)>], x: f64, y: f64) -> bool {
    let mut inside = false;
    for r in rings {
        let n = r.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (x1, y1) = r[i];
            let (x2, y2) = r[j];
            if (y1 > y) != (y2 > y) && x < (x2 - x1) * (y - y1) / (y2 - y1) + x1 {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

/// Nearest point on a set of rings or a line to (lon, lat): distance in metres and bearing.
pub fn nearest_on(pts: &[(f64, f64)], lon: f64, lat: f64) -> (f64, f64, (f64, f64)) {
    let kx = 111320.0 * lat.to_radians().cos();
    let ky = 111200.0;
    let mut best = (f64::MAX, 0.0, (lon, lat));
    for w in pts.windows(2) {
        let (ax, ay) = ((w[0].0 - lon) * kx, (w[0].1 - lat) * ky);
        let (bx, by) = ((w[1].0 - lon) * kx, (w[1].1 - lat) * ky);
        let (dx, dy) = (bx - ax, by - ay);
        let l = dx * dx + dy * dy;
        let t = if l == 0.0 { 0.0 } else { (-(ax * dx + ay * dy) / l).clamp(0.0, 1.0) };
        let (px, py) = (ax + t * dx, ay + t * dy);
        let d = (px * px + py * py).sqrt();
        if d < best.0 {
            best = (d, (px.atan2(py).to_degrees() + 360.0) % 360.0, (lon + px / kx, lat + py / ky));
        }
    }
    best
}
