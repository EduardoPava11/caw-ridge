//! Runs the analysis and renders every map. What comes out is a list of map files for the
//! pages and the numbers the pages quote.

use std::path::Path;

use crate::analysis;
use crate::cog::Res;
use crate::config::{self, BBox};
use crate::draw::{Family, Placer, Ramp, Raster, Rgb, Svg, TextStyle};
use crate::geo;
use crate::maps::{self, Collar, Frame, LegendItem, Overlay, Sheet};
use crate::route::{self, Leg, Profile};
use crate::sun;
use crate::vector::Geom;
use crate::world::World;

#[allow(dead_code)]
pub struct MapOut {
    pub id: String,
    pub group: &'static str,
    pub title: String,
    pub blurb: String,
    /// Full sheet with collar, for viewing.
    pub view: String,
    /// Full sheet for download (PNG for the topographic sheets).
    pub download: String,
    /// Frame only, cut square to the box, for the interactive map and the KMZ.
    pub bare: String,
    pub bbox: BBox,
    pub w: usize,
    pub h: usize,
}

pub struct VantageOut {
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    pub elev: f32,
    pub open_km2: f32,
    pub share: f32,
    pub dist_m: f64,
    pub bearing: f64,
}

pub struct SnowRow {
    pub winter: String,
    pub first: Option<(i64, f32)>,
    pub lasting: Option<i64>,
}

pub struct Stats {
    pub cover_3km: Vec<(String, f32)>,
    pub slope_3km: Vec<(String, f32)>,
    pub elev_min_3km: f32,
    pub elev_max_3km: f32,
    pub seen_km2_wpt: f32,
    pub seen_share_wpt: f32,
    pub sunrise_terrain: Option<f32>,
    pub sunset_terrain: Option<f32>,
    pub sun_hours_wpt: f32,
    pub treeline: f32,
    /// Which way the ground at the waypoint faces, and how steeply.
    pub wpt_aspect: f32,
    pub wpt_slope: f32,
    /// Highest ground within a kilometre: height, bearing from the waypoint, distance.
    pub rim: (f32, f64, f64),
    /// Read from the official layers at the waypoint.
    pub wmu: String,
    pub caribou_range: Option<String>,
    pub grizzly_core: bool,
    /// Share of the regional sheet inside the core grizzly zone.
    pub grizzly_share: f32,
    pub coal_lease: Option<(String, String, String)>,
    pub goat_sheep_range: bool,
    pub park: Option<String>,
}

pub struct Products {
    pub maps: Vec<MapOut>,
    pub vantages: Vec<VantageOut>,
    pub drive: Vec<Leg>,
    pub drive_profile: Profile,
    pub track_end: (f64, f64),
    pub walk: Vec<(f64, f64)>,
    pub walk_profile: Profile,
    pub walk_out_h: f32,
    pub walk_back_h: f32,
    pub snow: Vec<SnowRow>,
    pub stats: Stats,
}

fn rgb_hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0] as u8, c[1] as u8, c[2] as u8)
}

fn cover_name(code: i32) -> &'static str {
    match code {
        1 | 2 => "Conifer forest",
        5 => "Deciduous forest",
        6 => "Mixed forest",
        8 | 11 => "Shrub",
        10 | 12 => "Grass and alpine tundra",
        13 | 16 => "Rock and bare ground",
        14 => "Wetland",
        17 => "Built or disturbed",
        18 => "Water",
        19 => "Snow and ice",
        _ => "Other",
    }
}

fn cover_colour(code: i32) -> Rgb {
    crate::draw::hex(match code {
        1 | 2 => "#2f6b3a",
        5 => "#7fb24a",
        6 => "#55904a",
        8 | 11 => "#c9b04a",
        10 | 12 => "#efe08a",
        13 | 16 => "#b9b2a6",
        14 => "#5fa8a0",
        17 => "#c0453b",
        18 => "#3f7fc0",
        19 => "#ffffff",
        _ => "#dddddd",
    })
}

pub fn is_open(code: f32) -> bool {
    !matches!(code as i32, 1 | 2 | 5 | 6 | 18 | 0)
}

struct Saver<'a> {
    dir: &'a Path,
    maps: Vec<MapOut>,
}

impl<'a> Saver<'a> {
    #[allow(clippy::too_many_arguments)]
    fn save(&mut self, f: &Frame, id: &str, group: &'static str, title: &str, blurb: &str, sheet: &Raster, bare: &Raster, png: bool) -> Res<()> {
        let view = format!("maps/{id}.jpg");
        let download = if png { format!("maps/{id}.png") } else { view.clone() };
        let b = format!("maps/bare/{id}.jpg");
        if !maps::DRY.load(std::sync::atomic::Ordering::Relaxed) {
            sheet.save_jpg(&self.dir.join(&view), 88)?;
            if png {
                sheet.save_png(&self.dir.join(&download))?;
            }
            bare.save_jpg(&self.dir.join(&b), 86)?;
            // The thumbnail is cut from the map itself, without the collar, so it reads small.
            let tw = 720usize;
            bare.resized(tw, (tw as f32 * bare.h as f32 / bare.w as f32) as usize).save_jpg(&self.dir.join(format!("maps/thumb/{id}.jpg")), 82)?;
            // A middle size for the figures set into the pages.
            let mw = 1600usize.min(bare.w);
            bare.resized(mw, (mw as f32 * bare.h as f32 / bare.w as f32) as usize).save_jpg(&self.dir.join(format!("maps/mid/{id}.jpg")), 84)?;
            eprintln!("  wrote {id}");
        }
        self.maps.push(MapOut {
            id: id.to_string(),
            group,
            title: title.to_string(),
            blurb: blurb.to_string(),
            view,
            download,
            bare: b,
            bbox: f.sheet.bbox,
            w: bare.w,
            h: bare.h,
        });
        Ok(())
    }
}

fn none(_: &mut Svg, _: &Frame, _: &mut Placer) {}

