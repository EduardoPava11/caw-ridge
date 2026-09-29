//! The map sheets: what is drawn, in what order, and in which colours.

use std::fmt::Write as _;

use crate::config::{self, BBox};
use crate::contour::{self, Contour};
use crate::draw::{hex, mix, Family, Placer, Ramp, Raster, Rgb, Svg, TextStyle};
use crate::geo::{self, Utm};
use crate::vector::{Feature, Geom};
use crate::view::{Terrain, View};
use crate::world::World;

#[derive(Clone)]
pub struct Sheet {
    pub id: &'static str,
    #[allow(dead_code)]
    pub title: &'static str,
    pub bbox: BBox,
    /// Metres per cell of the grid the terrain is computed on.
    pub grid_res: f64,
    /// Display pixels per grid cell.
    pub scale: f64,
    pub contour: f32,
    pub index: f32,
    pub utm_step: f64,
    /// Graticule tick spacing in minutes of arc.
    pub grat_min: f64,
    pub stream_min_km2: f32,
    /// 0 region, 1 ridge, 2 close in.
    pub level: u8,
}

pub const REGION: Sheet = Sheet {
    id: "region",
    title: "Grande Cache to Caw Ridge",
    bbox: config::REGION,
    grid_res: 25.0,
    scale: 1.0,
    contour: 100.0,
    index: 500.0,
    utm_step: 10000.0,
    grat_min: 5.0,
    stream_min_km2: 6.0,
    level: 0,
};

pub const RIDGE: Sheet = Sheet {
    id: "ridge",
    title: "Caw Ridge",
    bbox: config::RIDGE,
    grid_res: 10.0,
    scale: 1.5,
    contour: 20.0,
    index: 100.0,
    utm_step: 1000.0,
    grat_min: 1.0,
    stream_min_km2: 1.2,
    level: 1,
};

pub const CLOSE: Sheet = Sheet {
    id: "close",
    title: "Caw Ridge: the waypoint",
    bbox: config::CLOSE,
    grid_res: 5.0,
    scale: 2.0,
    contour: 10.0,
    index: 50.0,
    utm_step: 1000.0,
    grat_min: 1.0,
    stream_min_km2: 0.45,
    level: 2,
};

/// What goes on top of the base.
#[derive(Clone)]
pub struct Overlay {
    pub contours: bool,
    /// Draw contours in a neutral dark ink, for bases that already carry colour.
    pub quiet: bool,
    pub water: bool,
    pub roads: bool,
    pub boundaries: bool,
    pub labels: bool,
    pub grid: bool,
    pub wildlife: bool,
    pub coal: bool,
}

impl Overlay {
    pub fn topo() -> Overlay {
        Overlay { contours: true, quiet: false, water: true, roads: true, boundaries: true, labels: true, grid: true, wildlife: false, coal: false }
    }
    pub fn thematic() -> Overlay {
        Overlay { contours: true, quiet: true, water: true, roads: true, boundaries: false, labels: true, grid: true, wildlife: false, coal: false }
    }
}

pub struct LegendItem {
    pub swatch: String,
    pub label: String,
}

pub struct Frame<'a> {
    pub world: &'a World,
    pub sheet: Sheet,
    pub vg: View,
    pub vd: View,
    pub terr: Terrain,
    pub contours: Vec<Contour>,
    /// Style scale: 1.0 on a display 1600 pixels wide.
    pub k: f32,
}

pub const FLAT_SHADE: f32 = 0.672;

/// Set by `cawridge pages`: skip the drawing and only rewrite the pages.
pub static DRY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn dry() -> bool {
    DRY.load(std::sync::atomic::Ordering::Relaxed)
}

