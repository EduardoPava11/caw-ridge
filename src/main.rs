mod cog;
mod config;
mod dem;
mod geo;
mod sat;
mod vector;
mod view;
mod contour;
mod analysis;
mod sun;
mod wmm;
mod draw;
mod world;
mod maps;
mod route;
mod products;
mod climate;
mod chart;
mod export;
mod site;

use std::path::Path;

use cog::Res;

fn fetch() -> Res<()> {
    let data = Path::new("data");
    std::fs::create_dir_all(data)?;
    if !data.join("dem.png").exists() {
        eprintln!("fetching elevation (MRDEM-30 DTM)");
        let d = dem::SrcDem::fetch(config::MRDEM_DTM, config::REGION, 1500.0)?;
        d.save(data, "dem", "Natural Resources Canada, MRDEM-30 digital terrain model, metres CGVD2013")?;
    }
    if !data.join("landcover.png").exists() {
        eprintln!("fetching land cover 2020");
        dem::SrcDem::fetch(config::LANDCOVER, config::REGION, 500.0)?.save(
            data,
            "landcover",
            "Natural Resources Canada, 2020 Land Cover of Canada, class codes of the North American Land Change Monitoring System",
        )?;
    }
    let snow_dir = data.join("snow");
    std::fs::create_dir_all(&snow_dir)?;
    for w in config::SNOW_WINTERS {
        for layer in ["startF", "startF_u", "startB"] {
            let name = format!("{layer}_{w}");
            if !snow_dir.join(format!("{name}.png")).exists() {
                eprintln!("fetching snow onset {name}");
                let url = format!("{}HLS_Fmask_v1_1_snow_{layer}_winterYear{w}_Canada.tif", config::SNOW_BASE);
                dem::SrcDem::fetch(&url, config::RIDGE, 500.0)?.save(
                    &snow_dir,
                    &name,
                    "Natural Resources Canada, annual 30 m snow dynamics; days relative to 31 December of the first year of the winter",
                )?;
            }
        }
    }
    let vec_dir = data.join("vector");
    std::fs::create_dir_all(&vec_dir)?;
    for (name, url) in config::ARCGIS {
        let p = vec_dir.join(format!("{name}.geojson"));
        if !p.exists() {
            eprintln!("fetching {name}");
            std::fs::write(&p, vector::fetch_arcgis(url, config::REGION)?)?;
        }
    }
    if !vec_dir.join("names.json").exists() {
        eprintln!("fetching official place names");
        std::fs::write(vec_dir.join("names.json"), vector::fetch_names(config::REGION)?)?;
    }
    if !vec_dir.join("osm.json").exists() {
        eprintln!("fetching OpenStreetMap");
        std::fs::write(vec_dir.join("osm.json"), vector::fetch_overpass(config::REGION)?)?;
    }
    if !data.join("climate.json").exists() {
        eprintln!("fetching ten autumns of weather history");
        std::fs::write(data.join("climate.json"), climate::fetch_history()?)?;
    }
    // The forecasts are refreshed on every fetch; a failure keeps the last one.
    for (name, lat, lon) in [("forecast.json", config::WPT_LAT, config::WPT_LON), ("forecast_ridge.json", config::RIDGE_LAT, config::RIDGE_LON)] {
        match climate::fetch_forecast(lat, lon) {
            Ok(s) => {
                let mut v: serde_json::Value = serde_json::from_str(&s)?;
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
                let (y, m, d) = sun::civil_from_days((now / 86400) as i64);
                v["fetched"] = serde_json::json!(format!("{y}-{m:02}-{d:02} {:02}:{:02} UTC", (now % 86400) / 3600, (now % 3600) / 60));
                std::fs::write(data.join(name), serde_json::to_string(&v)?)?;
                eprintln!("{name} refreshed");
            }
            Err(e) => eprintln!("{name} not refreshed: {e}"),
        }
    }
    let cache = data.join("cache");
    std::fs::create_dir_all(&cache)?;
    if !cache.join("sat_ridge.png").exists() || !cache.join("sat_region.png").exists() {
        eprintln!("searching for the latest clear Sentinel-2 pass");
        let found = sat::search(config::WPT_LON, config::WPT_LAT, config::SAT_FROM, config::SAT_TO, 15.0)?;
        let order = sat::choose(&found, 11, &["11ULA", "11ULV"]);
        for s in &order {
            eprintln!("  candidate {} cloud {:.1}%", s.id, s.cloud);
        }
        eprintln!("reading the ridge at 10 m");
        sat::Sat::fetch(config::RIDGE, 10.0, &order)?.save(&cache, "sat_ridge")?;
        eprintln!("reading the region at 20 m");
        sat::Sat::fetch(config::REGION, 20.0, &order)?.save(&cache, "sat_region")?;
    }
    Ok(())
}