pub fn build(world: &World, out: &Path) -> Res<Products> {
    for d in ["maps", "maps/bare", "maps/thumb", "maps/mid"] {
        std::fs::create_dir_all(out.join(d))?;
    }
    let ra = &world.ra;
    let rt = &world.rt;
    let n = ra.w * ra.h;
    let ridge_win = world.window(BBox { w: config::RIDGE.w - 0.02, s: config::RIDGE.s - 0.015, e: config::RIDGE.e + 0.02, n: config::RIDGE.n + 0.015 });
    let (wx, wy) = ra.px(config::WPT_LON, config::WPT_LAT);
    let (wxi, wyi) = (wx as usize, wy as usize);

    // Ground cover on the regional grid.
    let cover: Vec<f32> = (0..n)
        .map(|i| {
            let (lon, lat) = ra.lonlat((i % ra.w) as f64 + 0.5, (i / ra.w) as f64 + 0.5);
            world.landcover.nearest(lon, lat)
        })
        .collect();

    // ------------------------------------------------------------ the drive
    eprintln!("routing the drive");
    let mut track_end = (config::WPT_LON, config::WPT_LAT);
    let mut best = f64::MAX;
    for f in world.osm.iter().filter(|f| route::kind_of(f).is_some()) {
        if let Geom::Line(pts) = &f.geom {
            for p in pts {
                let d = geo::haversine(p.0, p.1, config::WPT_LON, config::WPT_LAT);
                if d < best {
                    best = d;
                    track_end = *p;
                }
            }
        }
    }
    let raw = route::drive(&world.osm, config::TOWN, track_end).ok_or("no drivable route found in the road data")?;
    // Fold the few blocks of town streets into one leg, and join legs of the same kind.
    let first_long = raw.iter().position(|l| l.kind == "highway" && l.km >= 3.0).unwrap_or(0);
    let mut drive: Vec<Leg> = Vec::new();
    for (i, l) in raw.into_iter().enumerate() {
        let kind = if i < first_long { "town".to_string() } else { l.kind.clone() };
        match drive.last_mut() {
            Some(prev) if prev.kind == kind => {
                prev.pts.extend(l.pts.into_iter().skip(1));
                prev.km += l.km;
            }
            _ => drive.push(Leg { kind, name: l.name, pts: l.pts, km: l.km }),
        }
    }
    let drive_line: Vec<(f64, f64)> = drive.iter().flat_map(|l| l.pts.clone()).collect();
    let drive_profile = route::profile(&world.dem, &drive_line, 40.0, 400.0);
    for l in &drive {
        eprintln!("  {:>8} {:>6.1} km  {}", l.kind, l.km, l.name);
    }

    // ------------------------------------------------------------- walking
    eprintln!("walking times");
    let mut factor = vec![0.6f32; n];
    for f in world.osm.iter().filter(|f| f.has("highway")) {
        if let Geom::Line(pts) = &f.geom {
            for w in pts.windows(2) {
                let (a, b) = (ra.px(w[0].0, w[0].1), ra.px(w[1].0, w[1].1));
                let steps = ((b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil() as usize).max(1);
                for i in 0..=steps {
                    let t = i as f64 / steps as f64;
                    let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                    if x >= 0.0 && y >= 0.0 && (x as usize) < ra.w && (y as usize) < ra.h {
                        factor[y as usize * ra.w + x as usize] = 1.0;
                    }
                }
            }
        }
    }
    let (t_out, _) = analysis::travel_time(rt, ridge_win, (wxi, wyi), &factor, false);
    let (t_in, prev_in) = analysis::travel_time(rt, ridge_win, (wxi, wyi), &factor, true);
    let (tx, ty) = ra.px(track_end.0, track_end.1);
    let tcell = ty as usize * ra.w + tx as usize;
    let mut walk = vec![track_end];
    let mut c = tcell;
    while prev_in[c] != u32::MAX {
        c = prev_in[c] as usize;
        walk.push(ra.lonlat((c % ra.w) as f64 + 0.5, (c / ra.w) as f64 + 0.5));
    }
    walk.push((config::WPT_LON, config::WPT_LAT));
    let walk_profile = route::profile(&world.dem, &walk, 20.0, 100.0);
    let (walk_out_h, walk_back_h) = (t_in[tcell], t_out[tcell]);

    // ------------------------------------------------------------ the sun
    eprintln!("sun on {:?}", config::SUN_DATE);
    let day = sun::days_from_civil(config::SUN_DATE.0, config::SUN_DATE.1, config::SUN_DATE.2);
    let (off, _) = sun::alberta_offset(day);
    let (rise, set, _) = sun::crossing(day, config::WPT_LON, config::WPT_LAT, 90.833).ok_or("no sunrise")?;
    let step = 1.0 / 6.0;
    let mut samples = Vec::new();
    let mut t = rise + 0.02;
    while t < set {
        let (az, alt) = sun::position(day, t, config::WPT_LON, config::WPT_LAT);
        if alt > 0.0 {
            samples.push(((t + off) as f32, az as f32, alt as f32));
        }
        t += step;
    }
    let (sun_first, sun_last, sun_hours) = analysis::sun_day(rt, ridge_win, &samples, step as f32);

    // ------------------------------------------------------- what is in view
    eprintln!("viewshed and vantage points");
    let view_wpt = analysis::viewshed(rt, wx as f32 - 0.5, wy as f32 - 0.5, config::EYE_M, config::ANIMAL_M, 8000.0);
    let close_win = world.window(config::CLOSE);
    // Hide the forest from the scoring by sinking nothing: the score simply counts open cells.
    let scores = analysis::vantage_scores(rt, close_win, 6, 3, 300.0, 3000.0, config::EYE_M, config::ANIMAL_M);
    // Re-score the best candidates on open ground only, at full target density.
    let mut ranked: Vec<(usize, usize, f32, f32, f32)> = scores
        .iter()
        .filter(|v| is_open(cover[v.y * ra.w + v.x]))
        .map(|v| (v.x, v.y, v.elev, v.seen_km2, v.share))
        .collect();
    ranked.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    ranked.truncate(60);
    let open_seen = |x: usize, y: usize| -> (f32, f32) {
        let cell = rt.cell[y];
        let r = (3000.0 / cell) as i64;
        let (mut seen, mut total) = (0u32, 0u32);
        for dy in -r..=r {
            for dx in -r..=r {
                let d2 = (dx * dx + dy * dy) as f32;
                let (px, py) = (x as i64 + dx, y as i64 + dy);
                if d2 > (r * r) as f32 || d2 < (300.0 / cell).powi(2) || px < 0 || py < 0 || px >= ra.w as i64 || py >= ra.h as i64 {
                    continue;
                }
                if !is_open(cover[py as usize * ra.w + px as usize]) {
                    continue;
                }
                total += 1;
                if analysis::sees(rt, x as f32, y as f32, config::EYE_M, px as f32, py as f32, config::ANIMAL_M) {
                    seen += 1;
                }
            }
        }
        (seen as f32 * cell * cell / 1e6, if total > 0 { seen as f32 / total as f32 } else { 0.0 })
    };
    let mut rescored: Vec<(usize, usize, f32, f32, f32)> = ranked.iter().map(|v| {
        let (km2, share) = open_seen(v.0, v.1);
        (v.0, v.1, v.2, km2, share)
    }).collect();
    rescored.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
    let mut picked: Vec<(usize, usize, f32, f32, f32)> = Vec::new();
    for v in rescored {
        if picked.len() >= 8 {
            break;
        }
        let far = picked.iter().all(|p| (((p.0 as f32 - v.0 as f32).powi(2) + (p.1 as f32 - v.1 as f32).powi(2)).sqrt() * rt.cell[v.1]) > 700.0);
        if far {
            picked.push(v);
        }
    }
    let vantages: Vec<VantageOut> = picked
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let (lon, lat) = ra.lonlat(v.0 as f64 + 0.5, v.1 as f64 + 0.5);
            VantageOut {
                name: format!("G{}", i + 1),
                lon,
                lat,
                elev: v.2,
                open_km2: v.3,
                share: v.4,
                dist_m: geo::haversine(config::WPT_LON, config::WPT_LAT, lon, lat),
                bearing: geo::bearing(config::WPT_LON, config::WPT_LAT, lon, lat),
            }
        })
        .collect();
    // A smooth score field for the map: spread each candidate's score over its neighbours.
    let mut score_grid = vec![f32::NAN; n];
    let smax = scores.iter().map(|v| v.seen_km2).fold(0.0, f32::max).max(0.1);
    for y in close_win.1..close_win.3 {
        for x in close_win.0..close_win.2 {
            let (mut s, mut wsum) = (0.0, 0.0);
            for v in &scores {
                let d2 = (v.x as f32 - x as f32).powi(2) + (v.y as f32 - y as f32).powi(2);
                if d2 < 100.0 {
                    let w = (-d2 / 18.0).exp();
                    s += w * v.seen_km2;
                    wsum += w;
                }
            }
            if wsum > 0.0 {
                score_grid[y * ra.w + x] = s / wsum / smax;
            }
        }
    }
    // Ground seen from how many of the picked points.
    let mut seen_count = vec![f32::NAN; n];
    for v in &picked {
        let vs = analysis::viewshed(rt, v.0 as f32, v.1 as f32, config::EYE_M, config::ANIMAL_M, 4000.0);
        for i in 0..n {
            if !vs[i].is_nan() {
                let c = if seen_count[i].is_nan() { 0.0 } else { seen_count[i] };
                seen_count[i] = c + vs[i];
            }
        }
    }

    // ---------------------------------------------------------------- snow
    let mut snow_rows = Vec::new();
    for (w, first, unc, lasting) in &world.snow {
        let y1 = 2000 + w[..2].parse::<i32>().unwrap_or(0);
        let base = sun::days_from_civil(y1, 12, 31);
        let f = first.nearest(config::WPT_LON, config::WPT_LAT);
        let u = unc.nearest(config::WPT_LON, config::WPT_LAT);
        let l = lasting.nearest(config::WPT_LON, config::WPT_LAT);
        snow_rows.push(SnowRow {
            winter: format!("{}-{}", y1, y1 + 1),
            first: if f.is_nan() { None } else { Some((base + f as i64, u)) },
            lasting: if l.is_nan() { None } else { Some(base + l as i64) },
        });
    }
    // Median first snow day (as a day of the autumn, days after 31 August) per cell.
    let snow_median = |lon: f64, lat: f64| -> Option<f32> {
        // Winters whose date is known only to within more than a fortnight are left out.
        let mut v: Vec<f32> = world
            .snow
            .iter()
            .filter(|s| s.2.nearest(lon, lat) <= 15.0)
            .map(|s| s.1.nearest(lon, lat))
            .filter(|x| !x.is_nan())
            .map(|d| d + 122.0)
            .collect();
        if v.len() < 3 {
            return None;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(v[v.len() / 2])
    };

    // --------------------------------------------------------------- stats
    let mut cover_area: std::collections::BTreeMap<&'static str, f32> = Default::default();
    let mut slope_area = [0f32; 5];
    let (mut zmin, mut zmax, mut total) = (f32::MAX, f32::MIN, 0f32);
    let r3 = (3000.0 / rt.cell[wyi]) as i64;
    let mut tree_z: Vec<f32> = Vec::new();
    for dy in -r3..=r3 {
        for dx in -r3..=r3 {
            if dx * dx + dy * dy > r3 * r3 {
                continue;
            }
            let i = (wyi as i64 + dy) as usize * ra.w + (wxi as i64 + dx) as usize;
            let a = rt.cell[wyi].powi(2) / 1e6;
            *cover_area.entry(cover_name(cover[i] as i32)).or_default() += a;
            let s = rt.slope[i];
            slope_area[if s < 10.0 { 0 } else if s < 20.0 { 1 } else if s < 30.0 { 2 } else if s < 40.0 { 3 } else { 4 }] += a;
            zmin = zmin.min(rt.z[i]);
            zmax = zmax.max(rt.z[i]);
            total += a;
            if matches!(cover[i] as i32, 1 | 2 | 6) {
                tree_z.push(rt.z[i]);
            }
        }
    }
    tree_z.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // Treeline read from the data: the height below which 95 percent of the forest cells lie.
    let treeline = if tree_z.is_empty() { f32::NAN } else { tree_z[(tree_z.len() as f32 * 0.95) as usize] };
    let mut cover_3km: Vec<(String, f32)> = cover_area.iter().map(|(k, v)| (k.to_string(), v / total)).collect();
    cover_3km.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let slope_names = ["Under 10\u{b0}", "10 to 20\u{b0}", "20 to 30\u{b0}", "30 to 40\u{b0}", "Over 40\u{b0}"];
    let slope_3km = slope_names.iter().zip(slope_area).map(|(n, a)| (n.to_string(), a / total)).collect();
    let (mut seen, mut all) = (0f32, 0f32);
    for v in view_wpt.iter().filter(|v| !v.is_nan()) {
        all += 1.0;
        seen += v;
    }
    let cell_km2 = rt.cell[wyi].powi(2) / 1e6;
    let wi = wyi * ra.w + wxi;
    let mut rim = (f32::MIN, 0.0, 0.0);
    let r1 = (1000.0 / rt.cell[wyi]) as i64;
    for dy in -r1..=r1 {
        for dx in -r1..=r1 {
            if dx * dx + dy * dy > r1 * r1 {
                continue;
            }
            let (x, y) = ((wxi as i64 + dx) as usize, (wyi as i64 + dy) as usize);
            let z = rt.z[y * ra.w + x];
            if z > rim.0 {
                let (lon, lat) = ra.lonlat(x as f64 + 0.5, y as f64 + 0.5);
                rim = (z, geo::bearing(config::WPT_LON, config::WPT_LAT, lon, lat), geo::haversine(config::WPT_LON, config::WPT_LAT, lon, lat));
            }
        }
    }
    // Aspect and slope of the ground about the waypoint, averaged over a 150 m square.
    let (mut nx, mut ny, mut nz) = (0f32, 0f32, 0f32);
    for dy in -2..=2i64 {
        for dx in -2..=2i64 {
            let n = rt.normal[(wyi as i64 + dy) as usize * ra.w + (wxi as i64 + dx) as usize];
            nx += n[0];
            ny += n[1];
            nz += n[2];
        }
    }
    let wpt_aspect = (nx.atan2(ny).to_degrees() + 360.0) % 360.0;
    let wpt_slope = ((nx * nx + ny * ny).sqrt() / nz).atan().to_degrees();
    fn here(fs: &[crate::vector::Feature]) -> Option<&crate::vector::Feature> {
        fs.iter().find(|f| matches!(&f.geom, Geom::Poly(r) if crate::vector::in_poly(r, config::WPT_LON, config::WPT_LAT)))
    }
    let (mut gin, mut gall) = (0f32, 0f32);
    for iy in 0..40 {
        for ix in 0..40 {
            let lon = config::REGION.w + (config::REGION.e - config::REGION.w) * (ix as f64 + 0.5) / 40.0;
            let lat = config::REGION.s + (config::REGION.n - config::REGION.s) * (iy as f64 + 0.5) / 40.0;
            gall += 1.0;
            if world.grizzly.iter().any(|f| matches!(&f.geom, Geom::Poly(r) if crate::vector::in_poly(r, lon, lat))) {
                gin += 1.0;
            }
        }
    }
    let stats = Stats {
        wmu: here(&world.wmu).map(|f| format!("{} {}", f.get("WMUNIT_CODE").trim_start_matches('0'), f.get("WMUNIT_NAME"))).unwrap_or_default(),
        caribou_range: here(&world.caribou).map(|f| f.get("SUBUNIT").to_string()),
        grizzly_core: here(&world.grizzly).is_some(),
        grizzly_share: gin / gall,
        coal_lease: world.coal.iter().filter(|f| f.get("AgreementGroup") == "LEASE").find(|f| matches!(&f.geom, Geom::Poly(r) if crate::vector::in_poly(r, config::WPT_LON, config::WPT_LAT))).map(|f| (f.get("AgreementNumber").to_string(), f.get("DesRepName").to_string(), f.get("CurrentExpiryText").to_string())),
        goat_sheep_range: here(&world.goat_sheep).is_some(),
        park: here(&world.parks).map(|f| f.get("OC_NAME").to_string()),
        wpt_aspect,
        wpt_slope,
        rim,
        cover_3km,
        slope_3km,
        elev_min_3km: zmin,
        elev_max_3km: zmax,
        seen_km2_wpt: seen * cell_km2,
        seen_share_wpt: seen / all.max(1.0),
        sunrise_terrain: if sun_first[wi].is_nan() { None } else { Some(sun_first[wi]) },
        sunset_terrain: if sun_last[wi].is_nan() { None } else { Some(sun_last[wi]) },
        sun_hours_wpt: sun_hours[wi],
        treeline,
    };

    // ---------------------------------------------------------------- maps
    let mut sv = Saver { dir: out, maps: Vec::new() };
    let li = |c: &str, l: &str| LegendItem { swatch: maps::swatch_box(c, "#555"), label: l.to_string() };
    let route_extra = |svg: &mut Svg, f: &Frame, placer: &mut Placer| {
        let k = f.k;
        let px: Vec<(f32, f32)> = walk.iter().map(|p| f.p(p.0, p.1)).collect();
        svg.path(&Svg::path_d(&px, false), &format!("fill=\"none\" stroke=\"#ffffff\" stroke-width=\"{:.1}\" stroke-opacity=\"0.8\" stroke-linecap=\"round\"", 4.2 * k));
        svg.path(&Svg::path_d(&px, false), &format!("fill=\"none\" stroke=\"#d7261e\" stroke-width=\"{:.1}\" stroke-dasharray=\"{:.1} {:.1}\" stroke-linecap=\"round\"", 2.0 * k, 1.0 * k, 5.0 * k));
        let (x, y) = f.p(track_end.0, track_end.1);
        svg.circle(x, y, 5.0 * k, &format!("fill=\"#ffffff\" stroke=\"#1d1d1b\" stroke-width=\"{:.1}\"", 2.0 * k));
        let st = TextStyle::new(Family::Semi, 11.5 * k, 600, "#1d1d1b").halo("#ffffff", 3.0 * k);
        let s = "End of mapped track";
        let w = st.width(s);
        if placer.try_claim([x - w - 9.0 * k, y - st.size, x - 7.0 * k, y + 4.0 * k]) {
            svg.text(x - 9.0 * k, y + 3.5 * k, s, &st, "end", 0.0);
        }
    };
    let vantage_extra = |svg: &mut Svg, f: &Frame, placer: &mut Placer| {
        let k = f.k;
        let st = TextStyle::new(Family::Condensed, 14.0 * k, 700, "#1d1d1b").halo("#ffffff", 3.2 * k);
        for v in &vantages {
            if !f.sheet.bbox.contains(v.lon, v.lat) {
                continue;
            }
            let (x, y) = f.p(v.lon, v.lat);
            let r = 6.5 * k;
            svg.path(
                &format!("M{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}L{:.1} {:.1}Z", x, y - r, x + r, y, x, y + r, x - r, y),
                &format!("fill=\"#ffd21f\" stroke=\"#1d1d1b\" stroke-width=\"{:.1}\"", 1.8 * k),
            );
            placer.claim([x - r, y - r, x + r, y + r]);
            let w = st.width(&v.name);
            for (dx, dy) in [(r + 3.0 * k, 5.0 * k), (-r - 3.0 * k - w, 5.0 * k), (-w / 2.0, -r - 4.0 * k), (-w / 2.0, r + st.size)] {
                if placer.try_claim([x + dx, y + dy - st.size, x + dx + w, y + dy + 2.0]) {
                    svg.text(x + dx, y + dy, &v.name, &st, "start", 0.0);
                    break;
                }
            }
        }
    };

    // Region.
    {
        let f = Frame::new(world, &maps::REGION);
        let base = f.base_topo();
        let ov = Overlay::topo();
        let collar = Collar { title: "Grande Cache to Caw Ridge", subtitle: "Regional topographic map", notes: &["The waypoint is in Wildlife Management Unit 446, Kakwa River."], legend: maps::legend_topo(), credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, "region_topo", "Topographic", "Region: Grande Cache to Caw Ridge", "The whole approach on one sheet: Highway 40, Beaverdam Road, the ridge, the parks and the wildlife management units. Contours every 100 m.", &sheet, &bare, true)?;

        let sat = f.base_sat(0, 0.35);
        let mut ov = Overlay::thematic();
        ov.contours = false;
        ov.boundaries = true;
        let legend = maps::legend_topo().into_iter().take(8).collect();
        let sub = format!("Sentinel-2 natural colour, {}", &world.sat_region.date[..10]);
        let collar = Collar { title: "Grande Cache to Caw Ridge", subtitle: &sub, notes: &["Pale patches in the forest are cutblocks and well sites. Grey scars north of town are the coal mines."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&sat, &ov, &none, Some(&collar))?;
        let bare = f.compose(&sat, &ov, &none, None)?;
        sv.save(&f, "region_sat", "Satellite", "Region from space", &format!("A cloud free pass on {}. Roads, boundaries and names drawn over the image.", &world.sat_region.date[..10]), &sheet, &bare, false)?;

        let grey = f.base_topo();
        let mut ov = Overlay::topo();
        ov.wildlife = true;
        ov.contours = false;
        let mut legend: Vec<LegendItem> = vec![
            LegendItem { swatch: "<rect x=\"4\" y=\"-7\" width=\"30\" height=\"14\" fill=\"#7a3fa0\" fill-opacity=\"0.35\" stroke=\"#7a3fa0\" stroke-width=\"1.5\"/>".into(), label: "Mountain goat and bighorn sheep range".into() },
            LegendItem { swatch: "<rect x=\"4\" y=\"-7\" width=\"30\" height=\"14\" fill=\"#b8651b\" fill-opacity=\"0.25\" stroke=\"#b8651b\" stroke-width=\"1.5\"/>".into(), label: "Caribou range".into() },
        ];
        legend.extend(maps::legend_topo().into_iter().take(8));
        let gz = format!(
            "The waypoint is {} the core grizzly bear recovery zone, which covers {:.0} percent of this sheet.",
            if stats.grizzly_core { "inside" } else { "outside" },
            stats.grizzly_share * 100.0
        );
        let collar = Collar {
            title: "Wildlife ranges",
            subtitle: "Provincial wildlife sensitivity layers",
            notes: &["Caribou cannot be hunted. There is no mountain goat season in WMU 446.", &gz],
            legend,
            credits: &maps::CREDITS,
        };
        let sheet = f.compose(&grey, &ov, &none, Some(&collar))?;
        let bare = f.compose(&grey, &ov, &none, None)?;
        sv.save(&f, "region_wildlife", "Wildlife and land", "Wildlife ranges", "Where the province maps mountain goat and bighorn sheep range and the caribou ranges. Hatched purple is goat and sheep ground; hatched orange is caribou range.", &sheet, &bare, false)?;

        let mut ov = Overlay::topo();
        ov.coal = true;
        ov.contours = false;
        let mut legend: Vec<LegendItem> = vec![
            LegendItem { swatch: "<rect x=\"4\" y=\"-7\" width=\"30\" height=\"14\" fill=\"#1d1d1b\" fill-opacity=\"0.25\" stroke=\"#1d1d1b\" stroke-width=\"1.4\"/>".into(), label: "Coal lease".into() },
            LegendItem { swatch: "<rect x=\"4\" y=\"-7\" width=\"30\" height=\"14\" fill=\"#555\" fill-opacity=\"0.12\" stroke=\"#555\" stroke-width=\"1\" stroke-dasharray=\"5 4\"/>".into(), label: "Coal lease application".into() },
        ];
        legend.extend(maps::legend_topo().into_iter().take(8));
        let collar = Collar {
            title: "Coal leases and mine roads",
            subtitle: "Crown mineral agreements for coal",
            notes: &["A coal lease is a right to the mineral. It does not by itself close the surface.", "Active mine sites and private roads are closed to you: stay on Beaverdam Road."],
            legend,
            credits: &maps::CREDITS,
        };
        let sheet = f.compose(&grey, &ov, &none, Some(&collar))?;
        let bare = f.compose(&grey, &ov, &none, None)?;
        sv.save(&f, "region_coal", "Wildlife and land", "Coal leases and mine roads", "The waypoint sits inside an active Crown coal lease. This sheet shows every coal agreement in the region and the mine roads you must stay off.", &sheet, &bare, false)?;
    }

    // Ridge, printed large.
    {
        let f = Frame::new(world, &maps::RIDGE);
        let base = f.base_topo();
        let ov = Overlay::topo();
        let collar = Collar { title: "Caw Ridge", subtitle: "Topographic map", notes: &["The red dotted line is a computed walking line, not a trail."], legend: maps::legend_topo(), credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &route_extra, Some(&collar))?;
        let bare = f.compose(&base, &ov, &route_extra, None)?;
        sv.save(&f, "ridge_topo", "Topographic", "Caw Ridge", "The ridge and the roads that reach it, with 20 m contours, a 1 km UTM grid, streams traced from the elevation model and the ground cover as a tint.", &sheet, &bare, true)?;
    }
    // Close in, printed large.
    {
        let f = Frame::new(world, &maps::CLOSE);
        let base = f.base_topo();
        let ov = Overlay::topo();
        let both = |svg: &mut Svg, fr: &Frame, pl: &mut Placer| {
            route_extra(svg, fr, pl);
            vantage_extra(svg, fr, pl);
        };
        let mut legend = maps::legend_topo();
        legend.insert(0, LegendItem { swatch: "<path d=\"M19 -7L26 0L19 7L12 0Z\" fill=\"#ffd21f\" stroke=\"#1d1d1b\" stroke-width=\"1.6\"/>".into(), label: "Glassing point (G1 is best)".into() });
        let collar = Collar { title: "Caw Ridge: the waypoint", subtitle: "Topographic map, 10 m contours", notes: &["Contours are interpolated from 30 m data: small cliffs and benches will not show.", "The red dotted line is a computed walking line, not a trail."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &both, Some(&collar))?;
        let bare = f.compose(&base, &ov, &both, None)?;
        sv.save(&f, "close_topo", "Topographic", "Around the waypoint", "The ground within a morning's walk, with 10 m contours, the computed glassing points and the walking line from the end of the mapped track.", &sheet, &bare, true)?;
    }

    // Thematic sheets, one pixel per grid cell.
    for (sheet_def, prefix) in [(Sheet { scale: 1.0, ..maps::RIDGE }, "ridge"), (Sheet { scale: 1.0, ..maps::CLOSE }, "close")] {
        let f = Frame::new(world, &sheet_def);
        let close = prefix == "close";
        let place = if close { "Around the waypoint" } else { "Caw Ridge" };
        let ov = Overlay::thematic();

        if !close && !maps::DRY.load(std::sync::atomic::Ordering::Relaxed) {
            // The picture at the head of the overview page: the ridge from space, lit by its relief.
            let hero = f.base_sat(0, 0.45);
            let (hw, hh) = (2000usize, (2000.0 * hero.h as f32 / hero.w as f32) as usize);
            hero.resized(hw, hh).save_jpg(&out.join("maps/hero.jpg"), 80)?;
        }
        // Satellite.
        let mut ovs = ov.clone();
        ovs.contours = close;
        for (mode, id, name, blurb, note) in [
            (0u8, "sat", "from space", "Natural colour at 10 m. Dark green is conifer, olive and tan is open alpine, grey is rock, scree or mine.", "Natural colour: what the eye would see from above."),
            (1u8, "cir", "in colour infrared", "Near infrared shown as red. Healthy green growth is bright red, conifer is dark red, rock and cured grass are grey and tan, water is black.", "Colour infrared: the redder, the greener the growth."),
        ] {
            let base = f.base_sat(mode, 0.25);
            let sub = format!("Sentinel-2, {}", &world.sat_ridge.date[..10]);
            let collar = Collar { title: place, subtitle: &sub, notes: &[note], legend: maps::legend_topo().into_iter().take(6).collect(), credits: &maps::CREDITS };
            let sheet = f.compose(&base, &ovs, &none, Some(&collar))?;
            let bare = f.compose(&base, &ovs, &none, None)?;
            sv.save(&f, &format!("{prefix}_{id}"), "Satellite", &format!("{place} {name}"), &format!("{blurb} Pass of {}.", &world.sat_ridge.date[..10]), &sheet, &bare, false)?;
        }

        // Slope.
        let slope_ramp = Ramp::new(&[(0.0, "#f4f4ee"), (10.0, "#bfe3a8"), (20.0, "#fff07a"), (30.0, "#f9a23c"), (35.0, "#e34a33"), (40.0, "#9e2a8c"), (50.0, "#3b1053")]);
        let base = f.base_value(0.78, |_, _, i| {
            let s = f.terr.slope[i];
            if s < 10.0 { None } else { Some(slope_ramp.step(s)) }
        });
        let legend = maps::ramp_legend(&slope_ramp, &[(10.0, "10 to 20\u{b0}: steady walking"), (20.0, "20 to 30\u{b0}: steep, slow"), (30.0, "30 to 35\u{b0}: hands may be needed"), (35.0, "35 to 40\u{b0}: very steep"), (40.0, "40 to 50\u{b0}: escape terrain"), (50.0, "Over 50\u{b0}: cliff")]);
        let collar = Collar { title: place, subtitle: "Slope angle", notes: &["From 30 m data: slopes read low on short cliffs and gullies. Treat every colour as a minimum.", "Goats and sheep stay close to ground over 40\u{b0}. Early snow on 30 to 45\u{b0} lee slopes can avalanche."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &vantage_extra, Some(&collar))?;
        let bare = f.compose(&base, &ov, &vantage_extra, None)?;
        sv.save(&f, &format!("{prefix}_slope"), "Terrain", &format!("{place}: slope angle"), "How steep the ground is. Pale is easy, green is steady walking, yellow and orange are steep, red and purple are escape terrain.", &sheet, &bare, false)?;

        // Aspect.
        let aspect_cols = ["#2c5aa0", "#3f93c0", "#5fbf8f", "#c4d44a", "#f5b53f", "#ef7a3a", "#c4508f", "#6f4fa8"];
        let base = f.base_value(0.72, |_, _, i| {
            if f.terr.slope[i] < 5.0 {
                return None;
            }
            let a = f.terr.aspect[i];
            Some(crate::draw::hex(aspect_cols[(((a + 22.5) / 45.0) as usize) % 8]))
        });
        let legend = ["North", "North-east", "East", "South-east", "South", "South-west", "West", "North-west"].iter().zip(aspect_cols).map(|(n, c)| li(c, &format!("Faces {}", n.to_lowercase()))).collect();
        let collar = Collar { title: place, subtitle: "Aspect: which way the slope faces", notes: &["South and south-west slopes (orange) warm first and melt first. North slopes (blue) hold snow and shade.", "Morning thermals rise on sunlit slopes; evening air drains down the shaded ones."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, &format!("{prefix}_aspect"), "Terrain", &format!("{place}: aspect"), "Which way each slope faces. Warm colours face the sun; cool colours face away. Use it with the wind to plan an approach.", &sheet, &bare, false)?;

        // Ground cover.
        let base = f.base_value(0.80, |lon, lat, _| {
            let c = world.landcover.nearest(lon, lat);
            if c.is_nan() { None } else { Some(cover_colour(c as i32)) }
        });
        let legend = [1, 6, 5, 8, 10, 13, 14, 18].iter().map(|c| li(&rgb_hex(cover_colour(*c)), cover_name(*c))).collect();
        let tl = format!("Forest gives out at about {:.0} m near the waypoint (95 percent of forest cells lie below).", (stats.treeline / 10.0).round() * 10.0);
        let collar = Collar { title: place, subtitle: "Ground cover, 2020 Land Cover of Canada", notes: &[&tl, "A 30 m satellite classification from 2020: burns, cutblocks and mine work since then will not show."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, &format!("{prefix}_cover"), "Terrain", &format!("{place}: ground cover"), "Timber, shrub, alpine tundra and rock from the national land cover. The edges between timber and open ground are where to look at first and last light.", &sheet, &bare, false)?;

        // First sun.
        let sr = samples.first().map(|s| s.0).unwrap_or(8.0);
        let first_ramp = Ramp::new(&[(sr, "#fff3a0"), (sr + 0.5, "#ffd24d"), (sr + 1.0, "#fb9a3c"), (sr + 1.5, "#e8583e"), (sr + 2.0, "#b5367a"), (sr + 3.0, "#6a2a8a"), (sr + 4.5, "#2d2a6e")]);
        let base = f.base_value(0.80, |lon, lat, _| {
            let v = world.sample_ra(&sun_first, lon, lat);
            Some(if v.is_nan() { crate::draw::hex("#1b1b3a") } else { first_ramp.step(v) })
        });
        let tfmt = |h: f32| sun::hm(h as f64);
        let legend = vec![
            li(&rgb_hex(first_ramp.step(sr)), &format!("Lit by {}", tfmt(sr + 0.5))),
            li(&rgb_hex(first_ramp.step(sr + 0.5)), &format!("{} to {}", tfmt(sr + 0.5), tfmt(sr + 1.0))),
            li(&rgb_hex(first_ramp.step(sr + 1.0)), &format!("{} to {}", tfmt(sr + 1.0), tfmt(sr + 1.5))),
            li(&rgb_hex(first_ramp.step(sr + 1.5)), &format!("{} to {}", tfmt(sr + 1.5), tfmt(sr + 2.0))),
            li(&rgb_hex(first_ramp.step(sr + 2.0)), &format!("{} to {}", tfmt(sr + 2.0), tfmt(sr + 3.0))),
            li(&rgb_hex(first_ramp.step(sr + 3.0)), &format!("{} to {}", tfmt(sr + 3.0), tfmt(sr + 4.5))),
            li(&rgb_hex(first_ramp.step(sr + 4.5)), &format!("After {}", tfmt(sr + 4.5))),
            li("#1b1b3a", "No direct sun all day"),
        ];
        let sd = format!("First direct sun, {} {} {}, Mountain Daylight Time", config::SUN_DATE.2, sun::month_name(config::SUN_DATE.1), config::SUN_DATE.0);
        let collar = Collar { title: place, subtitle: &sd, notes: &["Shadows cast by the surrounding terrain are included. Cloud is not.", "Sunrise moves about two minutes later each day through October."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, &format!("{prefix}_sunrise"), "Sun and snow", &format!("{place}: first sun"), "When direct sunlight first reaches each piece of ground on an early October morning, with the shadows of the surrounding ridges counted.", &sheet, &bare, false)?;

        // Hours of sun.
        let hours_ramp = Ramp::new(&[(0.0, "#1b1b3a"), (2.0, "#3b3f8f"), (4.0, "#7a4fa3"), (6.0, "#c8567f"), (8.0, "#f08a4b"), (10.0, "#fbd25a")]);
        let base = f.base_value(0.80, |lon, lat, _| {
            let v = world.sample_ra(&sun_hours, lon, lat);
            if v.is_nan() { None } else { Some(hours_ramp.step(v)) }
        });
        let legend = maps::ramp_legend(&hours_ramp, &[(0.0, "Under 2 hours"), (2.0, "2 to 4 hours"), (4.0, "4 to 6 hours"), (6.0, "6 to 8 hours"), (8.0, "8 to 10 hours"), (10.0, "Over 10 hours")]);
        let sd = format!("Hours of direct sun, {} {} {}", config::SUN_DATE.2, sun::month_name(config::SUN_DATE.1), config::SUN_DATE.0);
        let collar = Collar { title: place, subtitle: &sd, notes: &["Dark ground stays cold, holds frost and keeps the first snow. Bright ground dries and melts first."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, &format!("{prefix}_sunhours"), "Sun and snow", &format!("{place}: hours of sun"), "How many hours of direct sun each slope gets on an early October day. It shows where snow and frost linger and where the ground dries.", &sheet, &bare, false)?;

        // First snow.
        let snow_ramp = Ramp::new(&[(0.0, "#3b1053"), (15.0, "#6a2a8a"), (30.0, "#2f5fa8"), (45.0, "#4fa3c8"), (61.0, "#a8dbc0"), (76.0, "#eef3b8")]);
        let base = f.base_value(0.80, |lon, lat, _| snow_median(lon, lat).map(|d| snow_ramp.step(d)));
        let legend = maps::ramp_legend(&snow_ramp, &[(0.0, "Before 15 September"), (15.0, "15 to 30 September"), (30.0, "1 to 15 October"), (45.0, "16 to 31 October"), (61.0, "1 to 15 November"), (76.0, "After 15 November")]);
        let collar = Collar { title: place, subtitle: "Usual date of the first snow that stays a while", notes: &["Median over the seven winters 2018 to 2025, read from Landsat and Sentinel-2 by Natural Resources Canada.", "Dates are good to about a week: satellites only see the ground between clouds."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &none, Some(&collar))?;
        let bare = f.compose(&base, &ov, &none, None)?;
        sv.save(&f, &format!("{prefix}_snow"), "Sun and snow", &format!("{place}: first snow"), "The usual start of the first snow period of the autumn, from seven winters of satellite records. The ridge top whitens weeks before the valleys.", &sheet, &bare, false)?;

        // Walking time.
        let time_ramp = Ramp::new(&[(0.0, "#1a9850"), (0.5, "#66bd63"), (1.0, "#b8e186"), (1.5, "#fee08b"), (2.0, "#fdae61"), (3.0, "#f46d43"), (4.0, "#d73027"), (5.0, "#8e0152"), (6.0, "#40004b")]);
        let base = f.base_value(0.75, |lon, lat, _| {
            let v = world.sample_ra(&t_out, lon, lat);
            if v.is_finite() { Some(time_ramp.step(v)) } else { None }
        });
        let legend = maps::ramp_legend(&time_ramp, &[(0.0, "Under 30 minutes"), (0.5, "30 to 60 minutes"), (1.0, "1 to 1.5 hours"), (1.5, "1.5 to 2 hours"), (2.0, "2 to 3 hours"), (3.0, "3 to 4 hours"), (4.0, "4 to 5 hours"), (5.0, "Over 5 hours")]);
        let collar = Collar { title: place, subtitle: "Walking time out from the waypoint", notes: &["Tobler's hiking function: full speed on roads and tracks, three fifths of it off them.", "No pack, snow, deadfall, willow or creek is counted. Walking back up takes longer. Allow half as much again."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &route_extra, Some(&collar))?;
        let bare = f.compose(&base, &ov, &route_extra, None)?;
        sv.save(&f, &format!("{prefix}_walk"), "Terrain", &format!("{place}: walking time"), "How long it takes to walk from the waypoint to anywhere on the sheet, from slope alone. Use it to judge how far a pack-out would be.", &sheet, &bare, false)?;

        // Viewshed from the waypoint.
        let base = f.base_value(0.62, |lon, lat, _| {
            let (x, y) = ra.px(lon, lat);
            if x < 0.0 || y < 0.0 || x as usize >= ra.w || y as usize >= ra.h {
                return None;
            }
            let v = view_wpt[y as usize * ra.w + x as usize];
            if v.is_nan() { None } else if v > 0.5 { Some(crate::draw::hex("#ffd21f")) } else { Some(crate::draw::hex("#3a3f55")) }
        });
        let legend = vec![li("#ffd21f", "In view from the waypoint"), li("#3a3f55", "Hidden by terrain"), li("#ececE8", "Beyond 8 km")];
        let note = format!("From the waypoint you command {:.1} km2, {:.0} percent of the ground within 8 km.", stats.seen_km2_wpt, stats.seen_share_wpt * 100.0);
        let collar = Collar { title: place, subtitle: "What can be seen from the waypoint", notes: &[&note, "Eye at 1.7 m, animal back at 1.0 m, bare ground: trees hide more than this shows. It also shows where you are skylined."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &vantage_extra, Some(&collar))?;
        let bare = f.compose(&base, &ov, &vantage_extra, None)?;
        sv.save(&f, &format!("{prefix}_view"), "Glassing", &format!("{place}: in view from the waypoint"), "Every piece of ground a standing person can see from the waypoint, and every piece that is hidden. What you can see can see you.", &sheet, &bare, false)?;

        // Vantage points.
        let count_ramp = Ramp::new(&[(0.0, "#3a3f55"), (1.0, "#4f8fc0"), (2.0, "#6cc08b"), (3.0, "#d9e04a"), (5.0, "#ffb02e")]);
        let base = f.base_value(0.62, |lon, lat, _| {
            let (x, y) = ra.px(lon, lat);
            if x < 0.0 || y < 0.0 || x as usize >= ra.w || y as usize >= ra.h {
                return None;
            }
            let v = seen_count[y as usize * ra.w + x as usize];
            if v.is_nan() { None } else { Some(count_ramp.step(v)) }
        });
        let mut legend = maps::ramp_legend(&count_ramp, &[(0.0, "Seen from none of them"), (1.0, "Seen from one point"), (2.0, "Seen from two"), (3.0, "Seen from three or four"), (5.0, "Seen from five or more")]);
        legend.insert(0, LegendItem { swatch: "<path d=\"M19 -7L26 0L19 7L12 0Z\" fill=\"#ffd21f\" stroke=\"#1d1d1b\" stroke-width=\"1.6\"/>".into(), label: "Glassing point, ranked G1 to G8".into() });
        let collar = Collar { title: place, subtitle: "Glassing points and the ground they cover", notes: &["Points are ranked by the open ground in view between 300 m and 3 km.", "Dark ground is dead ground: nothing sees into it. Animals bed there, and you can move there unseen."], legend, credits: &maps::CREDITS };
        let sheet = f.compose(&base, &ov, &vantage_extra, Some(&collar))?;
        let bare = f.compose(&base, &ov, &vantage_extra, None)?;
        sv.save(&f, &format!("{prefix}_glass"), "Glassing", &format!("{place}: glassing points"), "The eight best places to sit behind glass, found by testing hundreds of spots for how much open ground each one sees, and the ground they cover between them.", &sheet, &bare, false)?;
        let _ = &score_grid;
    }

    Ok(Products { maps: sv.maps, vantages, drive, drive_profile, track_end, walk, walk_profile, walk_out_h, walk_back_h, snow: snow_rows, stats })
}
