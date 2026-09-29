//! Everything the maps and pages draw from, loaded once.

use std::path::Path;
use std::sync::Arc;

use resvg::usvg;

use crate::analysis;
use crate::cog::Res;
use crate::config::{self, BBox};
use crate::dem::SrcDem;
use crate::sat::Sat;
use crate::vector::{self, Feature, Geom};
use crate::view::{Terrain, View};
use crate::wmm;

pub struct World {
    pub dem: SrcDem,
    pub landcover: SrcDem,
    pub sat_ridge: Sat,
    pub sat_region: Sat,
    pub osm: Vec<Feature>,
    pub names: Vec<Feature>,
    pub wmu: Vec<Feature>,
    pub parks: Vec<Feature>,
    pub goat_sheep: Vec<Feature>,
    pub caribou: Vec<Feature>,
    pub grizzly: Vec<Feature>,
    pub coal: Vec<Feature>,
    /// The regional analysis grid: the whole region at 30 m.
    pub ra: View,
    pub rt: Terrain,
    /// Streams traced from the elevation model: lon/lat line and upstream area in km2.
    pub streams: Vec<(Vec<(f64, f64)>, f32)>,
    pub fonts: Arc<usvg::fontdb::Database>,
    pub declination: f64,
    pub convergence: f64,
    pub wpt_elev: f32,
    pub snow: Vec<(String, SrcDem, SrcDem, SrcDem)>,
}

impl World {
    pub fn load() -> Res<World> {
        let data = Path::new("data");
        let v = data.join("vector");
        let read = |n: &str| -> Res<String> { Ok(std::fs::read_to_string(v.join(n))?) };
        eprintln!("loading data");
        let dem = SrcDem::load(data, "dem")?;
        let landcover = SrcDem::load(data, "landcover")?;
        let sat_ridge = Sat::load(&data.join("cache"), "sat_ridge")?;
        let sat_region = Sat::load(&data.join("cache"), "sat_region")?;
        let osm = vector::parse_overpass(&read("osm.json")?)?;
        let names = vector::parse_names(&read("names.json")?)?;
        let wmu = vector::parse_geojson(&read("wmu.geojson")?)?;
        let parks = vector::parse_geojson(&read("parks.geojson")?)?;
        let goat_sheep = vector::parse_geojson(&read("goat_sheep.geojson")?)?;
        let mut caribou = vector::parse_geojson(&read("caribou_rpc.geojson")?)?;
        caribou.extend(vector::parse_geojson(&read("caribou_alp.geojson")?)?);
        let grizzly = vector::parse_geojson(&read("grizzly_core.geojson")?)?;
        let coal = vector::parse_geojson(&read("coal.geojson")?)?;

        eprintln!("building the regional terrain grid");
        let ra = View::new(config::REGION, 30.0);
        let rt = Terrain::new(&ra, ra.elevation(&dem));
        eprintln!("  {} x {} cells", ra.w, ra.h);

        eprintln!("tracing drainage");
        let (down, acc) = analysis::drainage(&rt);
        // Keep the traced streams off the big rivers, which OpenStreetMap maps better.
        let mut mask = vec![false; ra.w * ra.h];
        for f in osm.iter().filter(|f| f.get("waterway") == "river") {
            if let Geom::Line(pts) = &f.geom {
                for w in pts.windows(2) {
                    let (a, b) = (ra.px(w[0].0, w[0].1), ra.px(w[1].0, w[1].1));
                    let n = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1);
                    for i in 0..=n {
                        let t = i as f64 / n as f64;
                        let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                        for dy in -5..=5i64 {
                            for dx in -5..=5i64 {
                                let (xx, yy) = (x as i64 + dx, y as i64 + dy);
                                if xx >= 0 && yy >= 0 && xx < ra.w as i64 && yy < ra.h as i64 {
                                    mask[yy as usize * ra.w + xx as usize] = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut streams = Vec::new();
        for (pts, a) in analysis::streams(&rt, &down, &acc, 0.45) {
            let mut run: Vec<(f64, f64)> = Vec::new();
            for p in pts {
                let m = mask[(p.1 as usize).min(ra.h - 1) * ra.w + (p.0 as usize).min(ra.w - 1)];
                if m {
                    if run.len() >= 2 {
                        streams.push((std::mem::take(&mut run), a));
                    }
                    run.clear();
                } else {
                    run.push(ra.lonlat(p.0 as f64, p.1 as f64));
                }
            }
            if run.len() >= 2 {
                streams.push((run, a));
            }
        }
        eprintln!("  {} stream reaches", streams.len());

        let wpt_elev = dem.sample(config::WPT_LON, config::WPT_LAT);
        let declination = wmm::field(config::WPT_LAT, config::WPT_LON, wpt_elev as f64 / 1000.0, config::FIELD_YEAR).declination;
        let convergence = crate::geo::Utm::new(11).convergence(config::WPT_LON, config::WPT_LAT);

        let mut snow = Vec::new();
        for w in config::SNOW_WINTERS {
            let d = data.join("snow");
            snow.push((
                w.to_string(),
                SrcDem::load(&d, &format!("startF_{w}"))?,
                SrcDem::load(&d, &format!("startF_u_{w}"))?,
                SrcDem::load(&d, &format!("startB_{w}"))?,
            ));
        }

        Ok(World {
            dem,
            landcover,
            sat_ridge,
            sat_region,
            osm,
            names,
            wmu,
            parks,
            goat_sheep,
            caribou,
            grizzly,
            coal,
            ra,
            rt,
            streams,
            fonts: crate::draw::fontdb(),
            declination,
            convergence,
            wpt_elev,
            snow,
        })
    }

    /// The window of the regional grid that covers a box, as (x0, y0, x1, y1).
    pub fn window(&self, b: BBox) -> (usize, usize, usize, usize) {
        let (x0, y0) = self.ra.px(b.w, b.n);
        let (x1, y1) = self.ra.px(b.e, b.s);
        (
            (x0.floor().max(0.0) as usize).min(self.ra.w),
            (y0.floor().max(0.0) as usize).min(self.ra.h),
            (x1.ceil().max(0.0) as usize).min(self.ra.w),
            (y1.ceil().max(0.0) as usize).min(self.ra.h),
        )
    }

    /// Sample a regional grid layer at a lon/lat, bilinear, NaN aware.
    pub fn sample_ra(&self, layer: &[f32], lon: f64, lat: f64) -> f32 {
        let (x, y) = self.ra.px(lon, lat);
        let (x, y) = (x - 0.5, y - 0.5);
        if x < 0.0 || y < 0.0 || x >= (self.ra.w - 1) as f64 || y >= (self.ra.h - 1) as f64 {
            return f32::NAN;
        }
        let (x0, y0) = (x as usize, y as usize);
        let (tx, ty) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
        let i = y0 * self.ra.w + x0;
        let (a, b, c, d) = (layer[i], layer[i + 1], layer[i + self.ra.w], layer[i + self.ra.w + 1]);
        if a.is_nan() || b.is_nan() || c.is_nan() || d.is_nan() {
            // Fall back to the nearest cell so edges of analysis areas stay crisp.
            let n = layer[(y.round() as usize) * self.ra.w + x.round() as usize];
            return n;
        }
        (a * (1.0 - tx) + b * tx) * (1.0 - ty) + (c * (1.0 - tx) + d * tx) * ty
    }
}