fn main() -> Res<()> {
    let cmd = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let cmd = cmd.as_str();
    match cmd {
        "fetch" => fetch()?,
        "build" | "all" | "pages" => {
            if cmd == "pages" {
                maps::DRY.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            if cmd == "all" {
                fetch()?;
            }
            let out = Path::new("docs");
            let w = world::World::load()?;
            let clim = climate::load_history(Path::new("data"))?;
            // The prevailing wind of the season, from the record, sets the shelter map.
            let top = clim.rose.sectors.iter().enumerate().max_by(|a, b| a.1 .0.partial_cmp(&b.1 .0).unwrap()).map(|s| s.0).unwrap_or(6);
            let p = products::build(&w, out, top as f32 * 45.0)?;
            eprintln!(
                "waypoint {:.0} m, faces {:.0} at {:.0} deg, sees {:.2} km2 ({:.1}%), rim {:?}, cover {:?}, treeline {:.0}, climb {:?}, walk up {:.2} h down {:.2} h, ridge walk {:.2} h, G1 {:?}",
                w.wpt_elev,
                p.stats.wpt_aspect,
                p.stats.wpt_slope,
                p.stats.seen_km2_wpt,
                p.stats.seen_share_wpt * 100.0,
                p.stats.rim,
                p.stats.cover_3km.iter().take(3).collect::<Vec<_>>(),
                p.stats.treeline,
                p.climb.iter().map(|l| (l.kind.clone(), (l.km * 10.0).round() / 10.0)).collect::<Vec<_>>(),
                p.foot_up_h,
                p.foot_down_h,
                p.walk_out_h,
                p.vantages.first().map(|v| (v.name.clone(), v.open_km2, v.dist_m.round(), v.bearing.round(), v.elev.round()))
            );
            eprintln!("official layers at the waypoint: WMU {:?}, caribou {:?}, grizzly core {} (covers {:.0}% of the region), coal lease {:?}, goat and sheep range {}, park {:?}", p.stats.wmu, p.stats.caribou_range, p.stats.grizzly_core, p.stats.grizzly_share * 100.0, p.stats.coal_lease, p.stats.goat_sheep_range, p.stats.park);
            let fc = climate::load_forecast(Path::new("data"), "forecast.json")?;
            let fc_ridge = climate::load_forecast(Path::new("data"), "forecast_ridge.json")?;
            // The pages on the regulations are written for one unit. If the waypoint moves
            // out of it, stop rather than print the wrong rules.
            if !p.stats.wmu.starts_with("446 ") {
                return Err(format!("the waypoint is in WMU {}, but the regulations text in src/site.rs is written for WMU 446", p.stats.wmu).into());
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
            // The build date in Alberta's clock, so that "today" on the pages is the hunter's today.
            let utc_days = (now / 86400) as i64;
            let (off, zone) = sun::alberta_offset(utc_days);
            let local = now as i64 + (off * 3600.0) as i64;
            let days = local.div_euclid(86400);
            let (y, m, d) = sun::civil_from_days(days);
            let built = format!("{d} {} {y}, {:02}:{:02} {zone}", sun::month_name(m), local.rem_euclid(86400) / 3600, local.rem_euclid(3600) / 60);
            site::build(&w, &p, &clim, &fc, &fc_ridge, out, days, &built)?;
        }
        _ => eprintln!("usage: cawridge [all | fetch | build | pages]\n  fetch  download the data into data/\n  build  draw the maps and write the site into docs/\n  pages  rewrite the pages only, keeping the maps already drawn\n  all    fetch, then build (the default)"),
    }
    Ok(())
}