impl<'a> Frame<'a> {
    pub fn new(world: &'a World, sheet: &Sheet) -> Frame<'a> {
        eprintln!("sheet {}: terrain", sheet.id);
        // A dry run keeps the sheet's box but works on a token grid.
        let vg = View::new(sheet.bbox, if dry() { 400.0 } else { sheet.grid_res });
        let mut vd = vg.clone();
        vd.w = (vg.w as f64 * sheet.scale).round() as usize;
        vd.h = (vg.h as f64 * sheet.scale).round() as usize;
        vd.mres = vg.mres * vg.w as f64 / vd.w as f64;
        let terr = Terrain::new(&vg, vg.elevation(&world.dem));
        eprintln!("  grid {} x {}, display {} x {}", vg.w, vg.h, vd.w, vd.h);
        let contours = contour::trace(&terr.z, vg.w, vg.h, sheet.contour, 0.35 / sheet.scale as f32);
        eprintln!("  {} contour lines", contours.len());
        let k = vd.w as f32 / 1600.0;
        Frame { world, sheet: sheet.clone(), vg, vd, terr, contours, k }
    }

    pub fn p(&self, lon: f64, lat: f64) -> (f32, f32) {
        let (x, y) = self.vd.px(lon, lat);
        (x as f32, y as f32)
    }

    fn line(&self, pts: &[(f64, f64)]) -> Option<Vec<(f32, f32)>> {
        let b = self.sheet.bbox;
        let m = 0.02;
        if !pts.iter().any(|p| p.0 > b.w - m && p.0 < b.e + m && p.1 > b.s - m && p.1 < b.n + m) {
            return None;
        }
        Some(pts.iter().map(|p| self.p(p.0, p.1)).collect())
    }

    // ---------------------------------------------------------------- bases

    fn grid_raster<F: Fn(usize, usize, f64, f64) -> Rgb + Sync>(&self, f: F) -> Raster {
        let vg = &self.vg;
        Raster::from_fn(vg.w, vg.h, |x, y| {
            let (lon, lat) = vg.lonlat(x as f64 + 0.5, y as f64 + 0.5);
            f(x, y, lon, lat)
        })
    }

    fn finish(&self, r: Raster) -> Raster {
        r.resized(self.vd.w, self.vd.h)
    }

    pub fn landcover_colour(code: f32) -> Rgb {
        hex(match code as i32 {
            1 => "#c3d9ab",
            2 => "#cbdeb5",
            5 => "#d2e5b0",
            6 => "#c9dfae",
            8 => "#e0e5ba",
            10 => "#f1ecd0",
            11 => "#e5e3c5",
            12 => "#f0ead4",
            13 => "#ece8de",
            14 => "#cfe5da",
            15 => "#f3ecc4",
            16 => "#e8e3da",
            17 => "#e6d6d0",
            18 => "#b5d7f0",
            19 => "#fbfdff",
            _ => "#eeeadf",
        })
    }

    /// The topographic base: ground cover as a quiet tint, lit by the relief.
    pub fn base_topo(&self) -> Raster {
        let lc = &self.world.landcover;
        let blur = ((30.0 / self.sheet.grid_res) * 1.1).round() as usize;
        let tint = self.grid_raster(|x, y, lon, lat| {
            let c = Frame::landcover_colour(lc.nearest(lon, lat));
            // A little extra light with height keeps the alpine apart from the valley.
            let z = self.terr.z[y * self.vg.w + x];
            mix(c, [252.0, 250.0, 244.0], ((z - 1700.0) / 1400.0).clamp(0.0, 0.35))
        })
        .blurred(blur);
        let shade = self.terr.relief(1.6);
        self.finish(tint.shaded(&shade, FLAT_SHADE, 0.62))
    }

    /// Plain grey relief, the base for thematic colour.
    #[allow(dead_code)]
    pub fn base_grey(&self) -> Raster {
        let shade = self.terr.relief(1.6);
        self.finish(Raster::new(self.vg.w, self.vg.h, [236.0, 236.0, 232.0]).shaded(&shade, FLAT_SHADE, 0.6))
    }

    /// Colour a value grid over the grey relief. `value` is on the display grid's geography:
    /// it is asked for by lon/lat. NaN leaves the relief showing.
    pub fn base_value<F: Fn(f64, f64, usize) -> Option<Rgb> + Sync>(&self, alpha: f32, f: F) -> Raster {
        let shade = self.terr.relief(1.6);
        let w = self.vg.w;
        let r = self.grid_raster(|x, y, lon, lat| {
            let grey = [236.0, 236.0, 232.0];
            match f(lon, lat, y * w + x) {
                Some(c) => mix(grey, c, alpha),
                None => grey,
            }
        });
        self.finish(r.shaded(&shade, FLAT_SHADE, 0.55))
    }

    /// Satellite base. `mode` 0 = natural colour, 1 = colour infrared.
    pub fn base_sat(&self, mode: u8, relief: f32) -> Raster {
        let sat = if self.sheet.level == 0 { &self.world.sat_region } else { &self.world.sat_ridge };
        let utm = Utm::new(sat.zone);
        let vd = &self.vd;
        // Conifer forest reflects only a few percent, so the curve lifts the dark end hard.
        let tone = |v: f32, white: f32| -> f32 { 255.0 * (v / 10000.0 / white).clamp(0.0, 1.0).powf(0.46) };
        let r = Raster::from_fn(vd.w, vd.h, |x, y| {
            let (lon, lat) = vd.lonlat(x as f64 + 0.5, y as f64 + 0.5);
            let (e, n) = utm.forward(lon, lat);
            match sat.sample(e, n) {
                Some(b) => {
                    if mode == 0 {
                        [tone(b[0], 0.24), tone(b[1], 0.24), tone(b[2], 0.22)]
                    } else {
                        [tone(b[3], 0.50), tone(b[0], 0.24), tone(b[1], 0.24)]
                    }
                }
                None => [60.0, 60.0, 60.0],
            }
        });
        if relief > 0.0 {
            let shade = crate::view::resize(&self.terr.relief(1.4), self.vg.w, self.vg.h, vd.w, vd.h);
            r.shaded(&shade, FLAT_SHADE, relief)
        } else {
            r
        }
    }

    // -------------------------------------------------------------- overlay

    /// Build the sheet: base raster, line work, lettering and, if asked, the collar.
    pub fn compose(&self, base: &Raster, ov: &Overlay, extra: &dyn Fn(&mut Svg, &Frame, &mut Placer), collar: Option<&Collar>) -> crate::cog::Res<Raster> {
        if dry() {
            return Ok(Raster::new(1, 1, [0.0; 3]));
        }
        let k = self.k;
        let (fw, fh) = (self.vd.w as f32, self.vd.h as f32);
        let (ml, mt, mr, mb) = match collar {
            Some(_) => (78.0 * k, 64.0 * k, 78.0 * k, 360.0 * k),
            None => (0.0, 0.0, 0.0, 0.0),
        };
        let (sw, sh) = ((fw + ml + mr).ceil(), (fh + mt + mb).ceil());
        let mut svg = Svg::new(sw, sh);
        let _ = write!(svg.defs, "<clipPath id=\"frame\"><rect x=\"0\" y=\"0\" width=\"{fw}\" height=\"{fh}\"/></clipPath>");
        svg.open(&format!("transform=\"translate({ml:.1} {mt:.1})\""));
        svg.open("clip-path=\"url(#frame)\"");
        let mut placer = Placer::default();
        if ov.wildlife {
            self.draw_wildlife(&mut svg);
        }
        if ov.coal {
            self.draw_coal(&mut svg);
        }
        if ov.contours {
            self.draw_contours(&mut svg, ov.quiet, &mut placer);
        }
        if ov.water {
            self.draw_water(&mut svg);
        }
        if ov.boundaries {
            self.draw_boundaries(&mut svg);
        }
        if ov.grid {
            self.draw_grid(&mut svg);
        }
        if ov.roads {
            self.draw_roads(&mut svg);
        }
        extra(&mut svg, self, &mut placer);
        self.draw_waypoint(&mut svg, &mut placer);
        if ov.labels {
            self.draw_labels(&mut svg, &mut placer, ov);
        }
        svg.close();
        if collar.is_some() {
            svg.rect(0.0, 0.0, fw, fh, &format!("fill=\"none\" stroke=\"#1d1d1b\" stroke-width=\"{:.1}\"", 2.2 * k));
        }
        svg.close();
        if let Some(c) = collar {
            self.draw_collar(&mut svg, c, ml, mt, fw, fh, sw, sh);
        }
        let pm = crate::draw::rasterise(&svg.finish(), sw as usize, sh as usize, &self.world.fonts)?;
        let mut sheet = Raster::new(sw as usize, sh as usize, hex("#f6f3ea"));
        sheet.blit(base, ml.round() as usize, mt.round() as usize);
        sheet.over(&pm);
        Ok(sheet)
    }

    fn draw_contours(&self, svg: &mut Svg, quiet: bool, placer: &mut Placer) {
        let k = self.k;
        let s = self.sheet.scale as f32;
        let (ink, ink_index) = if quiet { ("#2b2b2b", "#1f1f1f") } else { ("#9b6a3c", "#7d4f26") };
        let (op, op_index) = if quiet { (0.28, 0.42) } else { (0.62, 0.85) };
        let mut thin = String::new();
        let mut thick = String::new();
        let st = TextStyle::new(Family::Semi, 9.2 * k, 500, if quiet { "#1f1f1f" } else { "#7d4f26" }).halo("#fbf8ef", 2.4 * k);
        let mut labels: Vec<(f32, f32, f32, String)> = Vec::new();
        for c in &self.contours {
            let pts: Vec<(f32, f32)> = c.pts.iter().map(|p| (p.0 * s, p.1 * s)).collect();
            let index = (c.level / self.sheet.index).fract().abs() < 1e-3;
            let d = Svg::path_d(&pts, c.closed);
            if index {
                thick.push_str(&d);
                let len = contour::length(&pts);
                if len > 260.0 * k {
                    let every = 900.0 * k;
                    let mut next = (len % every) / 2.0 + 120.0 * k;
                    let mut run = 0.0;
                    for w in pts.windows(2) {
                        let seg = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
                        if run + seg >= next && seg > 0.0 {
                            let t = (next - run) / seg;
                            let (x, y) = (w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
                            let mut a = (w[1].1 - w[0].1).atan2(w[1].0 - w[0].0).to_degrees();
                            if a > 90.0 {
                                a -= 180.0;
                            }
                            if a < -90.0 {
                                a += 180.0;
                            }
                            labels.push((x, y, a, format!("{}", c.level as i32)));
                            next += every;
                        }
                        run += seg;
                    }
                }
            } else {
                thin.push_str(&d);
            }
        }
        svg.path(&thin, &format!("fill=\"none\" stroke=\"{ink}\" stroke-opacity=\"{op}\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"", 0.55 * k));
        svg.path(&thick, &format!("fill=\"none\" stroke=\"{ink_index}\" stroke-opacity=\"{op_index}\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"", 1.15 * k));
        for (x, y, a, s) in labels {
            let w = st.width(&s);
            let r = w.max(st.size) / 2.0 + 2.0 * k;
            if x < r || y < r || x > self.vd.w as f32 - r || y > self.vd.h as f32 - r {
                continue;
            }
            if placer.try_claim([x - r, y - r, x + r, y + r]) {
                svg.text(x, y + st.size * 0.34, &s, &st, "middle", a);
            }
        }
    }

    fn smooth(pts: &[(f32, f32)], win: usize) -> Vec<(f32, f32)> {
        if pts.len() < 3 {
            return pts.to_vec();
        }
        let n = pts.len();
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if i == 0 || i == n - 1 {
                out.push(pts[i]);
                continue;
            }
            let a = i.saturating_sub(win);
            let b = (i + win).min(n - 1);
            let m = (b - a + 1) as f32;
            let (sx, sy) = pts[a..=b].iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
            out.push((sx / m, sy / m));
        }
        out
    }

    fn draw_water(&self, svg: &mut Svg) {
        let k = self.k;
        let blue = "#2f7ebc";
        // Streams traced from the elevation model, in four weights by catchment size.
        let mut by_w: [String; 4] = Default::default();
        for (pts, a) in &self.world.streams {
            if *a < self.sheet.stream_min_km2 {
                continue;
            }
            if let Some(px) = self.line(pts) {
                let px = contour::simplify(&Frame::smooth(&px, 3), 0.4);
                let class = if *a > 150.0 { 3 } else if *a > 30.0 { 2 } else if *a > 6.0 { 1 } else { 0 };
                by_w[class].push_str(&Svg::path_d(&px, false));
            }
        }
        for (i, d) in by_w.iter().enumerate() {
            let w = [0.7, 1.0, 1.5, 2.1][i] * k;
            svg.path(d, &format!("fill=\"none\" stroke=\"{blue}\" stroke-width=\"{w:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\" stroke-opacity=\"0.9\""));
        }
        let mut rivers = String::new();
        let mut lakes = String::new();
        for f in &self.world.osm {
            match &f.geom {
                Geom::Line(pts) if f.get("waterway") == "river" => {
                    if let Some(px) = self.line(pts) {
                        rivers.push_str(&Svg::path_d(&px, false));
                    }
                }
                Geom::Poly(rings) if f.get("natural") == "water" => {
                    if let Some(px) = self.line(&rings[0]) {
                        lakes.push_str(&Svg::path_d(&px, true));
                    }
                }
                _ => {}
            }
        }
        svg.path(&rivers, &format!("fill=\"none\" stroke=\"{blue}\" stroke-width=\"{:.2}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"", 2.6 * k));
        svg.path(&lakes, &format!("fill=\"#a9d2f0\" stroke=\"{blue}\" stroke-width=\"{:.2}\"", 0.8 * k));
    }

    fn draw_roads(&self, svg: &mut Svg) {
        let k = self.k;
        let lv = self.sheet.level;
        // Casing first, then fill, so junctions join cleanly.
        let mut hwy = String::new();
        let mut gravel = String::new();
        let mut private = String::new();
        let mut track = String::new();
        let mut path = String::new();
        let mut rail = String::new();
        let mut town = String::new();
        let mut industrial = String::new();
        for f in &self.world.osm {
            match &f.geom {
                Geom::Line(pts) => {
                    let h = f.get("highway");
                    if !h.is_empty() {
                        let Some(px) = self.line(pts) else { continue };
                        let d = Svg::path_d(&px, false);
                        match h {
                            "primary" | "secondary" | "trunk" | "primary_link" => hwy.push_str(&d),
                            "tertiary" | "unclassified" | "service" | "residential" => {
                                if lv == 0 && (h == "residential" || h == "service") {
                                    town.push_str(&d);
                                } else if f.get("access") == "private" {
                                    private.push_str(&d);
                                } else {
                                    gravel.push_str(&d);
                                }
                            }
                            "track" => track.push_str(&d),
                            "path" | "footway" | "cycleway" | "bridleway" | "steps" => {
                                if lv > 0 || contour::length(&px) > 60.0 * k {
                                    path.push_str(&d)
                                }
                            }
                            _ => {}
                        }
                    } else if f.get("railway") == "rail" {
                        if let Some(px) = self.line(pts) {
                            rail.push_str(&Svg::path_d(&px, false));
                        }
                    }
                }
                Geom::Poly(rings) => {
                    let l = f.get("landuse");
                    if l == "industrial" || l == "quarry" {
                        if let Some(px) = self.line(&rings[0]) {
                            industrial.push_str(&Svg::path_d(&px, true));
                        }
                    }
                }
                _ => {}
            }
        }
        let j = "stroke-linecap=\"round\" stroke-linejoin=\"round\" fill=\"none\"";
        svg.path(&industrial, &format!("fill=\"#5d5d5d\" fill-opacity=\"0.18\" stroke=\"#4a4a4a\" stroke-width=\"{:.2}\" stroke-dasharray=\"{:.1} {:.1}\"", 0.9 * k, 4.0 * k, 3.0 * k));
        svg.path(&town, &format!("{j} stroke=\"#6b6b6b\" stroke-width=\"{:.2}\"", 0.7 * k));
        svg.path(&rail, &format!("{j} stroke=\"#2a2a2a\" stroke-width=\"{:.2}\"", 1.5 * k));
        svg.path(&rail, &format!("fill=\"none\" stroke=\"#f6f3ea\" stroke-width=\"{:.2}\" stroke-dasharray=\"{:.1} {:.1}\"", 0.7 * k, 5.0 * k, 5.0 * k));
        svg.path(&path, &format!("{j} stroke=\"#fbf8ef\" stroke-opacity=\"0.7\" stroke-width=\"{:.2}\"", 2.4 * k));
        svg.path(&path, &format!("{j} stroke=\"#8c2f1b\" stroke-width=\"{:.2}\" stroke-dasharray=\"{:.1} {:.1}\"", 1.1 * k, 4.5 * k, 2.6 * k));
        svg.path(&track, &format!("{j} stroke=\"#fbf8ef\" stroke-opacity=\"0.75\" stroke-width=\"{:.2}\"", 3.2 * k));
        svg.path(&track, &format!("{j} stroke=\"#33271c\" stroke-width=\"{:.2}\" stroke-dasharray=\"{:.1} {:.1}\"", 1.5 * k, 7.0 * k, 3.0 * k));
        svg.path(&private, &format!("{j} stroke=\"#4a4a4a\" stroke-width=\"{:.2}\"", 2.8 * k));
        svg.path(&private, &format!("{j} stroke=\"#d9d5cb\" stroke-width=\"{:.2}\"", 1.5 * k));
        svg.path(&gravel, &format!("{j} stroke=\"#33271c\" stroke-width=\"{:.2}\"", 3.1 * k));
        svg.path(&gravel, &format!("{j} stroke=\"#fdf6df\" stroke-width=\"{:.2}\"", 1.7 * k));
        svg.path(&hwy, &format!("{j} stroke=\"#4d1c10\" stroke-width=\"{:.2}\"", 4.6 * k));
        svg.path(&hwy, &format!("{j} stroke=\"#e0693c\" stroke-width=\"{:.2}\"", 2.9 * k));
    }

    fn draw_boundaries(&self, svg: &mut Svg) {
        let k = self.k;
        let mut parks = String::new();
        for f in &self.world.parks {
            if let Geom::Poly(rings) = &f.geom {
                for r in rings {
                    if let Some(px) = self.line(r) {
                        parks.push_str(&Svg::path_d(&contour::simplify(&px, 0.5), true));
                    }
                }
            }
        }
        svg.path(&parks, &format!("fill=\"#3c8a4a\" fill-opacity=\"0.10\" fill-rule=\"evenodd\" stroke=\"#2f7a3d\" stroke-opacity=\"0.35\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"", 7.0 * k));
        svg.path(&parks, &format!("fill=\"none\" stroke=\"#1f6a30\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"", 1.3 * k));
        let mut wmu = String::new();
        for f in &self.world.wmu {
            if let Geom::Poly(rings) = &f.geom {
                for r in rings {
                    if let Some(px) = self.line(r) {
                        wmu.push_str(&Svg::path_d(&contour::simplify(&px, 0.5), true));
                    }
                }
            }
        }
        svg.path(&wmu, &format!("fill=\"none\" stroke=\"#b0268f\" stroke-opacity=\"0.22\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"", 9.0 * k));
        svg.path(
            &wmu,
            &format!(
                "fill=\"none\" stroke=\"#8e1672\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\" stroke-dasharray=\"{:.1} {:.1} {:.1} {:.1}\"",
                1.7 * k,
                14.0 * k,
                4.0 * k,
                2.0 * k,
                4.0 * k
            ),
        );
    }

    fn draw_wildlife(&self, svg: &mut Svg) {
        let k = self.k;
        let _ = write!(
            svg.defs,
            "<pattern id=\"hatchGoat\" width=\"{s}\" height=\"{s}\" patternUnits=\"userSpaceOnUse\" patternTransform=\"rotate(45)\"><line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"{s}\" stroke=\"#7a3fa0\" stroke-width=\"{w}\" stroke-opacity=\"0.55\"/></pattern>",
            s = 9.0 * k,
            w = 2.2 * k
        );
        let _ = write!(
            svg.defs,
            "<pattern id=\"hatchCaribou\" width=\"{s}\" height=\"{s}\" patternUnits=\"userSpaceOnUse\" patternTransform=\"rotate(-45)\"><line x1=\"0\" y1=\"0\" x2=\"0\" y2=\"{s}\" stroke=\"#b8651b\" stroke-width=\"{w}\" stroke-opacity=\"0.45\"/></pattern>",
            s = 14.0 * k,
            w = 2.0 * k
        );
        let poly = |fs: &[Feature]| -> String {
            let mut d = String::new();
            for f in fs {
                if let Geom::Poly(rings) = &f.geom {
                    for r in rings {
                        if let Some(px) = self.line(r) {
                            d.push_str(&Svg::path_d(&contour::simplify(&px, 0.6), true));
                        }
                    }
                }
            }
            d
        };
        let car = poly(&self.world.caribou);
        svg.path(&car, &format!("fill=\"url(#hatchCaribou)\" fill-rule=\"evenodd\" stroke=\"#b8651b\" stroke-width=\"{:.2}\"", 2.0 * k));
        let goat = poly(&self.world.goat_sheep);
        svg.path(&goat, &format!("fill=\"url(#hatchGoat)\" fill-rule=\"evenodd\" stroke=\"#7a3fa0\" stroke-width=\"{:.2}\"", 2.0 * k));
    }

    fn draw_coal(&self, svg: &mut Svg) {
        let k = self.k;
        let mut lease = String::new();
        let mut other = String::new();
        for f in &self.world.coal {
            if let Geom::Poly(rings) = &f.geom {
                for r in rings {
                    if let Some(px) = self.line(r) {
                        let d = Svg::path_d(&contour::simplify(&px, 0.6), true);
                        if f.get("AgreementGroup") == "LEASE" {
                            lease.push_str(&d)
                        } else {
                            other.push_str(&d)
                        }
                    }
                }
            }
        }
        svg.path(&other, &format!("fill=\"#555\" fill-opacity=\"0.10\" fill-rule=\"evenodd\" stroke=\"#555\" stroke-width=\"{:.2}\" stroke-dasharray=\"{:.1} {:.1}\"", 1.0 * k, 5.0 * k, 4.0 * k));
        svg.path(&lease, &format!("fill=\"#1d1d1b\" fill-opacity=\"0.20\" fill-rule=\"evenodd\" stroke=\"#1d1d1b\" stroke-width=\"{:.2}\"", 1.4 * k));
    }

    /// UTM grid lines, slightly tilted because the sheet is cut square to true north.
    fn draw_grid(&self, svg: &mut Svg) {
        let k = self.k;
        let utm = Utm::new(11);
        let b = self.sheet.bbox;
        let step = self.sheet.utm_step;
        let corners = [utm.forward(b.w, b.s), utm.forward(b.w, b.n), utm.forward(b.e, b.s), utm.forward(b.e, b.n)];
        let e0 = (corners.iter().map(|c| c.0).fold(f64::MAX, f64::min) / step).floor() * step;
        let e1 = (corners.iter().map(|c| c.0).fold(f64::MIN, f64::max) / step).ceil() * step;
        let n0 = (corners.iter().map(|c| c.1).fold(f64::MAX, f64::min) / step).floor() * step;
        let n1 = (corners.iter().map(|c| c.1).fold(f64::MIN, f64::max) / step).ceil() * step;
        let mut d = String::new();
        let mut e = e0;
        while e <= e1 {
            let (a, c) = (utm.inverse(e, n0), utm.inverse(e, n1));
            let (pa, pc) = (self.p(a.0, a.1), self.p(c.0, c.1));
            let _ = write!(d, "M{:.1} {:.1}L{:.1} {:.1}", pa.0, pa.1, pc.0, pc.1);
            e += step;
        }
        let mut n = n0;
        while n <= n1 {
            let (a, c) = (utm.inverse(e0, n), utm.inverse(e1, n));
            let (pa, pc) = (self.p(a.0, a.1), self.p(c.0, c.1));
            let _ = write!(d, "M{:.1} {:.1}L{:.1} {:.1}", pa.0, pa.1, pc.0, pc.1);
            n += step;
        }
        svg.path(&d, &format!("fill=\"none\" stroke=\"#10324f\" stroke-opacity=\"0.38\" stroke-width=\"{:.2}\"", 0.6 * k));
    }

    fn draw_waypoint(&self, svg: &mut Svg, placer: &mut Placer) {
        let k = self.k;
        let (x, y) = self.p(config::WPT_LON, config::WPT_LAT);
        let r = 11.0 * k;
        svg.circle(x, y, r, &format!("fill=\"none\" stroke=\"#ffffff\" stroke-width=\"{:.1}\" stroke-opacity=\"0.9\"", 5.0 * k));
        svg.circle(x, y, r, &format!("fill=\"#d7261e\" fill-opacity=\"0.12\" stroke=\"#d7261e\" stroke-width=\"{:.1}\"", 2.4 * k));
        for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            svg.line((x + dx * r * 0.55, y + dy * r * 0.55), (x + dx * r * 1.7, y + dy * r * 1.7), &format!("stroke=\"#d7261e\" stroke-width=\"{:.1}\" stroke-linecap=\"round\"", 2.4 * k));
        }
        svg.circle(x, y, 1.8 * k, "fill=\"#d7261e\"");
        placer.claim([x - r * 1.8, y - r * 1.8, x + r * 1.8, y + r * 1.8]);
        let st = TextStyle::new(Family::Condensed, 15.0 * k, 700, "#a5150f").halo("#ffffff", 3.4 * k).upper().spaced(0.6);
        let s2 = TextStyle::new(Family::Semi, 11.0 * k, 500, "#5c0d09").halo("#ffffff", 3.0 * k);
        let label = "Waypoint";
        let sub = format!("{:.0} m", self.world.wpt_elev);
        let w = st.width(label);
        let (lx, ly) = (x + r * 2.1, y - 2.0 * k);
        svg.text(lx, ly, label, &st, "start", 0.0);
        svg.text(lx, ly + 12.5 * k, &sub, &s2, "start", 0.0);
        placer.claim([lx, ly - st.size, lx + w, ly + 14.0 * k]);
    }

    /// Place a point label in the first free spot around the point.
    fn point_label(&self, svg: &mut Svg, placer: &mut Placer, x: f32, y: f32, off: f32, lines: &[(&str, &TextStyle)], force: bool) -> bool {
        let (fw, fh) = (self.vd.w as f32, self.vd.h as f32);
        let w = lines.iter().map(|(s, st)| st.width(s)).fold(0.0, f32::max);
        let h: f32 = lines.iter().map(|(_, st)| st.size * 1.08).sum();
        // Right, left, above, below, then the diagonals.
        let spots = [
            (off, -h / 2.0, "start"),
            (-off - w, -h / 2.0, "end"),
            (-w / 2.0, -off - h, "middle"),
            (-w / 2.0, off, "middle"),
            (off * 0.7, -off * 0.7 - h, "start"),
            (off * 0.7, off * 0.7, "start"),
            (-off * 0.7 - w, -off * 0.7 - h, "end"),
            (-off * 0.7 - w, off * 0.7, "end"),
        ];
        for (i, (dx, dy, anchor)) in spots.iter().enumerate() {
            let b = [x + dx - 2.0, y + dy - 1.0, x + dx + w + 2.0, y + dy + h + 1.0];
            if b[0] < 2.0 || b[1] < 2.0 || b[2] > fw - 2.0 || b[3] > fh - 2.0 {
                continue;
            }
            let last = i == spots.len() - 1;
            if placer.free(b) || (force && last) {
                placer.claim(b);
                let ax = match *anchor {
                    "start" => b[0] + 2.0,
                    "end" => b[2] - 2.0,
                    _ => (b[0] + b[2]) / 2.0,
                };
                let mut yy = b[1] + 1.0;
                for (s, st) in lines {
                    yy += st.size * 0.86;
                    svg.text(ax, yy, s, st, anchor, 0.0);
                    yy += st.size * 0.22;
                }
                return true;
            }
        }
        false
    }

    /// Letter a name along a line, near the middle of the part that is on the sheet.
    fn line_label(&self, svg: &mut Svg, placer: &mut Placer, px: &[(f32, f32)], s: &str, st: &TextStyle, lift: f32) -> bool {
        let (fw, fh) = (self.vd.w as f32, self.vd.h as f32);
        let w = st.width(s);
        let inside: Vec<(f32, f32)> = px.iter().cloned().filter(|p| p.0 > w * 0.6 && p.1 > 30.0 && p.0 < fw - w * 0.6 && p.1 < fh - 30.0).collect();
        if inside.len() < 2 || contour::length(&inside) < w * 1.15 {
            return false;
        }
        let total = contour::length(&inside);
        // Try the middle first, then either side of it.
        for frac in [0.5, 0.35, 0.65, 0.22, 0.78, 0.12, 0.88] {
            let target = total * frac;
            let mut run = 0.0;
            for wd in inside.windows(2) {
                let seg = ((wd[1].0 - wd[0].0).powi(2) + (wd[1].1 - wd[0].1).powi(2)).sqrt();
                if seg > 200.0 * self.k {
                    // A gap where the line left the sheet.
                    run += seg;
                    continue;
                }
                if run + seg >= target && seg > 0.0 {
                    let t = (target - run) / seg;
                    let (x, y) = (wd[0].0 + (wd[1].0 - wd[0].0) * t, wd[0].1 + (wd[1].1 - wd[0].1) * t);
                    // Direction over a span as long as the label, for a steady angle.
                    let pick = |dist: f32| -> (f32, f32) {
                        let tt = (target + dist).clamp(0.0, total);
                        let mut r = 0.0;
                        for w2 in inside.windows(2) {
                            let sg = ((w2[1].0 - w2[0].0).powi(2) + (w2[1].1 - w2[0].1).powi(2)).sqrt();
                            if r + sg >= tt && sg > 0.0 {
                                let q = (tt - r) / sg;
                                return (w2[0].0 + (w2[1].0 - w2[0].0) * q, w2[0].1 + (w2[1].1 - w2[0].1) * q);
                            }
                            r += sg;
                        }
                        inside[inside.len() - 1]
                    };
                    let (a, b) = (pick(-w / 2.0), pick(w / 2.0));
                    let mut ang = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
                    if ang > 90.0 {
                        ang -= 180.0;
                    }
                    if ang < -90.0 {
                        ang += 180.0;
                    }
                    let (c, sn) = (ang.to_radians().cos().abs(), ang.to_radians().sin().abs());
                    let (bw, bh) = (w * c + st.size * sn, w * sn + st.size * c);
                    let bx = [x - bw / 2.0, y - bh / 2.0 - lift, x + bw / 2.0, y + bh / 2.0 - lift];
                    if placer.try_claim(bx) {
                        let (nx, ny) = (ang.to_radians().sin() * lift, -ang.to_radians().cos() * lift);
                        svg.text(x + nx, y + ny + st.size * 0.3, s, st, "middle", ang);
                        return true;
                    }
                    break;
                }
                run += seg;
            }
        }
        false
    }

    fn draw_labels(&self, svg: &mut Svg, placer: &mut Placer, ov: &Overlay) {
        let k = self.k;
        let lv = self.sheet.level;
        let b = self.sheet.bbox;
        let (fw, fh) = (self.vd.w as f32, self.vd.h as f32);

        // Towns first: they win every argument.
        let town = TextStyle::new(Family::Condensed, 19.0 * k, 700, "#1d1d1b").halo("#ffffff", 3.6 * k).upper().spaced(1.2);
        for f in self.world.osm.iter().filter(|f| f.get("place") == "town" || f.get("place") == "village") {
            if let Geom::Point(lon, lat) = f.geom {
                if b.contains(lon, lat) {
                    let (x, y) = self.p(lon, lat);
                    svg.rect(x - 3.5 * k, y - 3.5 * k, 7.0 * k, 7.0 * k, &format!("fill=\"#1d1d1b\" stroke=\"#fff\" stroke-width=\"{:.1}\"", 1.2 * k));
                    self.point_label(svg, placer, x, y, 8.0 * k, &[(f.get("name"), &town)], true);
                }
            }
        }

        // Summits: OpenStreetMap peaks, with the height read from the elevation model.
        let peak = TextStyle::new(Family::Semi, 12.5 * k, 600, "#33271c").halo("#fbf8ef", 3.0 * k);
        let peak_h = TextStyle::new(Family::Semi, 10.5 * k, 400, "#4d3c2c").halo("#fbf8ef", 2.8 * k);
        let mut summit_names: Vec<String> = Vec::new();
        for f in self.world.osm.iter().filter(|f| f.get("natural") == "peak" && f.has("name")) {
            if let Geom::Point(lon, lat) = f.geom {
                if !b.contains(lon, lat) {
                    continue;
                }
                let (x, y) = self.p(lon, lat);
                let t = 5.0 * k;
                svg.path(
                    &format!("M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z", x, y - t, x + t, y + t * 0.8, x - t, y + t * 0.8),
                    &format!("fill=\"#33271c\" stroke=\"#fbf8ef\" stroke-width=\"{:.1}\"", 1.0 * k),
                );
                placer.claim([x - t, y - t, x + t, y + t]);
                let z = self.world.dem.sample(lon, lat);
                let name = f.get("name").replace("Carin Mountain", "Cairn Mountain");
                let h = format!("{z:.0} m");
                self.point_label(svg, placer, x, y, 8.0 * k, &[(&name, &peak), (&h, &peak_h)], lv > 0);
                summit_names.push(name);
            }
        }

        // Roads and trails, named along the line.
        if ov.roads {
            let road = TextStyle::new(Family::Semi, 11.5 * k, 500, "#33271c").halo("#fbf8ef", 3.0 * k);
            let hwy = TextStyle::new(Family::Condensed, 13.0 * k, 600, "#7a2a12").halo("#fbf8ef", 3.2 * k).upper().spaced(0.8);
            let mut done: Vec<String> = Vec::new();
            for f in self.world.osm.iter().filter(|f| f.has("highway") && f.has("name")) {
                let h = f.get("highway");
                let name = f.get("name");
                let major = matches!(h, "primary" | "secondary" | "trunk");
                let keep = major || matches!(h, "unclassified" | "track" | "tertiary") || (lv > 0 && matches!(h, "path"));
                if !keep || (lv == 0 && !major && name != "Beaverdam Road") {
                    continue;
                }
                if done.iter().filter(|n| n.as_str() == name).count() >= if major { 3 } else { 1 } {
                    continue;
                }
                if let Geom::Line(pts) = &f.geom {
                    if let Some(px) = self.line(pts) {
                        let label = if major && name == "Bighorn Highway" { "Highway 40" } else { name };
                        if self.line_label(svg, placer, &px, label, if major { &hwy } else { &road }, 7.5 * k) {
                            done.push(name.to_string());
                        }
                    }
                }
            }
        }

        // Water: official names, lettered along the OpenStreetMap line of the same name
        // where there is one, otherwise at the official point.
        if ov.water {
            let river = TextStyle::new(Family::Serif, 13.5 * k, 400, "#14507f").italic().halo("#f4f8fb", 3.0 * k).spaced(0.6);
            let creek = TextStyle::new(Family::Serif, 11.5 * k, 400, "#14507f").italic().halo("#f4f8fb", 2.8 * k).spaced(0.3);
            let mut done: Vec<String> = Vec::new();
            let mut waters: Vec<&Feature> = self.world.osm.iter().filter(|f| f.has("waterway") && f.has("name")).collect();
            waters.sort_by_key(|f| if f.get("waterway") == "river" { 0 } else { 1 });
            for f in waters {
                let name = f.get("name");
                let is_river = f.get("waterway") == "river";
                if lv == 0 && !is_river {
                    continue;
                }
                let cap = if is_river { 2 } else { 1 };
                if done.iter().filter(|n| n.as_str() == name).count() >= cap {
                    continue;
                }
                if let Geom::Line(pts) = &f.geom {
                    if let Some(px) = self.line(pts) {
                        if self.line_label(svg, placer, &px, name, if is_river { &river } else { &creek }, 7.0 * k) {
                            done.push(name.to_string());
                        }
                    }
                }
            }
            for f in self.world.names.iter().filter(|f| matches!(f.get("code"), "RIV" | "LAKE" | "FALL" | "RAP")) {
                let name = f.get("name");
                if done.iter().any(|n| n == name) || (lv == 0 && f.get("code") == "RIV") {
                    continue;
                }
                if let Geom::Point(lon, lat) = f.geom {
                    if !b.contains(lon, lat) {
                        continue;
                    }
                    // Find the traced stream that passes the official point and letter along it.
                    let mut best: Option<(&Vec<(f64, f64)>, f64)> = None;
                    for (pts, a) in &self.world.streams {
                        if *a < self.sheet.stream_min_km2 {
                            continue;
                        }
                        let d = crate::vector::nearest_on(pts, lon, lat).0;
                        if d < 400.0 && best.map(|b| d < b.1).unwrap_or(true) {
                            best = Some((pts, d));
                        }
                    }
                    let mut placed = false;
                    if let Some((pts, _)) = best {
                        if let Some(px) = self.line(pts) {
                            let px = Frame::smooth(&px, 4);
                            placed = self.line_label(svg, placer, &px, name, &creek, 7.0 * k);
                        }
                    }
                    if !placed {
                        let (x, y) = self.p(lon, lat);
                        placed = self.point_label(svg, placer, x, y, 5.0 * k, &[(name, &creek)], false);
                    }
                    if placed {
                        done.push(name.to_string());
                    }
                }
            }
        }

        // Landforms from the official names database that are not already summits.
        let land = TextStyle::new(Family::Semi, 12.5 * k, 500, "#3d3024").halo("#fbf8ef", 3.0 * k).italic();
        for f in self.world.names.iter().filter(|f| matches!(f.get("code"), "MTN" | "VALL" | "PLN" | "CAPE" | "UNP" | "GEOG" | "CLF")) {
            let name = f.get("name");
            if summit_names.iter().any(|n| n == name) {
                continue;
            }
            if let Geom::Point(lon, lat) = f.geom {
                if b.contains(lon, lat) {
                    let (x, y) = self.p(lon, lat);
                    if x > 20.0 && y > 20.0 && x < fw - 20.0 && y < fh - 20.0 {
                        self.point_label(svg, placer, x, y, 2.0 * k, &[(name, &land)], false);
                    }
                }
            }
        }

        // The ridge itself has no official name, so the sheet letters the local one.
        if lv >= 1 {
            let st = TextStyle::new(Family::Condensed, if lv == 1 { 25.0 } else { 30.0 } * k, 600, "#33271c").halo("#fbf8ef", 4.0 * k).upper().spaced(if lv == 1 { 9.0 } else { 14.0 });
            let (x, y) = self.p(-119.377, 54.0545);
            let w = st.width("Caw Ridge");
            if placer.try_claim([x - w / 2.0, y - st.size, x + w / 2.0, y + st.size * 0.4]) {
                svg.text(x, y, "Caw Ridge", &st, "middle", 0.0);
            }
        }

        // Campgrounds and staging areas.
        let camp = TextStyle::new(Family::Semi, 11.0 * k, 500, "#1f4f2a").halo("#fbf8ef", 2.8 * k);
        for f in self.world.osm.iter().filter(|f| matches!(f.get("tourism"), "camp_site" | "caravan_site") && f.has("name")) {
            if let Geom::Point(lon, lat) = f.geom {
                if b.contains(lon, lat) {
                    let (x, y) = self.p(lon, lat);
                    let t = 5.0 * k;
                    svg.path(
                        &format!("M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z", x, y - t, x + t * 1.1, y + t * 0.8, x - t * 1.1, y + t * 0.8),
                        &format!("fill=\"#1f6a30\" stroke=\"#fff\" stroke-width=\"{:.1}\"", 1.0 * k),
                    );
                    let name = f.get("name").replace("Suplhur", "Sulphur");
                    self.point_label(svg, placer, x, y, 7.0 * k, &[(&name, &camp)], false);
                }
            }
        }

        // Boundaries: unit numbers and park names inside their ground.
        if ov.boundaries {
            let wmu = TextStyle::new(Family::Condensed, 17.0 * k, 700, "#8e1672").halo("#ffffff", 3.4 * k).upper().spaced(1.5);
            for f in &self.world.wmu {
                if let Geom::Poly(rings) = &f.geom {
                    let code = f.get("WMUNIT_CODE").trim_start_matches('0').to_string();
                    let label = format!("WMU {code}");
                    let want = if lv == 0 { 2 } else { 1 };
                    let mut got = 0;
                    for (x, y) in self.interior_points(rings, 12) {
                        let w = wmu.width(&label);
                        if got < want && placer.try_claim([x - w / 2.0, y - wmu.size, x + w / 2.0, y + wmu.size * 0.3]) {
                            svg.text(x, y, &label, &wmu, "middle", 0.0);
                            got += 1;
                        }
                    }
                }
            }
            let park = TextStyle::new(Family::Condensed, 15.0 * k, 600, "#1f6a30").halo("#ffffff", 3.2 * k).upper().spaced(1.6);
            let mut seen: Vec<String> = Vec::new();
            for f in &self.world.parks {
                let name = f.get("OC_NAME").to_string();
                if seen.contains(&name) || f.get("HECTARES").parse::<f64>().unwrap_or(0.0) < 500.0 {
                    continue;
                }
                if let Geom::Poly(rings) = &f.geom {
                    for (x, y) in self.interior_points(rings, 8) {
                        if seen.contains(&name) {
                            break;
                        }
                        let w = park.width(&name);
                        if x - w / 2.0 > 4.0 && x + w / 2.0 < fw - 4.0 && placer.try_claim([x - w / 2.0, y - park.size, x + w / 2.0, y + park.size * 0.3]) {
                            svg.text(x, y, &name, &park, "middle", 0.0);
                            seen.push(name.clone());
                        }
                    }
                }
            }
        }
    }

    /// Up to `n` well separated points that are inside both the polygon and the sheet,
    /// preferring the ones furthest from the polygon edge.
    fn interior_points(&self, rings: &[Vec<(f64, f64)>], n: usize) -> Vec<(f32, f32)> {
        let b = self.sheet.bbox;
        let mut cands: Vec<(f64, f64, f64)> = Vec::new();
        let steps = 14;
        for iy in 1..steps {
            for ix in 1..steps {
                let lon = b.w + (b.e - b.w) * ix as f64 / steps as f64;
                let lat = b.s + (b.n - b.s) * iy as f64 / steps as f64;
                if crate::vector::in_poly(rings, lon, lat) {
                    let d = rings.iter().map(|r| crate::vector::nearest_on(r, lon, lat).0).fold(f64::MAX, f64::min);
                    // Distance to the sheet edge counts too.
                    let edge = ((lon - b.w).min(b.e - lon) * 65000.0).min((lat - b.s).min(b.n - lat) * 111000.0);
                    cands.push((lon, lat, d.min(edge * 1.5)));
                }
            }
        }
        cands.sort_by(|a, c| c.2.partial_cmp(&a.2).unwrap());
        let mut out: Vec<(f64, f64)> = Vec::new();
        for c in cands {
            if out.len() >= n {
                break;
            }
            if c.2 < 900.0 {
                break;
            }
            let apart = if self.sheet.level == 0 { 15000.0 } else { 3000.0 };
            if out.iter().all(|o| geo::haversine(o.0, o.1, c.0, c.1) > apart) {
                out.push((c.0, c.1));
            }
        }
        out.iter().map(|o| self.p(o.0, o.1)).collect()
    }

    // --------------------------------------------------------------- collar

    #[allow(clippy::too_many_arguments)]
    fn draw_collar(&self, svg: &mut Svg, c: &Collar, ml: f32, mt: f32, fw: f32, fh: f32, sw: f32, sh: f32) {
        let k = self.k;
        let ink = "#1d1d1b";
        let b = self.sheet.bbox;
        let utm = Utm::new(11);

        // Grid numbers around the frame.
        let gl = TextStyle::new(Family::Condensed, 12.5 * k, 600, "#10324f");
        let gs = TextStyle::new(Family::Condensed, 9.0 * k, 500, "#10324f");
        let step = self.sheet.utm_step;
        let corners = [utm.forward(b.w, b.s), utm.forward(b.w, b.n), utm.forward(b.e, b.s), utm.forward(b.e, b.n)];
        let e0 = (corners.iter().map(|c| c.0).fold(f64::MAX, f64::min) / step).floor() * step;
        let e1 = (corners.iter().map(|c| c.0).fold(f64::MIN, f64::max) / step).ceil() * step;
        let n0 = (corners.iter().map(|c| c.1).fold(f64::MAX, f64::min) / step).floor() * step;
        let n1 = (corners.iter().map(|c| c.1).fold(f64::MIN, f64::max) / step).ceil() * step;
        let every = if step < 5000.0 && (e1 - e0) / step > 16.0 { 2.0 } else { 1.0 };
        // Where a grid line crosses a frame edge, found by bisection along the edge.
        let cross_lat = |e: f64, lat: f64| -> Option<f64> {
            let f = |lon: f64| utm.forward(lon, lat).0 - e;
            let (mut a, mut c) = (b.w, b.e);
            if f(a) * f(c) > 0.0 {
                return None;
            }
            for _ in 0..40 {
                let m = (a + c) / 2.0;
                if f(a) * f(m) <= 0.0 { c = m } else { a = m }
            }
            Some((a + c) / 2.0)
        };
        let cross_lon = |n: f64, lon: f64| -> Option<f64> {
            let f = |lat: f64| utm.forward(lon, lat).1 - n;
            let (mut a, mut c) = (b.s, b.n);
            if f(a) * f(c) > 0.0 {
                return None;
            }
            for _ in 0..40 {
                let m = (a + c) / 2.0;
                if f(a) * f(m) <= 0.0 { c = m } else { a = m }
            }
            Some((a + c) / 2.0)
        };
        let fmt_grid = |v: f64| -> (String, String) {
            // Small leading digits, large principal digits, as on the national topographic maps.
            let km = (v / 1000.0).round() as i64;
            if step >= 10000.0 {
                (format!("{}", km / 100), format!("{:02}", km % 100))
            } else {
                (format!("{}", km / 100), format!("{:02}", km % 100))
            }
        };
        let mut e = e0;
        while e <= e1 {
            if ((e - e0) / step) % every < 0.5 {
                for (lat, top) in [(b.n, true), (b.s, false)] {
                    if let Some(lon) = cross_lat(e, lat) {
                        let x = ml + self.p(lon, lat).0;
                        let y = if top { mt - 9.0 * k } else { mt + fh + 19.0 * k };
                        let (small, big) = fmt_grid(e);
                        let wb = gl.width(&big);
                        svg.text(x + wb / 2.0, y, &big, &gl, "end", 0.0);
                        svg.text(x - wb / 2.0 - 1.0 * k, y - 3.5 * k, &small, &gs, "end", 0.0);
                    }
                }
            }
            e += step;
        }
        let mut n = n0;
        while n <= n1 {
            if ((n - n0) / step) % every < 0.5 {
                for (lon, left) in [(b.w, true), (b.e, false)] {
                    if let Some(lat) = cross_lon(n, lon) {
                        let y = mt + self.p(lon, lat).1 + 4.5 * k;
                        let (small, big) = fmt_grid(n);
                        if left {
                            svg.text(ml - 8.0 * k, y, &big, &gl, "end", 0.0);
                            svg.text(ml - 8.0 * k - gl.width(&big) - 1.0 * k, y - 3.5 * k, &small, &gs, "end", 0.0);
                        } else {
                            let ws = gs.width(&small);
                            svg.text(ml + fw + 8.0 * k, y - 3.5 * k, &small, &gs, "start", 0.0);
                            svg.text(ml + fw + 9.0 * k + ws, y, &big, &gl, "start", 0.0);
                        }
                    }
                }
            }
            n += step;
        }

        // Latitude and longitude ticks.
        let tl = TextStyle::new(Family::Semi, 9.5 * k, 400, "#1d1d1b");
        let gm = self.sheet.grat_min / 60.0;
        let label_every = if self.sheet.grat_min >= 5.0 { 2 } else { 5 };
        let mut i = (b.w / gm).ceil() as i64;
        while (i as f64) * gm <= b.e {
            let lon = i as f64 * gm;
            let x = ml + self.p(lon, b.n).0;
            let major = i.rem_euclid(label_every) == 0;
            let t = if major { 9.0 } else { 5.0 } * k;
            for y in [mt, mt + fh] {
                svg.line((x, y - t), (x, y + t), &format!("stroke=\"{ink}\" stroke-width=\"{:.1}\"", 1.0 * k));
            }
            if major {
                let d = lon.abs();
                let s = format!("{}\u{b0}{:02.0}'W", d.floor(), (d - d.floor()) * 60.0);
                svg.text(x, mt - 27.0 * k, &s, &tl, "middle", 0.0);
                svg.text(x, mt + fh + 38.0 * k, &s, &tl, "middle", 0.0);
            }
            i += 1;
        }
        let mut i = (b.s / gm).ceil() as i64;
        while (i as f64) * gm <= b.n {
            let lat = i as f64 * gm;
            let y = mt + self.p(b.w, lat).1;
            let major = i.rem_euclid(label_every) == 0;
            let t = if major { 9.0 } else { 5.0 } * k;
            for x in [ml, ml + fw] {
                svg.line((x - t, y), (x + t, y), &format!("stroke=\"{ink}\" stroke-width=\"{:.1}\"", 1.0 * k));
            }
            if major {
                let s = format!("{}\u{b0}{:02.0}'N", lat.floor(), (lat - lat.floor()) * 60.0);
                svg.text(ml - 46.0 * k, y, &s, &tl, "middle", -90.0);
                svg.text(ml + fw + 50.0 * k, y, &s, &tl, "middle", 90.0);
            }
            i += 1;
        }

        // The band under the map.
        let top = mt + fh + 62.0 * k;
        let title = TextStyle::new(Family::Condensed, 40.0 * k, 700, ink).upper().spaced(1.5);
        let sub = TextStyle::new(Family::Semi, 16.0 * k, 500, "#4a443a");
        let small = TextStyle::new(Family::Semi, 11.5 * k, 400, "#2c2a26");
        let head = TextStyle::new(Family::Condensed, 12.5 * k, 700, "#7a2a12").upper().spaced(1.4);
        svg.text(ml, top + 34.0 * k, c.title, &title, "start", 0.0);
        svg.text(ml, top + 60.0 * k, c.subtitle, &sub, "start", 0.0);
        let ground = self.vd.mid_ground_res() as f32;
        let facts = [
            format!("Waypoint {:.6}, {:.6}", config::WPT_LAT, config::WPT_LON),
            format!("UTM {}   (NAD83)", geo::utm_string(config::WPT_LON, config::WPT_LAT)),
            format!("{:.0} m above sea level", self.world.wpt_elev),
            format!("Contour interval {} m, heavy line every {} m", self.sheet.contour, self.sheet.index),
            format!("Grid: UTM zone 11, {} km squares. One pixel is {:.1} m.", self.sheet.utm_step / 1000.0, ground),
        ];
        for (i, f) in facts.iter().enumerate() {
            svg.text(ml, top + 88.0 * k + i as f32 * 17.0 * k, f, &small, "start", 0.0);
        }
        let mut yy = top + 88.0 * k + facts.len() as f32 * 17.0 * k + 8.0 * k;
        for note in c.notes {
            svg.text(ml, yy, note, &TextStyle::new(Family::Semi, 11.5 * k, 500, "#7a2a12"), "start", 0.0);
            yy += 17.0 * k;
        }

        // Scale bar.
        let col2 = ml + 0.30 * sw;
        svg.text(col2, top + 14.0 * k, "Scale", &head, "start", 0.0);
        let km_px = 1000.0 / ground;
        let total_km = [20.0f32, 10.0, 5.0, 4.0, 2.0, 1.0].into_iter().find(|v| v * km_px < 0.19 * sw).unwrap_or(1.0);
        let parts = if total_km == 4.0 || total_km == 20.0 { 4 } else { 5 };
        let by = top + 34.0 * k;
        for i in 0..parts {
            let x0 = col2 + total_km * km_px * i as f32 / parts as f32;
            let w = total_km * km_px / parts as f32;
            svg.rect(x0, by, w, 8.0 * k, &format!("fill=\"{}\" stroke=\"{ink}\" stroke-width=\"{:.1}\"", if i % 2 == 0 { ink } else { "#f6f3ea" }, 1.0 * k));
        }
        for i in 0..=parts {
            let x0 = col2 + total_km * km_px * i as f32 / parts as f32;
            let v = total_km * i as f32 / parts as f32;
            let s = if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.1}") };
            svg.text(x0, by + 23.0 * k, &s, &small, "middle", 0.0);
        }
        svg.text(col2 + total_km * km_px + 12.0 * k, by + 8.0 * k, "km", &small, "start", 0.0);
        // Declination diagram.
        let dy = by + 52.0 * k;
        svg.text(col2, dy, "North", &head, "start", 0.0);
        let (ox, oy) = (col2 + 58.0 * k, dy + 150.0 * k);
        let len = 120.0 * k;
        let ray = |deg: f64, l: f32| -> (f32, f32) { (ox + l * deg.to_radians().sin() as f32, oy - l * deg.to_radians().cos() as f32) };
        // Angles are drawn wider than true so the three lines can be told apart.
        let (tn, gn, mn) = (0.0, self.world.convergence * 5.0, self.world.declination);
        for (a, l, dash) in [(tn, len, ""), (gn, len * 0.92, ""), (mn, len * 0.92, "")] {
            let e = ray(a, l);
            svg.line((ox, oy), e, &format!("stroke=\"{ink}\" stroke-width=\"{:.1}\" {dash}", 1.3 * k));
        }
        let e = ray(tn, len);
        let mut star = String::new();
        for i in 0..10 {
            let r = if i % 2 == 0 { 7.0 * k } else { 2.9 * k };
            let a = (i as f32 * 36.0).to_radians();
            star.push_str(&format!("{}{:.1} {:.1}", if i == 0 { "M" } else { "L" }, e.0 + r * a.sin(), e.1 - 8.0 * k - r * a.cos()));
        }
        svg.path(&format!("{star}Z"), &format!("fill=\"{ink}\""));
        let e = ray(gn, len * 0.92);
        svg.text(e.0 - 4.0 * k, e.1 - 5.0 * k, "GN", &TextStyle::new(Family::Condensed, 11.0 * k, 600, ink), "end", 0.0);
        let e = ray(mn, len * 0.92);
        svg.path(
            &format!("M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z", e.0, e.1, ray(mn - 5.0, len * 0.80).0, ray(mn - 5.0, len * 0.80).1, ray(mn, len * 0.82).0, ray(mn, len * 0.82).1),
            &format!("fill=\"{ink}\""),
        );
        svg.text(e.0 + 5.0 * k, e.1 - 3.0 * k, "MN", &TextStyle::new(Family::Condensed, 11.0 * k, 600, ink), "start", 0.0);
        let gma = self.world.declination - self.world.convergence;
        let dl = [
            format!("Magnetic north is {:.1}\u{b0} east of true north", self.world.declination),
            format!("and {:.1}\u{b0} east of grid north (autumn 2026).", gma),
            format!("Grid north is {:.1}\u{b0} west of true north.", self.world.convergence.abs()),
            "Compass to map: add. Map to compass: subtract.".to_string(),
        ];
        for (i, s) in dl.iter().enumerate() {
            svg.text(col2 + 120.0 * k, dy + 40.0 * k + i as f32 * 17.0 * k, s, &small, "start", 0.0);
        }

        // Legend.
        let col3 = ml + 0.60 * sw;
        svg.text(col3, top + 14.0 * k, "Legend", &head, "start", 0.0);
        let rows = ((c.legend.len() + 1) / 2).max(1);
        for (i, item) in c.legend.iter().enumerate() {
            let (cx, cy) = (col3 + (i / rows) as f32 * 0.19 * sw, top + 36.0 * k + (i % rows) as f32 * 21.0 * k);
            svg.raw(&format!("<g transform=\"translate({cx:.1} {cy:.1}) scale({k:.3})\">{}</g>", item.swatch));
            svg.text(cx + 46.0 * k, cy + 4.0 * k, &item.label, &small, "start", 0.0);
        }

        // Sources along the bottom edge.
        let cr = TextStyle::new(Family::Semi, 10.0 * k, 400, "#5a554b");
        let mut y = sh - 14.0 * k - (c.credits.len() as f32 - 1.0) * 14.0 * k;
        for line in c.credits {
            svg.text(ml, y, line, &cr, "start", 0.0);
            y += 14.0 * k;
        }
    }
}

pub struct Collar<'a> {
    pub title: &'a str,
    pub subtitle: &'a str,
    pub notes: &'a [&'a str],
    pub legend: Vec<LegendItem>,
    pub credits: &'a [&'a str],
}

pub const CREDITS: [&str; 3] = [
    "Elevation: Natural Resources Canada, MRDEM-30 (30 m), contours interpolated. Ground cover: 2020 Land Cover of Canada. Boundaries: Government of Alberta. Names: Canadian Geographical Names Database.",
    "Roads, trails and rivers: OpenStreetMap contributors (ODbL). Smaller streams are traced from the elevation model and may not carry water. Satellite: Copernicus Sentinel-2.",
    "Made by the cawridge generator. A planning aid: carry a real map, a compass and a GPS, and confirm every boundary and regulation with the official source.",
];

pub fn swatch_line(colour: &str, width: f32, dash: &str, casing: Option<(&str, f32)>) -> String {
    let mut s = String::new();
    if let Some((c, w)) = casing {
        let _ = write!(s, "<line x1=\"0\" y1=\"0\" x2=\"38\" y2=\"0\" stroke=\"{c}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>");
    }
    let d = if dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{dash}\"") };
    let _ = write!(s, "<line x1=\"0\" y1=\"0\" x2=\"38\" y2=\"0\" stroke=\"{colour}\" stroke-width=\"{width}\"{d}/>");
    s
}

pub fn swatch_box(fill: &str, stroke: &str) -> String {
    format!("<rect x=\"4\" y=\"-7\" width=\"30\" height=\"14\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"1\"/>")
}

pub fn legend_topo() -> Vec<LegendItem> {
    let li = |s: String, l: &str| LegendItem { swatch: s, label: l.to_string() };
    vec![
        li(swatch_line("#e0693c", 2.9, "", Some(("#4d1c10", 4.6))), "Highway 40 (paved)"),
        li(swatch_line("#fdf6df", 1.7, "", Some(("#33271c", 3.1))), "Gravel road"),
        li(swatch_line("#d9d5cb", 1.5, "", Some(("#4a4a4a", 2.8))), "Private or mine road"),
        li(swatch_line("#33271c", 1.5, "7 3", None), "Track (4x4 or OHV)"),
        li(swatch_line("#8c2f1b", 1.1, "4.5 2.6", None), "Trail"),
        li(swatch_line("#2f7ebc", 1.4, "", None), "River or stream"),
        li(swatch_line("#8e1672", 1.7, "14 4 2 4", Some(("#e9c6e0", 7.0))), "Wildlife management unit"),
        li(swatch_line("#1f6a30", 1.3, "", Some(("#bfdcc4", 7.0))), "Park or recreation area"),
        li(swatch_box("#c3d9ab", "#9bb58a"), "Forest"),
        li(swatch_box("#e0e5ba", "#b9bf95"), "Shrub"),
        li(swatch_box("#f1ecd0", "#c9c2a0"), "Grass and alpine tundra"),
        li(swatch_box("#e8e3da", "#bdb7aa"), "Rock and bare ground"),
    ]
}

pub fn ramp_legend(r: &Ramp, labels: &[(f32, &str)]) -> Vec<LegendItem> {
    labels
        .iter()
        .map(|(v, l)| {
            let c = r.step(*v);
            LegendItem { swatch: swatch_box(&format!("#{:02x}{:02x}{:02x}", c[0] as u8, c[1] as u8, c[2] as u8), "#555"), label: l.to_string() }
        })
        .collect()
}
