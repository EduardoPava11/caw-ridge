//! The pages. Text is written here, numbers come from the analysis.

use std::fmt::Write as _;
use std::path::Path;

use crate::chart::{self, Bar, LightDay, Mark, Span};
use crate::climate::{self, Climate, Forecast};
use crate::cog::Res;
use crate::config;
use crate::draw::esc;
use crate::export::{self, Place};
use crate::geo;
use crate::products::Products;
use crate::sun;
use crate::view::View;
use crate::world::World;

const NAV: [(&str, &str); 10] = [
    ("index.html", "Overview"),
    ("maps.html", "Maps"),
    ("explore.html", "Explore"),
    ("terrain.html", "Terrain"),
    ("access.html", "Access"),
    ("regulations.html", "Regulations"),
    ("wildlife.html", "Wildlife"),
    ("weather.html", "Weather"),
    ("safety.html", "Safety"),
    ("sources.html", "Sources"),
];

struct Page<'a> {
    slug: &'a str,
    title: &'a str,
    description: &'a str,
    body: String,
    head: &'a str,
    scripts: &'a str,
    footer: bool,
}

fn layout(p: &Page, built: &str) -> String {
    let mut nav = String::new();
    for (href, name) in NAV {
        let cur = if href == p.slug { " aria-current=\"page\"" } else { "" };
        let _ = write!(nav, "<a href=\"{href}\"{cur}>{name}</a>");
    }
    let title = if p.slug == "index.html" { "Caw Ridge: a hunter's field guide".to_string() } else { format!("{} | Caw Ridge", p.title) };
    let footer = if p.footer {
        format!(
            "<footer><div class=\"wrap\"><p><b>A planning aid, not an authority.</b> Regulations, boundaries, road access and conditions change. The Alberta Guide to Hunting Regulations and the Wildlife Regulation govern; so do posted signs and the people who manage the land. Confirm before you hunt.</p><p>Built {built} by the <code>cawridge</code> generator, written in Rust. Elevation and land cover from Natural Resources Canada, boundaries from the Government of Alberta, roads and trails from OpenStreetMap contributors, imagery from Copernicus Sentinel-2, weather from Open-Meteo. Full credits on the <a href=\"sources.html\">sources page</a>.</p></div></footer>"
        )
    } else {
        String::new()
    };
    format!(
        "<!doctype html>\n<html lang=\"en-CA\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n<title>{}</title>\n<meta name=\"description\" content=\"{}\">\n<meta name=\"theme-color\" content=\"#1d1d1b\">\n<link rel=\"manifest\" href=\"manifest.webmanifest\">\n<link rel=\"icon\" href=\"assets/icon.svg\" type=\"image/svg+xml\">\n<link rel=\"apple-touch-icon\" href=\"assets/icon-180.png\">\n<link rel=\"preload\" href=\"assets/fonts/BarlowCondensed-Bold.woff2\" as=\"font\" type=\"font/woff2\" crossorigin>\n<link rel=\"stylesheet\" href=\"assets/style.css\">\n{}\n<script>try{{var t=localStorage.getItem('cawridge-theme');if(t)document.documentElement.setAttribute('data-theme',t)}}catch(e){{}}</script>\n</head>\n<body>\n<header class=\"mast\"><div class=\"wrap\"><a class=\"brand\" href=\"index.html\"><b>Caw Ridge</b><span>54.0627 N 119.3907 W</span></a><nav aria-label=\"Sections\">{}</nav><button class=\"theme\" type=\"button\">Night</button></div></header>\n{}\n{}\n<script src=\"assets/app.js\" defer></script>\n{}\n</body>\n</html>\n",
        esc(&title),
        esc(p.description),
        p.head,
        nav,
        p.body,
        footer,
        p.scripts
    )
}

fn date_long(days: i64) -> String {
    let (y, m, d) = sun::civil_from_days(days);
    format!("{} {} {} {}", sun::weekday(days), d, sun::month_name(m), y)
}

fn date_short(days: i64) -> String {
    let (_, m, d) = sun::civil_from_days(days);
    format!("{} {}", d, sun::month_name(m))
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) if w.len() > 3 => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                _ => w.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

fn map_figure(p: &Products, id: &str, caption: &str) -> String {
    match p.maps.iter().find(|m| m.id == id) {
        Some(m) => format!(
            "<figure class=\"map\"><a href=\"viewer.html?m={id}\"><img src=\"maps/mid/{id}.jpg\" alt=\"{}\" loading=\"lazy\" width=\"1600\" height=\"1360\" style=\"width:100%;height:auto\"></a><figcaption><b>{}.</b> {} <a href=\"viewer.html?m={id}\">Open the full map</a>.</figcaption></figure>",
            esc(&m.title),
            esc(&m.title),
            caption
        ),
        None => String::new(),
    }
}

pub fn places(world: &World, p: &Products) -> Vec<Place> {
    let z = |lon: f64, lat: f64| world.dem.sample(lon, lat);
    let mut v = vec![Place {
        name: "Waypoint".into(),
        kind: "waypoint",
        lon: config::WPT_LON,
        lat: config::WPT_LAT,
        elev: world.wpt_elev,
        note: "The point this guide is built around. It sits in a high bowl with a short view.".into(),
    }];
    for g in &p.vantages {
        v.push(Place {
            name: format!("{} glassing point", g.name),
            kind: "glassing",
            lon: g.lon,
            lat: g.lat,
            elev: g.elev,
            note: format!("Sees {:.1} km2 of open ground within 3 km. {:.1} km from the waypoint, bearing {:.0} true.", g.open_km2, g.dist_m / 1000.0, g.bearing),
        });
    }
    v.push(Place { name: "End of mapped track".into(), kind: "road", lon: p.track_end.0, lat: p.track_end.1, elev: z(p.track_end.0, p.track_end.1), note: "Where the track drawn in OpenStreetMap stops. Riders report old roads along most of the ridge line.".into() });
    if let Some(l) = p.drive.iter().find(|l| l.kind == "track") {
        let q = l.pts[0];
        v.push(Place { name: "Caw Ridge turnoff".into(), kind: "road", lon: q.0, lat: q.1, elev: z(q.0, q.1), note: format!("Leave the gravel here. {:.1} km of old exploration road climbs to the ridge. A staging area is reported at the turnoff.", l.km) });
    }
    if let Some(l) = p.drive.iter().find(|l| l.kind == "gravel") {
        let q = l.pts[0];
        v.push(Place { name: "Beaverdam Road turnoff".into(), kind: "road", lon: q.0, lat: q.1, elev: z(q.0, q.1), note: "Leave Highway 40 here, 8 km north of Grande Cache. The road crosses coal mine property: stay on Beaverdam Road.".into() });
    }
    for (name, kind, lon, lat, note) in [
        ("Grande Cache", "service", config::TOWN.0, config::TOWN.1, "Fuel, groceries, lodging, tourism centre. Last services."),
        ("Grande Cache hospital", "service", -119.1189092, 53.890938, "Grande Cache Community Health Complex, 10200 Shand Avenue. Emergency department open 24 hours. Helipad."),
        ("Fuel: Fas Gas", "service", -119.1134877, 53.8885839, "Fuel in Grande Cache."),
        ("Fuel: Petro-Canada", "service", -119.1159509, 53.8917315, "Fuel in Grande Cache."),
        ("Sheep Creek campground", "camp", -119.011219, 54.062174, "Provincial recreation area on Highway 40, 25 km north of Grande Cache. Boat launch."),
        ("Smoky River South campground", "camp", -119.158516, 53.890360, "Provincial recreation area, 22 unserviced sites. Usually open to Thanksgiving."),
        ("Sulphur Gates campground", "camp", -119.186183, 53.850132, "Provincial recreation area and Willmore staging. 11 sites, open all year. Firearm discharge permit rules apply here."),
    ] {
        v.push(Place { name: name.into(), kind, lon, lat, elev: z(lon, lat), note: note.into() });
    }
    v
}

pub struct SunRow {
    pub days: i64,
    pub legal_start: f64,
    pub rise: f64,
    pub set: f64,
    pub legal_end: f64,
    pub civil_start: f64,
    pub civil_end: f64,
    pub zone: &'static str,
    pub moon_frac: f64,
    pub moon_name: &'static str,
}

pub fn sun_rows(from: i64, to: i64) -> Vec<SunRow> {
    let mut v = Vec::new();
    for d in from..=to {
        let (off, zone) = sun::alberta_offset(d);
        let Some((r, s, _)) = sun::crossing(d, config::WPT_LON, config::WPT_LAT, 90.833) else { continue };
        let (cr, cs, _) = sun::crossing(d, config::WPT_LON, config::WPT_LAT, 96.0).unwrap_or((r, s, 0.0));
        // The moon as it stands at local midnight at the start of the night.
        let (mf, _, mn) = sun::moon(d, 6.0 + 24.0);
        v.push(SunRow { days: d, legal_start: r + off - 0.5, rise: r + off, set: s + off, legal_end: s + off + 0.5, civil_start: cr + off, civil_end: cs + off, zone, moon_frac: mf, moon_name: mn });
    }
    v
}

#[allow(clippy::too_many_arguments)]
pub fn build(world: &World, p: &Products, clim: &Climate, fc: &Forecast, out: &Path, built_days: i64, built: &str) -> Res<()> {
    std::fs::create_dir_all(out.join("assets/fonts"))?;
    std::fs::create_dir_all(out.join("data"))?;
    std::fs::create_dir_all(out.join("vendor/leaflet/images"))?;
    for f in ["style.css", "app.js", "viewer.js", "explore.js", "icon.svg"] {
        std::fs::copy(Path::new("assets/site").join(f), out.join("assets").join(f))?;
    }
    for e in std::fs::read_dir("assets/webfonts")? {
        let e = e?;
        std::fs::copy(e.path(), out.join("assets/fonts").join(e.file_name()))?;
    }
    for f in ["leaflet.js", "leaflet.css", "LICENSE", "images/marker-icon.png", "images/marker-icon-2x.png", "images/marker-shadow.png", "images/layers.png", "images/layers-2x.png"] {
        std::fs::copy(Path::new("assets/vendor/leaflet").join(f), out.join("vendor/leaflet").join(f))?;
    }
    std::fs::write(out.join(".nojekyll"), "")?;

    // The home screen icon, drawn from the same SVG.
    let icon_svg = std::fs::read_to_string("assets/site/icon.svg")?;
    for size in [180usize, 512] {
        let scaled = icon_svg.replace("<svg ", &format!("<svg width=\"{size}\" height=\"{size}\" "));
        let pm = crate::draw::rasterise(&scaled, size, size, &world.fonts)?;
        pm.save_png(out.join(format!("assets/icon-{size}.png")))?;
    }

    let places = places(world, p);
    std::fs::write(out.join("data/cawridge.gpx"), export::gpx(&places, p))?;
    std::fs::write(out.join("data/cawridge.kml"), export::kml(&places, p))?;
    std::fs::write(out.join("data/cawridge.geojson"), export::geojson(&places, p))?;
    for id in ["ridge_topo", "close_topo", "ridge_sat", "close_slope"] {
        if let Some(m) = p.maps.iter().find(|m| m.id == id) {
            export::kmz(&out.join(format!("data/{id}.kmz")), out, m, &places, p)?;
        }
    }
    // Official boundaries, passed on unchanged for anyone who wants them in their own GIS.
    std::fs::copy("data/vector/wmu.geojson", out.join("data/wmu.geojson"))?;

    // Heights for the explorer, packed into the colour channels of a PNG.
    let ev = View::new(config::RIDGE, 20.0);
    let ez = ev.elevation(&world.dem);
    let mut buf = Vec::with_capacity(ev.w * ev.h * 3);
    for v in &ez {
        let t = (v + 32768.0).max(0.0);
        buf.extend_from_slice(&[(t / 256.0).floor() as u8, (t.floor() % 256.0) as u8, (t.fract() * 256.0) as u8]);
    }
    image::save_buffer(out.join("data/elev_ridge.png"), &buf, ev.w as u32, ev.h as u32, image::ExtendedColorType::Rgb8)?;

    let maps_json: Vec<serde_json::Value> = p
        .maps
        .iter()
        .map(|m| serde_json::json!({"id": m.id, "group": m.group, "title": m.title, "blurb": m.blurb, "view": m.view, "download": m.download, "bare": m.bare, "w": m.bbox.w, "s": m.bbox.s, "e": m.bbox.e, "n": m.bbox.n}))
        .collect();
    std::fs::write(out.join("maps.json"), serde_json::to_string(&maps_json)?)?;

    let light = sun_rows(built_days.min(sun::days_from_civil(2026, 9, 28)), sun::days_from_civil(2026, 11, 30));
    let today = sun_rows(built_days, built_days).into_iter().next();
    let town_km = geo::haversine(config::TOWN.0, config::TOWN.1, config::WPT_LON, config::WPT_LAT) / 1000.0;
    let town_brg = geo::bearing(config::TOWN.0, config::TOWN.1, config::WPT_LON, config::WPT_LAT);
    let leg_km = |k: &str| -> f64 { p.drive.iter().filter(|l| l.kind == k).map(|l| l.km).sum() };
    let (km_hwy, km_gravel, km_track) = (leg_km("highway") + leg_km("town"), leg_km("gravel"), leg_km("track"));
    let km_total = km_hwy + km_gravel + km_track;
    let g1 = &p.vantages[0];
    let sat_date = &world.sat_ridge.date[..10];

    // The forecast strip, as it stood at build time.
    let mut strip = String::new();
    for d in &fc.days {
        let dd = sun::days_from_civil(d.date[..4].parse()?, d.date[5..7].parse()?, d.date[8..10].parse()?);
        let wet = if d.snow >= 0.1 { format!("{:.1} cm snow", d.snow) } else if d.precip >= 0.1 { format!("{:.1} mm rain", d.precip) } else { "Dry".to_string() };
        let _ = write!(
            strip,
            "<div><div class=\"d\">{} {}</div><div class=\"t\">{:.0}&deg; <i>{:.0}&deg;</i></div><div class=\"w\">{}</div><div class=\"s\">{}</div><div class=\"w\">{} {:.0}, gusts {:.0}</div></div>",
            sun::weekday(dd),
            date_short(dd),
            d.tmax,
            d.tmin,
            climate::code_words(d.code),
            wet,
            geo::compass(d.dir as f64).chars().take(2).collect::<String>(),
            d.wind,
            d.gust
        );
    }
    let forecast_block = format!(
        "<div class=\"fc\" data-forecast=\"{}\">{}</div><p class=\"stamp\" data-forecast-stamp>Forecast as of {}. With a connection this strip refreshes itself.</p><p class=\"small\">Highs and lows in &deg;C at {:.0} m, wind in km/h. A global model on a coarse grid: ridge top wind will run stronger than shown.</p>",
        esc(&climate::forecast_url()),
        strip,
        esc(&fc.fetched),
        world.wpt_elev
    );
    let snow_soon: f32 = fc.days.iter().take(4).map(|d| d.snow).sum();
    let cold = fc.days.iter().take(7).map(|d| d.tmin).fold(f32::MAX, f32::min);
    let gust = fc.days.iter().take(7).map(|d| d.gust).fold(f32::MIN, f32::max);

    let mut pages: Vec<(String, String)> = Vec::new();
    let mut emit = |pg: Page| {
        pages.push((pg.slug.to_string(), layout(&pg, built)));
    };

    // ------------------------------------------------------------ overview
    {
        let mut b = String::new();
        let _ = write!(
            b,
            "<section class=\"hero\"><img src=\"maps/hero.jpg\" alt=\"Caw Ridge from space: a pale alpine ridge running north-west above dark forest\" width=\"2000\" height=\"1100\"><div class=\"over\"><div class=\"wrap\"><p class=\"kicker\">A hunter's field guide, Alberta WMU 446</p><h1>Caw Ridge</h1><div class=\"coords\"><span>{:.6}, {:.6}</span><span>{}</span><span>UTM {}</span><span>{:.0} m</span></div></div></div></section>",
            config::WPT_LAT,
            config::WPT_LON,
            format!("{} {}", geo::dms(config::WPT_LAT, 'N', 'S'), geo::dms(config::WPT_LON, 'E', 'W')),
            geo::utm_string(config::WPT_LON, config::WPT_LAT),
            world.wpt_elev
        );
        b.push_str("<main class=\"wrap\">");
        let _ = write!(
            b,
            "<p class=\"lede\" style=\"margin-top:34px\">Caw Ridge is a high, open ridge in the foothills {:.0} km north-west of Grande Cache: alpine tundra and rock above a sea of spruce and pine, the home of the most studied mountain goat herd on earth, and the far end of a rough road. This guide gathers what the public record knows about the ground around your waypoint, draws it as maps, and sets out the rules and the risks.</p>",
            town_km
        );
        let (tr, ts) = today.as_ref().map(|t| (sun::hm(t.rise), sun::hm(t.set))).unwrap_or_default();
        let (tl0, tl1, tz) = today.as_ref().map(|t| (sun::hm(t.legal_start), sun::hm(t.legal_end), t.zone)).unwrap_or_default();
        let _ = write!(
            b,
            "<dl class=\"facts\">\
<div class=\"fact\"><dt>Elevation</dt><dd>{} m<small>{} ft. In the alpine: forest gives out near {:.0} m.</small></dd></div>\
<div class=\"fact\"><dt>Wildlife management unit</dt><dd>WMU 446<small>Kakwa River. Checked against the official boundary file.</small></dd></div>\
<div class=\"fact\"><dt>From Grande Cache</dt><dd>{:.0} km by road<small>{:.0} km paved, {:.0} km gravel, {:.0} km rough track. {:.0} km in a straight line.</small></dd></div>\
<div class=\"fact\"><dt>Land</dt><dd>Crown land<small>{} Treaty 8 territory.</small></dd></div>\
<div class=\"fact\"><dt>Legal light, {}</dt><dd>{} to {}<small>Sunrise {}, sunset {} {}. Half an hour either side.</small></dd></div>\
<div class=\"fact\"><dt>Compass</dt><dd>{:.1}&deg; east<small>Magnetic declination, autumn 2026. Grid north is {:.1}&deg; west of true.</small></dd></div>\
<div class=\"fact\"><dt>Coordinates</dt><dd class=\"mono\">{:.6}, {:.6}<br>{}</dd></div>\
<div class=\"fact\"><dt>Latest clear satellite pass</dt><dd>{}<small>No snow on the ridge that day. See the <a href=\"viewer.html?m=ridge_sat\">image</a>.</small></dd></div>\
</dl>",
            chart::thousands(world.wpt_elev),
            chart::thousands(world.wpt_elev * 3.28084),
            (p.stats.treeline / 10.0).round() * 10.0,
            km_total,
            km_hwy,
            km_gravel,
            km_track,
            town_km,
            format!(
                "{} {}",
                match &p.stats.park { Some(n) => format!("Inside {n}."), None => "Not in a park.".to_string() },
                match &p.stats.coal_lease { Some(l) => format!("Inside coal lease {}.", l.0), None => "Not inside a coal lease.".to_string() }
            ),
            date_short(built_days),
            tl0,
            tl1,
            tr,
            ts,
            tz,
            world.declination,
            world.convergence.abs(),
            config::WPT_LAT,
            config::WPT_LON,
            geo::utm_string(config::WPT_LON, config::WPT_LAT),
            sat_date
        );
        b.push_str("<h2>Read this first</h2><ol class=\"stoplist\">");
        let _ = write!(
            b,
            "<li><div><b>There is no mountain goat season here.</b><span>WMU 446 has no goat season of any kind. The Caw Ridge goats are a research herd, followed animal by animal since 1989 and not hunted since 1969. Many carry ear tags or collars. Leave them be, and give them room.</span></div></li>\
<li><div><b>Caribou and grizzly bear are closed everywhere in Alberta.</b><span>The waypoint is inside the Redrock-Prairie Creek caribou range and the core grizzly recovery zone. Be certain of your target: a caribou is not an elk, and a grizzly is not a black bear.</span></div></li>\
<li><div><b>Know what your tag allows.</b><span>In 446, elk must be six point or better, moose in the rifle season is by draw, bighorn sheep is by draw only, and deer are on a general tag. The <a href=\"regulations.html\">regulations page</a> has the table.</span></div></li>\
<li><div><b>No weapon on a quad before noon.</b><span>In WMUs 400 to 446 it is unlawful to carry a weapon on an off-highway vehicle from one hour before sunrise until noon during an open big game season. The last {:.0} km to the ridge is an off-highway vehicle trail.</span></div></li>\
<li><div><b>The road in crosses a working coal mine.</b><span>Beaverdam Road runs through mine property. The municipal district's instruction is plain: stay on Beaverdam Road. Mine roads are private.</span></div></li>\
<li><div><b>Your waypoint sits in a bowl.</b><span>From the waypoint a standing hunter sees only {:.1} km&sup2; of ground. Walk {:.1} km to glassing point {} and you command {:.1} km&sup2; of open country. The <a href=\"terrain.html\">terrain page</a> shows where to sit.</span></div></li>\
<li><div><b>Winter comes early at {:.0} m.</b><span>The first snow that stays a while has arrived between late September and late October in most recent years. The forecast when this page was built: {} over the next four days, a low of {:.0}&deg;C and gusts to {:.0} km/h this week.</span></div></li>\
<li><div><b>Plan on no phone signal.</b><span>Coverage is confirmed only in town. Carry a satellite messenger, leave a trip plan, and know that the nearest emergency room is in Grande Cache, {:.0} km back down the road.</span></div></li></ol>",
            km_track,
            p.stats.seen_km2_wpt,
            g1.dist_m / 1000.0,
            g1.name,
            g1.open_km2,
            world.wpt_elev,
            if snow_soon >= 0.5 { format!("{snow_soon:.0} cm of snow") } else { "no snow".to_string() },
            cold,
            gust,
            km_total
        );
        b.push_str("<h2>The weather on the ridge</h2>");
        b.push_str(&forecast_block);
        b.push_str("<p><a href=\"weather.html\">Ten autumns of weather, first snow dates, and sun and moon tables</a></p>");
        b.push_str("<h2>The ground</h2>");
        b.push_str(&map_figure(p, "ridge_topo", "Drawn for this guide from the national 30 m elevation model, with 20 m contours and a 1 km UTM grid."));
        b.push_str("<h2>In this guide</h2><div class=\"cards\">");
        for (n, href, title, text) in [
            ("01", "maps.html", "Maps", "Twenty-eight sheets: topographic, satellite, slope, aspect, cover, sun, snow, walking time and what is in view. View, download, print."),
            ("02", "explore.html", "Explore", "Every sheet laid over the ground in one interactive map, with your GPS position, grid references and bearings. Works offline once saved."),
            ("03", "terrain.html", "Terrain", "Where to glass from, what each spot sees, how steep, which way it faces, and how long the walking takes."),
            ("04", "access.html", "Getting there", "The drive from Grande Cache leg by leg, the climb to the ridge, staging, camping, fuel and distances."),
            ("05", "regulations.html", "Regulations", "WMU 446 seasons for 2026, the rules particular to the mountains, licences, registration and who to call."),
            ("06", "wildlife.html", "Wildlife", "Goats, sheep, caribou, grizzly, elk, moose and deer: what lives here, what is protected, and when the rut runs."),
            ("07", "weather.html", "Weather", "Live forecast, the climate of ten autumns at ridge height, first snow by year, legal light for every day to the end of November."),
            ("08", "safety.html", "Safety", "Bears and meat care, emergency numbers, the hospital, communications, and a packing list that remembers your ticks."),
        ] {
            let _ = write!(b, "<a class=\"card plain\" href=\"{href}\"><div class=\"body\"><span class=\"num\">{n}</span><h3>{title}</h3><p>{text}</p></div></a>");
        }
        b.push_str("</div>");
        b.push_str("<h2>Take it with you</h2><p>There is no signal on the ridge. Save the guide to your phone before you leave town: every page, every map and the explorer will then open with no connection.</p><div class=\"btnrow\"><button class=\"btn\" type=\"button\" data-save-offline>Save everything for offline use</button><a class=\"btn ghost\" href=\"data/cawridge.gpx\" download>GPX for your GPS</a><a class=\"btn ghost\" href=\"maps/ridge_topo.png\" download>Print map (PNG)</a></div><p class=\"small\" data-save-status>About 120 MB. Use wifi.</p>");
        b.push_str("</main>");
        emit(Page { slug: "index.html", title: "Overview", description: "A hunter's field guide to Caw Ridge, Alberta: maps, terrain analysis, regulations for WMU 446, access, wildlife, weather and safety.", body: b, head: "", scripts: "", footer: true });
    }

    // ---------------------------------------------------------------- maps
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Maps</p><h1>Twenty-eight sheets</h1><p class=\"lede\">Every map here was drawn for this guide by one program, from open data, at three scales: the region, the ridge, and the ground around the waypoint. Open a sheet to pan and zoom, download it to print, or see it over the ground in the explorer.</p></div>");
        for (group, intro) in [
            ("Topographic", "The working maps. Contours, streams, roads, the UTM grid and ground cover as a quiet tint. These are the ones to print."),
            ("Satellite", "A cloud free pass of the Sentinel-2 satellite, at 10 m to the pixel, in natural colour and in colour infrared."),
            ("Terrain", "Steepness, the way slopes face, what grows on them, and how long the walking takes."),
            ("Glassing", "What can be seen from where. Computed sight lines from the elevation model."),
            ("Sun and snow", "Where the morning sun lands first, how long each slope is lit, and when the snow usually comes."),
            ("Wildlife and land", "Provincial wildlife ranges, and the coal leases and mine roads that shape where you may go."),
        ] {
            let _ = write!(b, "<h2>{group}</h2><p>{intro}</p><div class=\"cards\">");
            for m in p.maps.iter().filter(|m| m.group == group) {
                let _ = write!(
                    b,
                    "<div class=\"card\"><a class=\"cover\" href=\"viewer.html?m={id}\"><img src=\"maps/thumb/{id}.jpg\" alt=\"{t}\" loading=\"lazy\" width=\"640\" height=\"480\"></a><div class=\"body\"><h3>{t}</h3><p>{bl}</p><div class=\"links\"><a href=\"viewer.html?m={id}\">View</a><a href=\"{dl}\" download>Download</a><a href=\"explore.html?layer={id}\">On the ground</a></div></div></div>",
                    id = m.id,
                    t = esc(&m.title),
                    bl = esc(&m.blurb),
                    dl = m.download
                );
            }
            b.push_str("</div>");
        }
        b.push_str("<h2>Files for your GPS and phone</h2><div class=\"scroll\"><table><thead><tr><th>File</th><th>What it is</th><th>Use it in</th></tr></thead><tbody>\
<tr><td><a href=\"data/cawridge.gpx\" download>cawridge.gpx</a></td><td>The waypoint, eight glassing points, turnoffs, camps and services; the drive as a track; the computed walking line.</td><td>Garmin and other GPS units, Gaia GPS, onX, Avenza, CalTopo.</td></tr>\
<tr><td><a href=\"data/ridge_topo.kmz\" download>ridge_topo.kmz</a></td><td>The ridge topographic map as a ground overlay, with the waypoints.</td><td>Google Earth, and phone apps that read KMZ overlays.</td></tr>\
<tr><td><a href=\"data/close_topo.kmz\" download>close_topo.kmz</a></td><td>The close topographic map as a ground overlay.</td><td>As above.</td></tr>\
<tr><td><a href=\"data/ridge_sat.kmz\" download>ridge_sat.kmz</a></td><td>The satellite image as a ground overlay.</td><td>As above.</td></tr>\
<tr><td><a href=\"data/close_slope.kmz\" download>close_slope.kmz</a></td><td>Slope angle around the waypoint as a ground overlay.</td><td>As above.</td></tr>\
<tr><td><a href=\"data/cawridge.kml\" download>cawridge.kml</a></td><td>Waypoints and lines only.</td><td>Google Earth, Google My Maps.</td></tr>\
<tr><td><a href=\"data/cawridge.geojson\" download>cawridge.geojson</a></td><td>Waypoints and lines as GeoJSON.</td><td>QGIS, geojson.io, your own code.</td></tr>\
<tr><td><a href=\"data/wmu.geojson\" download>wmu.geojson</a></td><td>Official wildlife management unit boundaries for the region, unchanged from the Government of Alberta.</td><td>Any GIS.</td></tr>\
</tbody></table></div>\
<div class=\"notice\"><h3>Printing</h3><p>The three topographic sheets download as PNG at full size. The ridge sheet prints well at 60 cm wide (about 1:41,000) and is still readable on letter paper. On every sheet the grid squares are 1 km, which is the quickest way to judge distance in the field.</p></div>\
<div class=\"notice law\"><h3>What these maps cannot show</h3><p>The elevation model has a 30 m cell. Cliffs shorter than that, narrow gullies and small benches are smoothed away, and 10 m contours on the close sheet are interpolated. Trails come from volunteers and may be missing or out of date. Small streams are traced from the shape of the land and may be dry. Nothing here is a substitute for the ground in front of you.</p></div></main>");
        emit(Page { slug: "maps.html", title: "Maps", description: "Topographic, satellite, slope, aspect, sun, snow and visibility maps of Caw Ridge, free to view, download and print.", body: b, head: "", scripts: "", footer: true });
    }

    // -------------------------------------------------------------- viewer
    {
        let b = "<div class=\"stage\" aria-label=\"Map viewer. Drag to move, pinch or scroll to zoom.\"><img alt=\"\"></div><div class=\"hud\"><div class=\"panel\"><b data-title>Map</b><span data-blurb></span></div><div class=\"tools\"><a class=\"tool\" href=\"maps.html\">All maps</a><a class=\"tool\" data-download href=\"#\">Download</a><button type=\"button\" data-out aria-label=\"Zoom out\">&minus;</button><button type=\"button\" data-fit>Fit</button><button type=\"button\" data-in aria-label=\"Zoom in\">+</button></div></div>".to_string();
        emit(Page { slug: "viewer.html", title: "Map viewer", description: "Pan and zoom a Caw Ridge map.", body: b, head: "", scripts: "<script src=\"assets/viewer.js\" defer></script>", footer: false });
    }

    // ------------------------------------------------------------- explore
    {
        let mut maps_obj = serde_json::Map::new();
        for m in &p.maps {
            maps_obj.insert(m.id.clone(), serde_json::json!({"bare": m.bare, "w": m.bbox.w, "s": m.bbox.s, "e": m.bbox.e, "n": m.bbox.n}));
        }
        let layer = |name: &str, ids: &[&str]| serde_json::json!({"name": name, "ids": ids});
        let layers = vec![
            layer("Topographic", &["region_topo", "ridge_topo", "close_topo"]),
            layer("Satellite", &["region_sat", "ridge_sat", "close_sat"]),
            layer("Colour infrared", &["region_sat", "ridge_cir", "close_cir"]),
            layer("Slope angle", &["region_topo", "ridge_slope", "close_slope"]),
            layer("Aspect", &["region_topo", "ridge_aspect", "close_aspect"]),
            layer("Ground cover", &["region_topo", "ridge_cover", "close_cover"]),
            layer("Glassing points", &["region_topo", "ridge_glass", "close_glass"]),
            layer("In view from the waypoint", &["region_topo", "ridge_view", "close_view"]),
            layer("Walking time", &["region_topo", "ridge_walk", "close_walk"]),
            layer("First sun", &["region_topo", "ridge_sunrise", "close_sunrise"]),
            layer("Hours of sun", &["region_topo", "ridge_sunhours", "close_sunhours"]),
            layer("First snow", &["region_topo", "ridge_snow", "close_snow"]),
            layer("Wildlife ranges", &["region_wildlife"]),
            layer("Coal leases", &["region_coal"]),
        ];
        let pl: Vec<serde_json::Value> = places.iter().map(|q| serde_json::json!({"name": q.name, "kind": q.kind, "lat": q.lat, "lon": q.lon, "elev": q.elev.round(), "note": q.note})).collect();
        let caw = serde_json::json!({
            "waypoint": [config::WPT_LAT, config::WPT_LON],
            "declination": (world.declination * 10.0).round() / 10.0,
            "maps": maps_obj,
            "layers": layers,
            "places": pl,
            "elev": {"src": "data/elev_ridge.png", "w": config::RIDGE.w, "s": config::RIDGE.s, "e": config::RIDGE.e, "n": config::RIDGE.n}
        });
        let b = "<div id=\"map\" aria-label=\"Interactive map of Caw Ridge\"></div><div class=\"readout\" aria-live=\"polite\"></div>".to_string();
        let scripts = format!("<script>window.CAW={};</script><script src=\"vendor/leaflet/leaflet.js\"></script><script src=\"assets/explore.js\"></script>", caw.to_string().replace("</", "<\\/"));
        emit(Page { slug: "explore.html", title: "Explore", description: "An interactive map of Caw Ridge with every sheet of this guide, your GPS position, grid references and bearings.", body: b, head: "<link rel=\"stylesheet\" href=\"vendor/leaflet/leaflet.css\">", scripts: &scripts, footer: false });
    }

    // ------------------------------------------------------------- terrain
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Terrain</p><h1>Reading the ridge</h1><p class=\"lede\">The elevation model is more than a picture. Run sight lines across it and it tells you where to sit; run the sun across it and it tells you which slope warms first; run a walker across it and it tells you how far a pack-out will be.</p></div>");
        let _ = write!(
            b,
            "<dl class=\"facts\">\
<div class=\"fact\"><dt>Within 3 km of the waypoint</dt><dd>{:.0} to {:.0} m<small>Lowest and highest ground.</small></dd></div>\
<div class=\"fact\"><dt>Open ground</dt><dd>{}<small>Shrub, tundra, grass and rock. The rest is timber.</small></dd></div>\
<div class=\"fact\"><dt>Forest gives out at</dt><dd>About {:.0} m<small>95 percent of forest cells lie below this height.</small></dd></div>\
<div class=\"fact\"><dt>Steeper than 30&deg;</dt><dd>{}<small>Of the ground within 3 km, by the 30 m model.</small></dd></div></dl>",
            p.stats.elev_min_3km,
            p.stats.elev_max_3km,
            pct(p.stats.cover_3km.iter().filter(|c| !c.0.contains("forest") && c.0 != "Water").map(|c| c.1).sum()),
            (p.stats.treeline / 10.0).round() * 10.0,
            pct(p.stats.slope_3km.iter().skip(3).map(|s| s.1).sum())
        );
        let _ = write!(
            b,
            "<h2>The waypoint is in a bowl</h2><p>Your waypoint lies at {} m on ground that faces {} and falls away at about {:.0}&deg;. Higher ground stands close by: {} m at {:.0} m to the {}, and more of it beyond. The sight lines say that a hunter standing at the waypoint sees {:.1} km&sup2;, which is {:.1} percent of the ground within 8 km. It is a sheltered place, out of the worst of the wind and off the skyline. It is a poor place to glass from.</p>",
            chart::thousands(world.wpt_elev),
            geo::compass_words(p.stats.wpt_aspect as f64),
            p.stats.wpt_slope,
            chart::thousands(p.stats.rim.0),
            (p.stats.rim.2 / 10.0).round() * 10.0,
            geo::compass_words(p.stats.rim.1),
            p.stats.seen_km2_wpt,
            p.stats.seen_share_wpt * 100.0
        );
        b.push_str(&map_figure(p, "close_view", "Yellow is in view from the waypoint, dark is hidden. Yellow diamonds are the glassing points."));
        b.push_str("<h2>Where to glass from</h2><p>The program tried a candidate every 180 m across the close sheet, let each one shuffle to the highest ground nearby, and counted the open ground it could see between 300 m and 3 km with an eye 1.7 m up and an animal's back 1.0 m up. Timber was left out of the count: you cannot glass into it. The eight best, kept at least 700 m apart:</p>");
        b.push_str("<div class=\"scroll\"><table><thead><tr><th>Point</th><th class=\"n\">Open ground in view</th><th class=\"n\">Share of open ground</th><th class=\"n\">Height</th><th class=\"n\">From waypoint</th><th>Bearing</th><th>Latitude, longitude</th><th>UTM 11U</th></tr></thead><tbody>");
        for v in &p.vantages {
            let u = geo::Utm::new(11).forward(v.lon, v.lat);
            let _ = write!(
                b,
                "<tr><td><b>{}</b></td><td class=\"n\">{:.1} km&sup2;</td><td class=\"n\">{}</td><td class=\"n\">{} m</td><td class=\"n\">{:.1} km</td><td>{:.0}&deg; {}</td><td class=\"mono\">{:.5}, {:.5}</td><td class=\"mono\">{:06.0} E {:.0} N</td></tr>",
                v.name,
                v.open_km2,
                pct(v.share),
                chart::thousands(v.elev),
                v.dist_m / 1000.0,
                v.bearing,
                geo::compass(v.bearing),
                v.lat,
                v.lon,
                u.0,
                u.1
            );
        }
        b.push_str("</tbody></table></div>");
        let bars: Vec<Bar> = p.vantages.iter().map(|v| Bar { label: format!("{}  ({:.1} km, {})", v.name, v.dist_m / 1000.0, geo::compass(v.bearing)), value: v.open_km2, text: format!("{:.1} km\u{b2}", v.open_km2), tip: format!("{}|{:.1} km\u{b2} of open ground in view|{} of the open ground within 3 km", v.name, v.open_km2, pct(v.share)) }).collect();
        let _ = write!(b, "<div class=\"viz\"><h3>Open ground in view</h3><p class=\"sub\">Square kilometres of open ground seen between 300 m and 3 km, by glassing point</p>{}</div>", chart::bars("Open ground in view by glassing point", &bars));
        let _ = write!(
            b,
            "<div class=\"notice\"><h3>How to use this</h3><p><b>{}</b> is {:.1} km from the waypoint at {:.0}&deg; true ({:.0}&deg; on the compass), on a {} m top. Bearings are true; subtract {:.0}&deg; for a magnetic compass. Sight lines are over bare ground: a single krummholz clump can take a view away, and the model cannot see a bench under a small cliff. Walk the last hundred metres with your eyes.</p><p>The dark ground on the glassing map matters as much as the bright. It is dead ground that no point sees into: where animals bed out of sight, and where you can move without being seen.</p></div>",
            g1.name,
            g1.dist_m / 1000.0,
            g1.bearing,
            (g1.bearing - world.declination + 360.0) % 360.0,
            chart::thousands(g1.elev),
            world.declination
        );
        b.push_str(&map_figure(p, "close_glass", "The glassing points and how many of them see each piece of ground."));

        b.push_str("<h2>Steepness</h2><div class=\"cols\"><div>");
        let bars: Vec<Bar> = p.stats.slope_3km.iter().map(|s| Bar { label: s.0.clone(), value: s.1, text: pct(s.1), tip: format!("{}|{} of the ground within 3 km", s.0, pct(s.1)) }).collect();
        let _ = write!(b, "<div class=\"viz\"><h3>Slope within 3 km</h3><p class=\"sub\">Share of ground by slope angle</p>{}</div>", chart::bars("Share of ground by slope angle", &bars));
        b.push_str("</div><div><p>Caw Ridge is rolling country with short cliffs and rockslides, not a wall. Most of the ground within 3 km is between 10 and 30 degrees: steady to steep walking. Less than a tenth is over 30 degrees, and almost none reads over 40.</p><p>That last number deserves suspicion. A 30 m cell averages a 15 m cliff into a moderate slope. The goats know where the cliffs are; the model only knows where they are likely. Read every colour on the slope map as a minimum.</p></div></div>");
        b.push_str(&map_figure(p, "close_slope", "Green is steady walking, yellow and orange are steep, red and purple are escape terrain."));

        b.push_str("<h2>Ground cover</h2><div class=\"cols\"><div>");
        let bars: Vec<Bar> = p.stats.cover_3km.iter().filter(|c| c.1 >= 0.005).map(|c| Bar { label: c.0.clone(), value: c.1, text: pct(c.1), tip: format!("{}|{} of the ground within 3 km", c.0, pct(c.1)) }).collect();
        let _ = write!(b, "<div class=\"viz\"><h3>Cover within 3 km</h3><p class=\"sub\">Share of ground by cover class, 2020 Land Cover of Canada</p>{}</div>", chart::bars("Share of ground by cover class", &bars));
        let _ = write!(
            b,
            "</div><div><p>About half the ground within 3 km of the waypoint is conifer forest; the other half is open. Timber climbs to roughly {:.0} m, thins to shrub and krummholz, and gives way to tundra, grass and rock on the ridge tops.</p><p>The edge between timber and open ground is the line to watch at first and last light. On the cover map it is the boundary between dark green and everything else.</p></div></div>",
            (p.stats.treeline / 10.0).round() * 10.0
        );
        b.push_str(&map_figure(p, "ridge_cover", "Timber, shrub, tundra and rock across the whole ridge."));

        let _ = write!(
            b,
            "<h2>Sun and thermals</h2><p>On {} {} the sun rises at {} and sets at {} on a level horizon. At the waypoint, with the surrounding ridges counted, direct sun arrives about {} and is gone about {}: {:.1} hours of it.</p><p>The pattern that follows is old knowledge. Slopes that take the first sun warm first, and air begins to rise on them; shaded slopes keep draining cold air downhill until the sun finds them. In the evening the order reverses. Plan an approach so that the air carries your scent away from where you expect animals to be, and remember that ridge top wind will often overrule the thermals altogether.</p>",
            config::SUN_DATE.2,
            sun::month_name(config::SUN_DATE.1),
            sun_rows(sun::days_from_civil(config::SUN_DATE.0, config::SUN_DATE.1, config::SUN_DATE.2), sun::days_from_civil(config::SUN_DATE.0, config::SUN_DATE.1, config::SUN_DATE.2)).first().map(|r| sun::hm(r.rise)).unwrap_or_default(),
            sun_rows(sun::days_from_civil(config::SUN_DATE.0, config::SUN_DATE.1, config::SUN_DATE.2), sun::days_from_civil(config::SUN_DATE.0, config::SUN_DATE.1, config::SUN_DATE.2)).first().map(|r| sun::hm(r.set)).unwrap_or_default(),
            p.stats.sunrise_terrain.map(|h| sun::hm(h as f64)).unwrap_or_else(|| "never".into()),
            p.stats.sunset_terrain.map(|h| sun::hm(h as f64)).unwrap_or_else(|| "never".into()),
            p.stats.sun_hours_wpt
        );
        b.push_str("<div class=\"cols\">");
        b.push_str(&map_figure(p, "close_sunrise", "When direct sun first reaches each slope."));
        b.push_str(&map_figure(p, "close_aspect", "Which way each slope faces."));
        b.push_str("</div>");

        let _ = write!(
            b,
            "<h2>Walking</h2><p>Walking times come from Tobler's hiking function, a rule of thumb fitted to real walkers: about 5 km/h on the level, fastest on a gentle downhill, and slower the steeper it gets in either direction. On roads and tracks the full speed is used; off them, three fifths of it, which is Tobler's own figure for open ground without a path.</p><p>From the end of the mapped track to the waypoint is {:.1} km and about {:.0} minutes; coming back, about {:.0} minutes. The model knows nothing of a loaded pack, snow, deadfall, willow, or a creek in flood. Add half again for a real day, and double it with a quarter on your back.</p>",
            p.walk_profile.pts.last().map(|q| q.0).unwrap_or(0.0),
            p.walk_out_h * 60.0,
            p.walk_back_h * 60.0
        );
        b.push_str(&map_figure(p, "ridge_walk", "Time to walk from the waypoint to anywhere on the sheet."));
        b.push_str("</main>");
        emit(Page { slug: "terrain.html", title: "Terrain", description: "Terrain analysis of Caw Ridge: glassing points, visibility, slope, aspect, ground cover, sun exposure and walking times.", body: b, head: "", scripts: "", footer: true });
    }

    // -------------------------------------------------------------- access
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Getting there</p><h1>The road to the ridge</h1>");
        let _ = write!(
            b,
            "<p class=\"lede\">From Grande Cache it is {:.0} km to the top: {:.0} km of pavement, {:.0} km of gravel through coal mine country, and {:.0} km of old exploration road that climbs {} m to the alpine.</p></div>",
            km_total,
            km_hwy,
            km_gravel,
            km_track,
            chart::thousands(p.drive.iter().find(|l| l.kind == "track").map(|l| { let pr = crate::route::profile(&world.dem, &l.pts, 30.0, 200.0); pr.pts.last().map(|q| q.1).unwrap_or(0.0) - pr.pts.first().map(|q| q.1).unwrap_or(0.0) }).unwrap_or(0.0))
        );
        b.push_str("<div class=\"scroll\"><table><thead><tr><th>Leg</th><th>Road</th><th>Surface</th><th class=\"n\">Distance</th><th class=\"n\">Running total</th><th>Notes</th></tr></thead><tbody>");
        let mut run = 0.0;
        let mut marks: Vec<Mark> = Vec::new();
        let mut n = 0;
        for l in &p.drive {
            n += 1;
            let (road, surface, note) = match l.kind.as_str() {
                "town" => ("Through Grande Cache", "Paved", "From the middle of town out to Highway 40. Fuel here: there is none beyond."),
                "highway" => ("Highway 40 north (Bighorn Highway)", "Paved", "Leave Grande Cache northbound. Cross the Smoky River. Watch for the Beaverdam Road sign on the left."),
                "gravel" => ("Beaverdam Road", "Gravel", "Crosses coal mine property and the main mine haul road. Stay on Beaverdam Road. Riders call it rough for a pickup and fine for small trailers, and speak of kilometre markers along it."),
                _ => ("Caw Ridge road", "Rough track", "An old mining exploration road: eroded, with long cobble sections. Treat it as an off-highway vehicle, horse or foot route, not a truck road."),
            };
            if l.kind != "town" {
                marks.push(Mark { x: run, label: match l.kind.as_str() { "highway" => "Highway 40".into(), "gravel" => "Beaverdam Road".into(), _ => "Track".into() } });
            }
            run += l.km;
            let _ = write!(b, "<tr><td class=\"n\">{n}</td><td><b>{road}</b></td><td>{surface}</td><td class=\"n\">{:.1} km</td><td class=\"n\">{:.1} km</td><td>{note}</td></tr>", l.km, run);
        }
        let _ = write!(
            b,
            "<tr><td class=\"n\">{}</td><td><b>On foot to the waypoint</b></td><td>Open alpine</td><td class=\"n\">{:.1} km</td><td class=\"n\">{:.1} km</td><td>No trail. About {:.0} minutes by the walking model.</td></tr></tbody></table></div>",
            n + 1,
            p.walk_profile.pts.last().map(|q| q.0).unwrap_or(0.0),
            run + p.walk_profile.pts.last().map(|q| q.0).unwrap_or(0.0),
            p.walk_out_h * 60.0
        );
        let _ = write!(
            b,
            "<div class=\"viz\"><h3>The drive in profile</h3><p class=\"sub\">Height above sea level along the route from Grande Cache, metres. Low point {} m, high point {} m.</p>{}{}</div>",
            chart::thousands(p.drive_profile.min),
            chart::thousands(p.drive_profile.max),
            chart::area("Elevation along the drive from Grande Cache to the ridge", &p.drive_profile.pts, "km from Grande Cache", "m", &marks),
            chart::table(
                "Show the numbers",
                &["Distance", "Height"],
                &p.drive_profile.pts.iter().step_by((p.drive_profile.pts.len() / 48).max(1)).map(|q| vec![format!("{:.1} km", q.0), format!("{} m", chart::thousands(q.1))]).collect::<Vec<_>>()
            )
        );
        b.push_str("<p>The route above was found by the program in OpenStreetMap, preferring better roads. It matches the directions published by the Municipal District of Greenview almost to the hundred metres: 8.1 km north on Highway 40, 30 km along Beaverdam Road to the Caw Ridge turnoff on the left, then 8.1 km up the old road to the top.</p>");
        b.push_str(&map_figure(p, "region_topo", "The whole approach on one sheet."));
        b.push_str("<h2>What riders report</h2><p>None of what follows is official, and the newest of it is from 2023. It comes from public forum threads by people who have made the trip on quads and side-by-sides.</p><ul>\
<li>Beaverdam Road is rough in a pickup and better suited to small trailers than large ones.</li>\
<li>The highway to the ridge top is just shy of 50 km.</li>\
<li>Staging areas are reported at the mine flats by the Smoky River off Highway 40, at Gustavs Flats, and at the Caw Ridge turnoff itself.</li>\
<li>Old exploration roads run along most of the ridge line once you are up. A loop is described that descends to the south past an open pit and meets Beaverdam Road about 8 km east of the turnoff.</li>\
<li>Sheep Creek Road, further north, is a better kept oilfield road but was reported gated to highway vehicles at kilometre 13, and signed against off-highway vehicles.</li>\
<li>Snow closed the top to riders in late May 2008 and again in June 2013. In autumn the same drifts arrive early.</li>\
<li>Fresh grizzly tracks are reported regularly along the access.</li></ul>\
<div class=\"notice law\"><h3>Radio controlled roads</h3><p>Resource roads in this country carry loaded coal and log trucks. No radio channel is published for Beaverdam Road, and one forum post holds that radios are required of commercial traffic only. Drive with your lights on, keep right on blind corners, give trucks the road, and ask at the Grande Cache Tourism and Interpretive Centre (780-827-3300) about current conditions before you go.</p></div>\
<div class=\"notice stop\"><h3>The off-highway vehicle weapons rule</h3><p>In WMUs 400 to 446 it is unlawful to carry a weapon on an off-highway vehicle from one hour before sunrise until noon during an open big game season. The exception is direct travel to an isolated campsite with the weapon and ammunition out of view in separate locked containers. If you ride the last 8 km to hunt the morning, you ride without your rifle or you wait until noon. Many hunters camp high or walk.</p></div>");
        let _ = write!(
            b,
            "<h2>Distances</h2><div class=\"scroll\"><table><thead><tr><th>From</th><th class=\"n\">To Grande Cache</th><th>Road</th></tr></thead><tbody>\
<tr><td>Hinton (Highway 16)</td><td class=\"n\">about 140 km</td><td>Highway 40 north, paved</td></tr>\
<tr><td>Grande Prairie</td><td class=\"n\">about 190 km</td><td>Highway 40 south, paved. About two hours.</td></tr>\
<tr><td>Edmonton</td><td class=\"n\">about 430 km</td><td>Highway 16 west to Hinton, then Highway 40. About four and three quarter hours.</td></tr>\
<tr><td>Grande Cache to the waypoint</td><td class=\"n\">{:.0} km</td><td>As the raven flies, bearing {:.0}&deg; true ({}).</td></tr></tbody></table></div>",
            town_km,
            town_brg,
            geo::compass(town_brg)
        );
        b.push_str("<h2>Camping</h2><p>Random camping is allowed on public land for up to 14 days in one place: keep 30 m back from water, and stay at least 1 km from a provincial park or recreation area. A <a href=\"https://www.alberta.ca/public-lands-camping-pass\">Public Lands Camping Pass</a> is required along the eastern slopes ($20 for three days or $30 for the year, per person); the official map appears to take in the land west of Highway 40 here, so buy one.</p><div class=\"scroll\"><table><thead><tr><th>Campground</th><th>Where</th><th>Notes</th><th>Coordinates</th></tr></thead><tbody>\
<tr><td><b>Sheep Creek</b> provincial recreation area</td><td>Highway 40, 25 km north of Grande Cache</td><td>Closest developed site to the Beaverdam Road country. Boat launch.</td><td class=\"mono\">54.06217, -119.01122</td></tr>\
<tr><td><b>Smoky River South</b> provincial recreation area</td><td>Highway 40 at the Smoky River bridge</td><td>22 unserviced sites. Usually open to Thanksgiving.</td><td class=\"mono\">53.89036, -119.15852</td></tr>\
<tr><td><b>Sulphur Gates</b> provincial recreation area</td><td>South-west of town, Willmore staging</td><td>11 sites, open all year, road not kept in winter. Firearm discharge permit rules apply.</td><td class=\"mono\">53.85013, -119.18618</td></tr>\
<tr><td><b>Grande Cache municipal campground</b></td><td>In town</td><td>Check that it is still open this late in the year.</td><td class=\"mono\">in town</td></tr>\
<tr><td><b>Pierre Grey's Lakes</b> provincial park</td><td>Highway 40, 37 km south</td><td>On the way in from Hinton.</td><td class=\"mono\">53.90438, -118.59179</td></tr></tbody></table></div>");
        b.push_str("<h2>Last services</h2><p>Grande Cache has fuel (Fas Gas and Petro-Canada), a supermarket, hotels and motels, a hospital with a 24 hour emergency department, and an RCMP detachment. There is nothing past the Beaverdam Road turnoff: no fuel, no water you have not treated, no help you have not arranged. Fill the tank and the jerry cans in town.</p>");
        b.push_str("<h2>Whose land</h2><p>The ridge is provincial Crown land in the Green Area, within the Municipal District of Greenview and Treaty 8 territory. It is not a park. Two protected areas lie close: Willmore Wilderness Park about 8 km to the south-west, where hunting is allowed but no motor vehicle may go, and Kakwa Wildland Provincial Park about 12 km to the west.</p><p>The Aseniwuche Winewak Nation and the Mountain M&eacute;tis have lived in this country for generations. The Nation's cooperatives and enterprises (Victor Lake, Susa Creek, Muskeg Seepee, Wanyandie Flats, Grande Cache Lake and Joachim) hold parcels along Highway 40. Treat them as private occupied land and ask before entering. Treaty and M&eacute;tis harvesters may lawfully be hunting at times and under rules that differ from yours.</p>\
<div class=\"notice\"><h3>Change is coming</h3><p>The Upper Smoky Sub-regional Plan, in force since November 2025, names Caw Ridge as part of a conservation area to be designated under the Provincial Parks Act. The plan says hunting will continue under existing law and that motorized recreation will be allowed in areas shown on a map the province will publish. As of the research for this guide the designation did not yet appear in the province's protected area data. Call 310-LAND (310-5263) for the present state of things.</p></div>");
        let lease = match &p.stats.coal_lease {
            Some(l) => format!("Coal leases and mine roads. The waypoint lies inside Crown coal lease {}, held by {} and running to {}. A lease is a right to the mineral and does not by itself close the surface.", l.0, title_case(&l.1), l.2.replace('/', "-")),
            None => "Coal leases and mine roads.".to_string(),
        };
        b.push_str(&map_figure(p, "region_coal", &lease));
        b.push_str("</main>");
        emit(Page { slug: "access.html", title: "Getting there", description: "Driving and walking access to Caw Ridge from Grande Cache: route legs, elevation profile, road reports, camping, services and land status.", body: b, head: "", scripts: "", footer: true });
    }

    // --------------------------------------------------------- regulations
    {
        let wmu_edge = world
            .wmu
            .iter()
            .filter(|f| f.get("WMUNIT_CODE").ends_with("446"))
            .filter_map(|f| if let crate::vector::Geom::Poly(r) = &f.geom { Some(r.iter().map(|ring| crate::vector::nearest_on(ring, config::WPT_LON, config::WPT_LAT)).fold((f64::MAX, 0.0, (0.0, 0.0)), |a, c| if c.0 < a.0 { c } else { a })) } else { None })
            .fold((f64::MAX, 0.0, (0.0, 0.0)), |a, c| if c.0 < a.0 { c } else { a });
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Regulations</p><h1>WMU 446, Kakwa River</h1><p class=\"lede\">What the 2026 Alberta Guide to Hunting Regulations and the Wildlife Regulation say about hunting at your waypoint, gathered on 28 September 2026. Read it as a briefing, then read the guide itself.</p></div>");
        b.push_str("<div class=\"notice stop\"><h3>This page is not the law</h3><p>Seasons, quotas and rules change every year and sometimes mid-season. The printed guide is itself only a summary of the Wildlife Act and the Wildlife Regulation. Before you hunt, check your species and your WMU in the <a href=\"https://albertaregulations.ca/huntingregs/\">official guide</a>, and carry your licences and tags on paper.</p></div>");
        let _ = write!(
            b,
            "<h2>Which unit you are in</h2><p>The waypoint lies in <b>Wildlife Management Unit 446, Kakwa River</b>. That was checked two ways: by a query against the Government of Alberta's own boundary service, and again by this guide's program, which tested the point against the official polygon. The nearest boundary is with WMU 442 (Sheep Creek), {:.1} km away at {:.0}&deg; true, along Grizzly and Copton creeks. WMU 444 (Mount Hamell) begins across Sheep Creek, about 11 km to the south-east.</p><p>The legal unit is the written description in the regulation, not any map: Smoky River, Sheep Creek, Horn Creek, the height of land to Copton Creek, Kakwa River, Prairie Creek and the 16th baseline. If you hunt near an edge, read the <a href=\"https://albertaregulations.ca/huntingregs/wmu/446.html\">description for 446</a>.</p>",
            wmu_edge.0 / 1000.0,
            wmu_edge.1
        );
        b.push_str(&map_figure(p, "region_topo", "Wildlife management units in purple, parks in green."));
        b.push_str("<h2>2026 seasons in WMU 446</h2><div class=\"scroll\"><table><thead><tr><th>Species</th><th>What may be taken</th><th>Archery only</th><th>General season</th><th>Licence</th></tr></thead><tbody>\
<tr><td><b>White-tailed deer</b></td><td>Antlered and antlerless</td><td>25 Aug to 16 Sep</td><td>17 Sep to 30 Nov</td><td><span class=\"tag open\">General</span> Supplemental antlerless licence valid in 440 to 446</td></tr>\
<tr><td><b>Mule deer</b></td><td>Antlered only</td><td>25 Aug to 16 Sep</td><td>17 Sep to 30 Nov</td><td><span class=\"tag open\">General</span> No antlerless season listed</td></tr>\
<tr><td><b>Elk</b></td><td>Antlered, six point or larger</td><td>25 Aug to 16 Sep</td><td>17 Sep to 30 Nov</td><td><span class=\"tag open\">General</span> No antlerless season in 446</td></tr>\
<tr><td><b>Moose</b></td><td>Antlered only</td><td>25 Aug to 23 Sep</td><td>24 Sep to 30 Nov</td><td><span class=\"tag draw\">Draw</span> Rifle season by special licence, residents only. Archery season on the archery moose licence.</td></tr>\
<tr><td><b>Bighorn sheep, trophy</b></td><td>Four fifths curl ram</td><td>None</td><td>25 Aug to 31 Oct</td><td><span class=\"tag draw\">Draw</span> Draw code 37, WMU 444 and 446, residents only</td></tr>\
<tr><td><b>Bighorn sheep, non-trophy</b></td><td>As the licence states</td><td>None</td><td>10 Sep to 31 Oct</td><td><span class=\"tag draw\">Draw</span></td></tr>\
<tr><td><b>Mountain goat</b></td><td colspan=\"3\">No season in WMU 446</td><td><span class=\"tag closed\">Closed</span></td></tr>\
<tr><td><b>Caribou</b></td><td colspan=\"3\">No season anywhere in Alberta. Threatened species.</td><td><span class=\"tag closed\">Closed</span></td></tr>\
<tr><td><b>Grizzly bear</b></td><td colspan=\"3\">No season anywhere in Alberta.</td><td><span class=\"tag closed\">Closed</span></td></tr>\
<tr><td><b>Black bear</b></td><td>Not a cub, not a sow with a cub</td><td>25 Aug to 31 Aug</td><td>1 Sep to 30 Nov</td><td><span class=\"tag open\">General</span> No baiting in 446. Second bear licence not valid here.</td></tr>\
<tr><td><b>Cougar</b></td><td>Either sex</td><td>None</td><td>25 Aug to 30 Nov</td><td><span class=\"tag open\">General</span> Residents only, no dogs in the fall season</td></tr>\
<tr><td><b>Wolf</b></td><td></td><td></td><td>From the opening of any big game season to 31 May 2027</td><td>Residents need no licence</td></tr>\
<tr><td><b>Ruffed, spruce and dusky grouse; ptarmigan</b></td><td>Daily 5, possession 15 of each</td><td></td><td>1 Sep to 15 Jan</td><td><span class=\"tag open\">Game bird licence</span></td></tr>\
</tbody></table></div>\
<p class=\"small\">Source: 2026 Alberta Guide to Hunting Regulations, big game season tables, and the 2026 draws booklet, cross-checked against Schedule 15 of the Wildlife Regulation as consolidated to 17 September 2026. Antlerless moose and antlerless mule deer do not appear in the 2026 tables for 446 and are read here as closed.</p>");
        b.push_str("<h3>What the words mean</h3><ul>\
<li><b>Six point elk:</b> a bull with at least one antler whose main beam carries five or more tines, each at least 7.6 cm (3 inches) long. Count twice. Brow tines count.</li>\
<li><b>Four fifths curl ram:</b> seen from the side, a straight line from the front of the horn base to the horn tip passes in front of the front edge of the eye. Full curl is a different rule and applies only in WMU 400 and 302.</li>\
<li><b>Archery only:</b> bows only. A crossbow is not a bow for this purpose and cannot be used in an archery only season.</li></ul>");
        b.push_str("<h2>Rules particular to the mountains</h2><p>These apply in WMUs 400 to 446, which includes yours.</p><ul>\
<li><b>Off-highway vehicles and weapons.</b> Unlawful to carry a weapon on an off-highway vehicle from one hour before sunrise until noon during an open big game season. It applies to bird hunters too. The exception is direct travel to an isolated campsite with weapons and ammunition out of view in separate locked containers.</li>\
<li><b>Aircraft.</b> No helicopter may carry big game hunters or their game. No hunting big game within six hours of stepping out of an aircraft.</li>\
<li><b>Pack animals.</b> No pack goats and no domestic sheep, to keep disease from wild sheep and goats. Pack dogs must be leashed or within 50 m.</li>\
<li><b>Marked animals.</b> Anyone who kills or finds an animal wearing a collar, tag or other device must report it and return the device. Some marked animals carry a tag warning not to eat the meat before speaking with Fish and Wildlife, because of the drugs used to handle them. On Caw Ridge, marked animals are common.</li></ul>");
        b.push_str("<h2>Rules that apply everywhere</h2><ul>\
<li><b>Legal hours.</b> You may hunt from half an hour before sunrise to half an hour after sunset. The <a href=\"weather.html#light\">light table</a> gives the times for every day to the end of November.</li>\
<li><b>Roads.</b> No shooting from, along or across a provincial highway or any road that is paved, oiled, graded or regularly maintained. No loaded firearm in or on a vehicle.</li>\
<li><b>Buildings and occupied land.</b> No hunting or shooting within 183 m (200 yards) of an occupied building, or on occupied land without consent. A working mine site is occupied land.</li>\
<li><b>Ammunition.</b> For big game: no rimfire, nothing under .22 calibre centrefire, no non-expanding bullets. Semi-automatics are limited to five rounds in the magazine.</li>\
<li><b>Hunter orange</b> is not required in Alberta. On a ridge shared with other hunters and riders it is still a good idea.</li>\
<li><b>Sunday hunting</b> is allowed in 446.</li>\
<li><b>Waste.</b> It is unlawful to let the edible meat of game go to waste. The guide sets out what must be packed out, and the exceptions.</li></ul>");
        b.push_str("<h2>Licences</h2><div class=\"scroll\"><table><thead><tr><th>You need</th><th class=\"n\">Resident</th><th>Notes</th></tr></thead><tbody>\
<tr><td>Wildlife Identification Number (WiN)</td><td class=\"n\">$10</td><td>Once, then renewed. Everyone needs one.</td></tr>\
<tr><td>Wildlife certificate</td><td class=\"n\">$35</td><td>Each year. $12 for resident youth and seniors.</td></tr>\
<tr><td>Deer licence</td><td class=\"n\">$50</td><td>White-tailed or mule deer, each its own licence.</td></tr>\
<tr><td>Elk licence</td><td class=\"n\">$50</td><td></td></tr>\
<tr><td>Antlered moose special licence</td><td class=\"n\">$65</td><td>By draw.</td></tr>\
<tr><td>Trophy sheep special licence</td><td class=\"n\">$95</td><td>By draw.</td></tr>\
<tr><td>Black bear licence</td><td class=\"n\">$30</td><td></td></tr>\
<tr><td>Cougar licence</td><td class=\"n\">$30</td><td></td></tr>\
<tr><td>Game bird licence</td><td class=\"n\">$20</td><td></td></tr>\
<tr><td>Bowhunting permit</td><td class=\"n\">$10</td><td>To hunt with a bow in any season.</td></tr></tbody></table></div>\
<p>Prices are before GST, from the 2026 guide. Tags must be carried on paper; they cannot be shown on a phone. Buy at <a href=\"https://www.albertarelm.com/\">AlbertaRELM</a> or a licence issuer in town.</p>\
<p><b>Not an Alberta resident?</b> To hunt big game, wolf or coyote you must be with an outfitter's guide or a licensed hunter host. Game birds need no guide.</p>");
        b.push_str("<h2>After the shot</h2><ul>\
<li><b>Tag at once.</b> Put the tag on the animal as the licence instructs before you move it.</li>\
<li><b>Evidence of sex and species</b> must stay attached as the guide sets out until the animal is home or at a butcher.</li>\
<li><b>Registration.</b> A male bighorn over one year old must be registered in person, by appointment, within 14 days of the season closing or 30 days of the kill, whichever comes first. The nearest offices that register sheep are Grande Prairie and Edson. A cougar must be registered within five business days.</li>\
<li><b>Chronic wasting disease.</b> WMU 446 is not on the 2026 list of units where deer heads must be submitted. Voluntary submission is always welcome.</li></ul>");
        b.push_str("<h2>Who to call</h2><div class=\"phones\">\
<div class=\"phone\"><div class=\"who\">Report A Poacher</div><a class=\"num\" href=\"tel:18006423800\">1-800-642-3800</a><p>Poaching, and dangerous wildlife. 24 hours.</p></div>\
<div class=\"phone\"><div class=\"who\">Fish and Wildlife, Grande Prairie</div><a class=\"num\" href=\"tel:17805385260\">780-538-5260</a><p>10925 84 Avenue. The office for this district. There is no Fish and Wildlife office in Grande Cache.</p></div>\
<div class=\"phone\"><div class=\"who\">Fish and Wildlife Enforcement</div><a class=\"num\" href=\"tel:17805385265\">780-538-5265</a><p>Enforcement hub covering Grande Cache.</p></div>\
<div class=\"phone\"><div class=\"who\">Public lands</div><a class=\"num\" href=\"tel:3105263\">310-LAND</a><p>310-5263. Access, camping, land use zones.</p></div>\
<div class=\"phone\"><div class=\"who\">Cougar quota line</div><a class=\"num\" href=\"tel:18006613729\">1-800-661-3729</a><p>Winter cougar season: call each day before hunting.</p></div>\
<div class=\"phone\"><div class=\"who\">Government of Alberta</div><a class=\"num\" href=\"tel:3100000\">310-0000</a><p>Toll free connection to any provincial office.</p></div></div>");
        b.push_str("<h2>Official sources</h2><ul>\
<li><a href=\"https://albertaregulations.ca/huntingregs/\">Alberta Guide to Hunting Regulations</a>, and the <a href=\"https://www.albertaregulations.ca/2026-Alberta-Hunting-Regulations.pdf\">2026 guide as a PDF</a></li>\
<li><a href=\"https://albertaregulations.ca/pdfs/hunting-regs/Big-Game-Seasons.pdf\">Big game season tables</a> and the <a href=\"https://www.albertaregulations.ca/2026-Alberta-Hunting-Draws.pdf\">2026 draws booklet</a></li>\
<li><a href=\"https://albertaregulations.ca/huntingregs/wmu/446.html\">WMU 446 boundary description</a> and <a href=\"https://www.alberta.ca/wildlife-management-units\">wildlife management unit maps</a></li>\
<li><a href=\"https://kings-printer.alberta.ca/documents/Regs/1997_143.pdf\">Wildlife Regulation, Alta Reg 143/97</a></li>\
<li><a href=\"https://open.alberta.ca/publications/upper-smoky-sub-regional-plan\">Upper Smoky Sub-regional Plan</a></li>\
<li><a href=\"https://www.alberta.ca/bears-and-hunters\">Bears and hunters</a> and <a href=\"https://www.alberta.ca/know-your-bears\">Know your bears</a></li></ul></main>");
        emit(Page { slug: "regulations.html", title: "Regulations", description: "2026 hunting seasons and rules for Alberta Wildlife Management Unit 446, Kakwa River, which contains Caw Ridge.", body: b, head: "", scripts: "", footer: true });
    }

    // ------------------------------------------------------------ wildlife
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Wildlife</p><h1>What lives on the ridge</h1><p class=\"lede\">Caw Ridge carries nearly the whole cast of the northern Rockies on one small piece of alpine: goats, sheep, caribou, grizzlies, wolves, and the elk, moose and deer of the timber below. Some you may hunt. Several you may not.</p></div>");
        b.push_str(&map_figure(p, "region_wildlife", "Provincial goat and sheep range in purple hatching, caribou range in orange."));
        b.push_str("<h2>Mountain goat <span class=\"tag closed\">Closed in 446</span></h2><p>The waypoint lies inside the range the province maps for mountain goat and bighorn sheep. The goats of Caw Ridge are the subject of the longest study of the species anywhere. Biologists from the Universit&eacute; de Sherbrooke and Universit&eacute; Laval, working with Alberta Fish and Wildlife, have followed them since 1989, marking kids and recording who lives, who breeds and who dies. By 2009 they had marked 427 animals. The herd has numbered between about 76 and 160, and was reported in decline through the 2010s.</p><ul>\
<li>They have not been hunted since 1969. There is no goat season in WMU 446.</li>\
<li>They use the alpine between about 1,750 and 2,170 m, never far from steep ground to escape to.</li>\
<li>The study found that goats are easily disturbed. A quad approaching fast is likely to put them on alert for ten minutes or send them running more than 100 m, and helicopters within 500 m moved them most of the time. Keep your distance, slow down, and do not approach for a photograph.</li>\
<li>The rut is in mid to late November. Billies travel then.</li>\
<li>Adult nannies survive at about 89 percent a year, billies 83, kids 63. A nanny first breeds at almost five years old. It is a slow herd to rebuild, which is why it is protected.</li></ul>");
        b.push_str("<h2>Bighorn sheep <span class=\"tag draw\">Draw only</span></h2><p>About 250 bighorns were counted on and around the ridge in the study's time, mostly on its eastern part and the adjacent lands. Both sheep seasons in 446 are by draw. If you hold the tag, the ram must make four fifths curl; judge it from the side, at rest, through a spotting scope, and if you are unsure, he is not legal. The sheep rut runs from late November into December, after the season has closed.</p>");
        b.push_str("<h2>Woodland caribou <span class=\"tag closed\">Closed</span></h2><p>The ridge lies on a migration route of the Redrock-Prairie Creek herd, which moves between summer range in the mountains and winter range in the foothills forest. Counts have fallen for decades: about 96 animals were estimated in that herd in 2017. Woodland caribou are a threatened species and there is no season.</p><div class=\"notice stop\"><h3>Caribou or elk</h3><p>In timber and poor light a caribou can be taken for a cow or young bull elk. Caribou are smaller and paler, with a white neck and mane, a white rump without the elk's tan patch edged in dark, and large splayed hooves. Both sexes carry antlers, and a caribou's are flattened and sweep forward with a brow shovel over the face. If it is not plainly an elk with six points, do not shoot.</p></div>");
        b.push_str("<h2>Grizzly bear <span class=\"tag closed\">Closed</span></h2><p>You are in Bear Management Area 2, Grande Cache, and the province's own map puts the waypoint inside the core recovery zone. A 2008 survey put the density here at about 18 bears per thousand square kilometres, the highest in the province outside the national parks. Riders report fresh tracks on the access roads as a matter of course. There is no grizzly season in Alberta.</p><div class=\"notice stop\"><h3>Black bear or grizzly</h3><p>Colour tells you nothing: black bears here are often brown or cinnamon, and grizzlies can be nearly black. Look for the shoulder hump and the dished face of the grizzly, its short round ears and its long pale claws. A black bear has no hump, a straight profile from forehead to nose, and taller ears. Size is a poor guide. If you cannot see the shoulder line and the face, you have not identified the bear.</p></div>");
        b.push_str("<h2>Elk <span class=\"tag open\">General, six point</span></h2><p>Elk use the timber, the cutblocks and the valley meadows below the ridge, and climb to the subalpine edge in fine weather. The rut in this country runs through September and tails off by mid October, so the early rifle season catches the end of the bugling. After that the bulls go quiet, break into bachelor groups and feed hard. Look to the forest edge, south-facing openings and regenerating cuts at first and last light.</p>");
        b.push_str("<h2>Moose <span class=\"tag draw\">Draw for rifle</span></h2><p>Moose keep to the wet ground: willow flats, beaver ponds and the creek bottoms of Beaverdam, Caw and Copton creeks. The rut peaks in late September and early October, which is when bulls answer a call. The cover map shows wetland and shrub in the valley floors.</p>");
        b.push_str("<h2>Mule deer and white-tailed deer <span class=\"tag open\">General</span></h2><p>Mule deer are the deer of the high country and the subalpine edge; white-tails keep to the valley bottoms and the river flats. The mule deer rut runs from late October and peaks in mid November, which falls inside the season and is when the big bucks show themselves in daylight.</p>");
        b.push_str("<h2>Black bear, wolf, cougar</h2><p>Black bears are common in the timber and on autumn berry slopes. Baiting is not allowed in 446. Wolves and cougars are both present and both prey on the goats; you are far more likely to see sign than the animal.</p>");
        b.push_str("<h2>Birds and small company</h2><p>Nobody has published a bird list for this ridge that this guide could find, so take this as what the northern Rockies alpine usually holds rather than a record: spruce and ruffed grouse in the timber, dusky grouse near treeline, ptarmigan on the tundra. The goat study does record golden eagles, which take kids when they can, along with wolverine and coyote.</p>");
        b.push_str("<h2>The rut, by the calendar</h2><div class=\"scroll\"><table><thead><tr><th>Species</th><th>Rut</th><th>Peak</th><th>Against the 2026 season</th></tr></thead><tbody>\
<tr><td>Elk</td><td>1 September to 15 October</td><td>Mid to late September</td><td>General season opens 17 September, on the peak</td></tr>\
<tr><td>Moose</td><td>Late September to mid October</td><td>First days of October</td><td>Rifle season opens 24 September</td></tr>\
<tr><td>Mule deer</td><td>Late October to early December</td><td>Mid November</td><td>Inside the season, which closes 30 November</td></tr>\
<tr><td>Mountain goat</td><td>November</td><td>Mid to late November</td><td>No season</td></tr>\
<tr><td>Bighorn sheep</td><td>Mid November to mid December</td><td>Late November</td><td>Season closes 31 October</td></tr></tbody></table></div><p class=\"small\">Rut dates are general figures for Alberta and the northern Rockies, not measured on this ridge.</p></main>");
        emit(Page { slug: "wildlife.html", title: "Wildlife", description: "The animals of Caw Ridge: the long studied mountain goat herd, bighorn sheep, caribou, grizzly bear, elk, moose and deer, with rut timing and identification.", body: b, head: "", scripts: "", footer: true });
    }

    // ------------------------------------------------------------- weather
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Weather</p><h1>Sky, snow and light</h1>");
        let _ = write!(b, "<p class=\"lede\">The ridge top is {:.0} m above Grande Cache and lives in different weather. Expect it four to five degrees colder than town, windier, and white weeks earlier.</p></div>", world.wpt_elev - 1250.0);
        b.push_str("<h2>Forecast for the waypoint</h2>");
        b.push_str(&forecast_block);
        b.push_str("<p>For a second opinion before you leave town:</p><ul>\
<li><a href=\"https://weather.gc.ca/en/location/index.html?coords=53.888,-119.119\">Environment Canada, Grande Cache</a>: the official forecast and any warnings, for the valley.</li>\
<li><a href=\"https://spotwx.com/products/grib_index.php?model=gem_lam_continental&amp;lat=54.06271&amp;lon=-119.39073\">SpotWx</a>: Canadian high resolution model runs for the exact point.</li>\
<li><a href=\"https://www.windy.com/54.063/-119.391?54.063,-119.391,10\">Windy</a>: wind at ridge height, animated.</li>\
<li><a href=\"https://avalanche.ca/forecasts/north-rockies\">Avalanche Canada, North Rockies</a>: once the snow is down. Forecasts run in winter only.</li>\
<li><a href=\"https://www.albertafirebans.ca/\">Alberta fire bans</a> and <a href=\"https://511.alberta.ca/\">511 Alberta</a> for Highway 40.</li></ul>");

        let _ = write!(b, "<h2>Ten autumns at ridge height</h2><p>There is no weather station on Caw Ridge. What follows is the ERA5 reanalysis, a reconstruction of past weather from every observation available, read at the waypoint and adjusted to {:.0} m, for the autumns of {} to {}. It is a model of the past and smooths the extremes, the wind most of all.</p>", world.wpt_elev, clim.years.0, clim.years.1);
        let spans: Vec<Span> = clim
            .weeks
            .iter()
            .map(|w| Span {
                label: format!("{} {}", w.start.1, sun::month_name(w.start.0)),
                lo: w.mean_min,
                hi: w.mean_max,
                tip: format!("Week of {} {}|{:.0}\u{b0} to {:.0}\u{b0}C on an average day|Coldest night on record {:.0}\u{b0}C|New snow on {} of days", w.start.1, sun::month_name(w.start.0), w.mean_min, w.mean_max, w.lowest, pct(w.snow_days)),
            })
            .collect();
        let rows: Vec<Vec<String>> = clim
            .weeks
            .iter()
            .map(|w| vec![format!("{} {}", w.start.1, sun::month_name(w.start.0)), format!("{:.1}", w.mean_max), format!("{:.1}", w.mean_min), format!("{:.0}", w.highest), format!("{:.0}", w.lowest), pct(w.snow_days), pct(w.wet_days), format!("{:.0}", w.mean_wind), format!("{:.0}", w.mean_gust), format!("{:.0}", w.top_gust)])
            .collect();
        let _ = write!(
            b,
            "<div class=\"viz\"><h3>Average daily range of temperature</h3><p class=\"sub\">Mean daily low to mean daily high by week, &deg;C, at {:.0} m</p>{}{}</div>",
            world.wpt_elev,
            chart::ranges("Average daily temperature range by week", &spans, "\u{b0}C", "Freezing"),
            chart::table("Show the numbers", &["Week of", "Mean high", "Mean low", "Warmest day", "Coldest night", "Days with new snow", "Days with rain or snow", "Wind km/h", "Gust km/h", "Top gust"], &rows)
        );
        let w_now = clim.weeks.iter().rev().find(|w| sun::days_from_civil(2026, w.start.0, w.start.1) <= built_days).or(clim.weeks.first());
        if let Some(w) = w_now {
            let _ = write!(
                b,
                "<p>In the week of {} {}, an average day on the ridge runs from {:.0}&deg; at night to {:.0}&deg; in the afternoon. The coldest night in the record for that week was {:.0}&deg;C. New snow fell on {} of days, and the day's strongest gust averaged {:.0} km/h. By the first week of November the average afternoon is below freezing.</p>",
                w.start.1,
                sun::month_name(w.start.0),
                w.mean_min,
                w.mean_max,
                w.lowest,
                pct(w.snow_days),
                w.mean_gust
            );
        }
        let names = ["North", "North-east", "East", "South-east", "South", "South-west", "West", "North-west"];
        let bars: Vec<Bar> = names.iter().zip(clim.rose.sectors).map(|(n, s)| Bar { label: format!("From the {}", n.to_lowercase()), value: s.0, text: pct(s.0), tip: format!("Wind from the {}|{} of days|Typical top speed {:.0} km/h", n.to_lowercase(), pct(s.0), s.1) }).collect();
        let top = names.iter().zip(clim.rose.sectors).max_by(|a, c| a.1 .0.partial_cmp(&c.1 .0).unwrap()).map(|x| (*x.0, x.1)).unwrap();
        let _ = write!(
            b,
            "<h2>Wind</h2><div class=\"cols\"><div><div class=\"viz\"><h3>Where the wind comes from</h3><p class=\"sub\">Share of days by prevailing direction, 15 September to 30 November</p>{}</div></div><div><p>The wind on Caw Ridge is a westerly wind. It came from the {} on {} of autumn days in the record, and from the west or south-west on {}. Plan on it: approach from the east and north-east, glass from the lee side of a crest, and expect your scent to pour down the eastern slopes.</p><p>Thermals work under the prevailing wind when it is light. They run uphill on sunlit slopes from mid-morning and downhill from dusk to dawn.</p><p>A steady 30 km/h wind at -5&deg;C feels like -13&deg;C on bare skin. Gusts over 60 km/h are ordinary on the crest.</p></div></div>",
            chart::bars("Share of days by wind direction", &bars),
            top.0.to_lowercase(),
            pct(top.1 .0),
            pct(clim.rose.sectors[5].0 + clim.rose.sectors[6].0)
        );

        b.push_str("<h2>When the snow comes</h2><p>Natural Resources Canada has read every clear Landsat and Sentinel-2 image of seven winters and worked out, for each 30 m cell in the country, when the snow arrived. At the waypoint:</p><div class=\"scroll\"><table><thead><tr><th>Winter</th><th>First snow period began</th><th>Known to within</th><th>Longest snow period began</th></tr></thead><tbody>");
        for s in &p.snow {
            let first = s.first.map(|f| (date_short(f.0), format!("{:.0} days either way", f.1), f.1)).unwrap_or(("No record".into(), String::new(), 99.0));
            let weak = if first.2 > 15.0 { " <span class=\"tag\">Cloud gap</span>" } else { "" };
            let _ = write!(b, "<tr><td>{}</td><td>{}{}</td><td>{}</td><td>{}</td></tr>", s.winter, first.0, weak, first.1, s.lasting.map(|l| { let (y, _, _) = sun::civil_from_days(l); format!("{} {}", date_short(l), y) }).unwrap_or_else(|| "No record".into()));
        }
        b.push_str("</tbody></table></div><p>A satellite sees the ground only between clouds, so each date is the midpoint between the last snow free view and the first white one. Where the gap was more than a fortnight the date is marked and should be set aside. In the winters with a good record, the first lasting snow at the waypoint came between 23 September and 21 October. On a windswept crest the snow also leaves again: the ridge can blow bare in a chinook and whiten the next night.</p>");
        b.push_str(&map_figure(p, "ridge_snow", "The usual date of first snow across the ridge, from the winters with a good record."));
        b.push_str("<div class=\"notice\"><h3>Early season avalanches</h3><p>The first snows of autumn fall on bare, smooth tundra and rock and are moved about by wind into slabs on lee slopes. A slope of 30 to 45 degrees below a crest, loaded by a west wind, is where an October slab releases. The slope map shows where those angles are. If the snow is over your boots and the slope is steep, go around.</p></div>");

        b.push_str("<h2 id=\"light\">Legal light, sun and moon</h2><p>Times are for the waypoint on a level horizon, in clock time. Legal hunting hours run from half an hour before sunrise to half an hour after sunset. Clocks go back one hour on Sunday 1 November 2026.</p>");
        let days: Vec<LightDay> = rows_to_days(&rows_filter(&light));
        let dst_i = light.iter().position(|r| r.zone == "MST");
        let _ = write!(b, "<div class=\"viz\"><h3>Daylight through the season</h3><p class=\"sub\">Clock time of legal light (pale band) and of the sun above the horizon (darker band)</p>{}</div>", chart::daylight("Legal light and daylight by date", &days, dst_i.map(|i| (i, "Clocks go back"))));
        b.push_str("<div class=\"scroll\" style=\"max-height:560px;overflow-y:auto\"><table><thead><tr><th>Date</th><th class=\"n\">Legal from</th><th class=\"n\">Sunrise</th><th class=\"n\">Sunset</th><th class=\"n\">Legal until</th><th class=\"n\">Day length</th><th class=\"n\">First light</th><th class=\"n\">Last light</th><th>Moon</th></tr></thead><tbody>");
        for r in &light {
            let (y, m, d) = sun::civil_from_days(r.days);
            let wd = sun::weekday(r.days);
            let cls = if wd == "Sat" || wd == "Sun" { " class=\"weekend\"" } else { "" };
            let len = r.set - r.rise;
            let _ = write!(
                b,
                "<tr data-date=\"{y}-{m:02}-{d:02}\"{cls}><td>{}</td><td class=\"n\"><b>{}</b></td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\"><b>{}</b></td><td class=\"n\">{}h {:02}m</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{} {}</td></tr>",
                format!("{} {} {}", wd, d, sun::month_name(m)),
                sun::hm(r.legal_start),
                sun::hm(r.rise),
                sun::hm(r.set),
                sun::hm(r.legal_end),
                len.floor(),
                ((len - len.floor()) * 60.0).round(),
                sun::hm(r.civil_start),
                sun::hm(r.civil_end),
                pct(r.moon_frac as f32),
                r.moon_name
            );
        }
        b.push_str("</tbody></table></div><p class=\"small\">First and last light are civil twilight, when the sun is six degrees below the horizon and there is enough light to walk by. Computed with the NOAA solar equations; good to about a minute. The terrain horizon is not counted, so the sun itself will show later and leave earlier in a basin.</p></main>");
        emit(Page { slug: "weather.html", title: "Weather", description: "Forecast, climate, wind, first snow dates and legal light tables for Caw Ridge at 1,991 m.", body: b, head: "", scripts: "", footer: true });
    }

    // -------------------------------------------------------------- safety
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Safety</p><h1>Coming home</h1><p class=\"lede\">You will be two hours from a hospital on a good day, in grizzly country, above treeline in October, with no phone signal. None of that is a reason to stay home. All of it is a reason to prepare.</p></div>");
        b.push_str("<h2>In an emergency</h2><div class=\"phones\">\
<div class=\"phone\"><div class=\"who\">Police, fire, ambulance, rescue</div><a class=\"num\" href=\"tel:911\">911</a><p>Search and rescue is sent by the RCMP. Give coordinates.</p></div>\
<div class=\"phone\"><div class=\"who\">Grande Cache hospital</div><a class=\"num\" href=\"tel:17808273701\">780-827-3701</a><p>Community Health Complex, 10200 Shand Avenue. Emergency department open 24 hours.</p></div>\
<div class=\"phone\"><div class=\"who\">RCMP Grande Cache</div><a class=\"num\" href=\"tel:17808272222\">780-827-2222</a><p>Non-emergency line. 9906 Shand Avenue West.</p></div>\
<div class=\"phone\"><div class=\"who\">Wildfire</div><a class=\"num\" href=\"tel:3103473\">310-FIRE</a><p>310-3473. Report smoke or fire.</p></div>\
<div class=\"phone\"><div class=\"who\">Dangerous wildlife, poaching</div><a class=\"num\" href=\"tel:18006423800\">1-800-642-3800</a><p>Report A Poacher line, 24 hours. Also for a bear on a kill.</p></div></div>");
        let u = geo::utm_string(config::WPT_LON, config::WPT_LAT);
        let _ = write!(
            b,
            "<div class=\"notice\"><h3>What to tell them</h3><p>Say: <b>Caw Ridge, north-west of Grande Cache, off Beaverdam Road.</b> Then give a position. The waypoint is <span class=\"mono\">{:.5}, {:.5}</span>, or <span class=\"mono\">{}</span>. The nearest helicopter ambulance base is STARS at Grande Prairie. A two-way satellite messenger lets rescuers ask questions; a one-way beacon only shouts.</p></div>",
            config::WPT_LAT, config::WPT_LON, u
        );
        b.push_str("<h2>Communications</h2><ul>\
<li><b>Phone.</b> Coverage is confirmed in Grande Cache only. Assume none past the highway. A high point may catch a bar; do not plan on it.</li>\
<li><b>Satellite messenger.</b> inReach, Zoleo, Spot, or a phone with satellite messaging. Test it before you leave, and agree check-in times.</li>\
<li><b>Trip plan.</b> Leave one with someone who will act on it: where, which vehicle, who, when you are due out, and when to call 911 if you are not.</li>\
<li><b>This guide offline.</b> Save it to your phone from the <a href=\"index.html\">overview page</a> while you have wifi.</li></ul>");
        b.push_str("<h2>Bears</h2><p>This is some of the densest grizzly country in Alberta, and a hunter does everything a bear safety course says not to: moves quietly, into the wind, at dawn and dusk, and then makes a pile of meat. The province's advice for hunters:</p><ul>\
<li>Carry bear spray on your belt or chest, where a hand finds it in one second. In the pack it is no use. Carry a noisemaker too.</li>\
<li>Hunt with a partner. One works, one watches.</li>\
<li>After the shot, make noise as you approach and while you work. Gut and quarter without delay.</li>\
<li>Move the carcass at least 200 m from the gut pile if you must leave it, and leave it in the open where you can see it from a distance.</li>\
<li>Hang meat at least 3.5 m off the ground and 1.5 m out from the trunk, and at least 100 m from camp. Above treeline there is nothing to hang from, so get the meat down the same day.</li>\
<li>Coming back to a kill, approach from upwind, stop well out and glass it. If the carcass has been moved or buried, or a bear is on it, leave. It is the bear's now. Call 310-0000 or 1-800-642-3800.</li>\
<li>Keep a clean camp: food, game bags and bloody clothes away from where you sleep.</li></ul><p><a href=\"https://www.alberta.ca/bears-and-hunters\">Bears and hunters</a>, Government of Alberta.</p>");
        b.push_str("<h2>Cold, wind and light</h2><ul>\
<li><b>Hypothermia</b> is a wet, windy, two degree day more often than a cold one. Pack a dry layer, a shell, a hat and gloves you do not take out until you need them.</li>\
<li><b>Whiteout.</b> Cloud sits on the ridge for days at a time. The tundra has few landmarks. Mark the track end on your GPS before you leave it, and carry the paper map and a compass set to the declination.</li>\
<li><b>Dark.</b> In early October you have about eleven and a half hours of daylight; by mid November, under nine. A pack-out that starts at last light ends by headlamp. Carry two lights.</li>\
<li><b>Water.</b> The crest is dry. Streams start a few hundred metres down. Carry what you need and treat what you find.</li>\
<li><b>Creeks.</b> Mountain creeks rise within minutes of rain upstream. The crossing at the end of Beaverdam Road is a ford.</li></ul>");
        b.push_str("<h2>Firearms and other people</h2><ul>\
<li>The ridge is shared with riders, researchers, hikers and other hunters. Know what is behind your target: on a convex ridge, the backstop is often sky.</li>\
<li>Unload before you get on a quad or into a truck. A loaded firearm in or on a vehicle is unlawful, and it is one of the commonest ways hunters are shot.</li>\
<li>Wear some orange above treeline even though Alberta does not require it.</li></ul>");
        b.push_str("<h2>Packing list</h2><p>Tick things off as you pack. Your ticks are remembered on this device.</p><div class=\"btnrow\"><button class=\"btn ghost\" type=\"button\" data-reset-checks>Clear all ticks</button></div>");
        let mut n = 0;
        for (head, items) in [
            ("Papers", vec!["Wildlife certificate and WiN card", "Species licences and paper tags", "Draw licence, if you hold one", "Firearms licence (PAL)", "Public Lands Camping Pass", "The hunting regulations guide, printed or saved", "Trip plan left with someone at home"]),
            ("Finding your way", vec!["Printed topographic maps from this guide", "Compass, set to 16 degrees east", "GPS with the GPX file loaded, spare batteries", "Phone with this guide saved offline", "Satellite messenger, tested", "Power bank and cable"]),
            ("Bear country", vec!["Bear spray on the belt, one per person", "Noisemaker", "Rope and pulley for hanging meat", "Odour proof bags for food"]),
            ("Clothing", vec!["Waterproof shell, jacket and trousers", "Insulating layer: down or synthetic", "Base layers, not cotton", "Warm hat and two pairs of gloves", "Broken in boots and gaiters", "Spare socks", "Something orange"]),
            ("The hunt", vec!["Rifle or bow, sighted in", "Ammunition, legal for big game", "Binoculars and spotting scope with tripod", "Rangefinder", "Knives and a sharpener", "Bone saw", "Game bags", "Pack frame for meat", "Flagging tape", "Latex gloves"]),
            ("Camp and survival", vec!["Headlamp and a spare", "Fire starter, two kinds", "First aid kit with a tourniquet", "Emergency bivouac bag", "Water, at least 3 litres each, and a filter", "Food for a day longer than planned", "Tent rated for wind, extra pegs", "Sleeping bag rated to minus 10 or colder", "Stove and fuel"]),
            ("Vehicle", vec!["Full tank and a jerry can", "Full size spare, jack, plug kit and compressor", "Tow strap, shovel, axe or saw", "Tire chains once the snow is down", "Quad or side-by-side: registration, insurance, helmet"]),
        ] {
            let _ = write!(b, "<h3>{head}</h3><ul class=\"check\">");
            for it in items {
                n += 1;
                let _ = write!(b, "<li><label><input type=\"checkbox\" id=\"c{n}\"><span>{it}</span></label></li>");
            }
            b.push_str("</ul>");
        }
        b.push_str("</main>");
        emit(Page { slug: "safety.html", title: "Safety", description: "Emergency contacts, bear safety and meat care, communications, weather hazards and a packing list for hunting Caw Ridge.", body: b, head: "", scripts: "", footer: true });
    }

    // ------------------------------------------------------------- sources
    {
        let mut b = String::from("<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Sources</p><h1>Where this comes from</h1><p class=\"lede\">Everything in this guide is drawn from public sources, by a program you can read. This page lists the sources, says how the maps were made, and is honest about what is not known.</p></div>");
        let _ = write!(b, "<h2>The data</h2><div class=\"scroll\"><table><thead><tr><th>What</th><th>Source</th><th>Notes</th></tr></thead><tbody>\
<tr><td>Elevation</td><td><a href=\"https://open.canada.ca/data/en/dataset/18752265-bda3-498c-a4ba-9dfe68cb98da\">Medium Resolution Digital Elevation Model (MRDEM-30)</a>, Natural Resources Canada</td><td>30 m bare earth model. Read directly from the national cloud optimized file. Open Government Licence, Canada. The 1 to 2 m lidar model does not cover the ridge.</td></tr>\
<tr><td>Ground cover</td><td><a href=\"https://open.canada.ca/data/en/dataset/ee1580ab-a23d-4f86-a09b-79763677eb47\">2020 Land Cover of Canada</a>, Natural Resources Canada</td><td>30 m classes from Landsat. Open Government Licence, Canada.</td></tr>\
<tr><td>Snow onset</td><td>Annual 30 m snow dynamics, 2018 to 2025, Natural Resources Canada</td><td>First and longest snow periods for seven winters. Open Government Licence, Canada.</td></tr>\
<tr><td>Satellite imagery</td><td><a href=\"https://dataspace.copernicus.eu/\">Copernicus Sentinel-2</a> level 2A, {}, through Element 84 Earth Search</td><td>Contains modified Copernicus Sentinel data 2026. Scenes: {}.</td></tr>\
<tr><td>Wildlife management units, parks, wildlife ranges, coal agreements</td><td><a href=\"https://geospatial.alberta.ca/\">Government of Alberta geospatial services</a></td><td>Open Government Licence, Alberta.</td></tr>\
<tr><td>Roads, trails, rivers, summits, services</td><td><a href=\"https://www.openstreetmap.org/copyright\">OpenStreetMap contributors</a></td><td>Open Database Licence. Mapped by volunteers; may be missing or out of date.</td></tr>\
<tr><td>Place names</td><td><a href=\"https://natural-resources.canada.ca/earth-sciences/geography/geographical-names-board-canada\">Canadian Geographical Names Database</a></td><td>Open Government Licence, Canada.</td></tr>\
<tr><td>Weather history and forecast</td><td><a href=\"https://open-meteo.com/\">Open-Meteo</a></td><td>ERA5 reanalysis and global forecast models. Creative Commons Attribution 4.0.</td></tr>\
<tr><td>Magnetic declination</td><td><a href=\"https://www.ncei.noaa.gov/products/world-magnetic-model\">World Magnetic Model 2025</a>, NOAA and the British Geological Survey</td><td>Public domain. Computed by this guide's program and checked against the published test values.</td></tr>\
<tr><td>Regulations</td><td><a href=\"https://albertaregulations.ca/huntingregs/\">2026 Alberta Guide to Hunting Regulations</a>; Wildlife Regulation, Alta Reg 143/97</td><td>Read on 28 September 2026.</td></tr>\
<tr><td>Goat study and ridge ecology</td><td><a href=\"https://www.albertawilderness.ca/wp-content/uploads/20100400_ar_wla_goats_cote.pdf\">C&ocirc;t&eacute; (2010), Wild Lands Advocate</a>; <a href=\"https://pmc.ncbi.nlm.nih.gov/articles/PMC6405896/\">published papers of the Caw Ridge study</a>; <a href=\"https://www.albertawilderness.ca/issues/wildlands/areas-of-concern/kakwa/\">Alberta Wilderness Association, Kakwa</a></td><td></td></tr>\
<tr><td>Access directions</td><td><a href=\"https://mdgreenview.ab.ca/tourism/quadding-and-snowmobiling/\">Municipal District of Greenview</a></td><td>Road reports from public forum threads, 2008 to 2023, marked as such where used.</td></tr>\
<tr><td>Bear safety</td><td><a href=\"https://www.alberta.ca/bears-and-hunters\">Government of Alberta, Bears and hunters</a></td><td></td></tr>\
<tr><td>Map lettering and site type</td><td>Barlow by Jeremy Tribby, PT Serif by ParaType, IBM Plex Mono by IBM</td><td>SIL Open Font Licence.</td></tr>\
<tr><td>Interactive map</td><td><a href=\"https://leafletjs.com/\">Leaflet</a></td><td>BSD 2-Clause licence.</td></tr></tbody></table></div>", sat_date, esc(&world.sat_ridge.scenes.join(", ")));
        b.push_str("<h2>How the maps are made</h2><p>One program, <code>cawridge</code>, written in Rust, does all of it. It has two commands. <code>cawridge fetch</code> downloads the data; <code>cawridge build</code> draws the maps and writes the site.</p><ul>\
<li><b>Reading the elevation.</b> The national elevation model is a single file of several hundred gigabytes. It is stored so that any window can be read without the rest: the program asks the server for the index, then for only the 36 tiles that cover the region. It takes about three seconds. The projection from latitude and longitude to the file's Lambert grid is worked out in the program and tested against the PROJ library.</li>\
<li><b>Relief.</b> Slope and aspect by Horn's method; shading from four lights so that slopes facing away from the main light keep their form.</li>\
<li><b>Contours.</b> Marching squares on a bicubic resampling of the model, joined into lines and thinned.</li>\
<li><b>Streams.</b> Pits in the model are filled, each cell passes its water to its lowest neighbour, and cells with enough ground above them become streams. Big rivers come from OpenStreetMap instead.</li>\
<li><b>Sight lines.</b> For each target cell the ground between it and the eye is checked against the straight line between them, allowing for the curve of the earth and the bending of light. Eye at 1.7 m, target at 1.0 m.</li>\
<li><b>Sun.</b> The sun's position every ten minutes from the NOAA solar equations; a ray marched from each cell towards it until it clears the highest ground or strikes a slope.</li>\
<li><b>Walking time.</b> Dijkstra's algorithm over the grid with Tobler's hiking function as the cost.</li>\
<li><b>Lettering and line work</b> are composed as SVG and rasterised by the resvg library, so the maps need no browser and no mapping software to build.</li></ul>");
        b.push_str("<h2>What is not known</h2><p>The research for this guide could not settle the following. Treat each as an open question and ask locally.</p><ul>\
<li>Whether the conservation area named in the Upper Smoky Sub-regional Plan has been legally created, and where off-highway vehicles will be allowed within it.</li>\
<li>The present state of gates, signs and bridges on Beaverdam Road and Sheep Creek Road, and any check-in or firearm rule the mine applies to traffic through its property.</li>\
<li>Whether a radio is required on Beaverdam Road, and on which channel.</li>\
<li>Whether the goat study is active this season, and the present size of the herd.</li>\
<li>Phone coverage on the ridge, and water on the crest.</li>\
<li>The wolf season end date for 446: the guide gives 31 May, or 15 June in units where the bear season runs to 15 June, and 446 fits neither case cleanly.</li>\
<li>The origin of the name. \"Caw Ridge\" is not an official name: the Canadian names database knows only Caw Creek, and older writing calls the ridge Copton Ridge, a name the database now places about 11 km to the north.</li>\
<li>Two published numbers exist for the hospital. This guide gives the one on the facility's own page. In an emergency call 911.</li></ul>");
        b.push_str("<h2>Limits of the maps</h2><ul>\
<li>The elevation model's cell is 30 m. Features smaller than that are smoothed away. Slope reads low on short cliffs. The 10 m contours of the close sheet are interpolated.</li>\
<li>Sight lines are over bare ground. Trees, krummholz and boulders hide more than the maps show.</li>\
<li>Ground cover is a 2020 satellite classification. Cutblocks, burns and mine work since then do not appear; the satellite image of September 2026 does show them.</li>\
<li>Boundaries are drawn from official files but the legal boundary is the written description.</li>\
<li>Positions are on NAD83, which agrees with the WGS84 of a GPS to within about a metre and a half here.</li></ul>");
        let _ = write!(b, "<p class=\"small\">Built {built}.</p></main>");
        emit(Page { slug: "sources.html", title: "Sources", description: "Data sources, methods, licences and known limits of the Caw Ridge field guide.", body: b, head: "", scripts: "", footer: true });
    }

    for (slug, html) in &pages {
        std::fs::write(out.join(slug), html)?;
    }
    std::fs::write(out.join("404.html"), layout(&Page { slug: "404.html", title: "Not found", description: "Page not found", body: "<main class=\"wrap\"><div class=\"pagehead\"><p class=\"kicker\">Off the map</p><h1>No such page</h1><p class=\"lede\">That page does not exist. <a href=\"index.html\">Back to the overview</a>.</p></div></main>".into(), head: "", scripts: "", footer: true }, built))?;

    // Offline support.
    let mut core: Vec<String> = vec!["./".into()];
    core.extend(pages.iter().map(|p| p.0.clone()));
    for f in ["assets/style.css", "assets/app.js", "assets/viewer.js", "assets/explore.js", "assets/icon.svg", "assets/icon-180.png", "vendor/leaflet/leaflet.js", "vendor/leaflet/leaflet.css", "maps.json", "offline.json", "data/cawridge.geojson", "data/elev_ridge.png", "maps/hero.jpg", "manifest.webmanifest"] {
        core.push(f.into());
    }
    for e in std::fs::read_dir(out.join("assets/fonts"))? {
        core.push(format!("assets/fonts/{}", e?.file_name().to_string_lossy()));
    }
    for m in &p.maps {
        core.push(format!("maps/thumb/{}.jpg", m.id));
        core.push(format!("maps/mid/{}.jpg", m.id));
    }
    let mut all: Vec<String> = Vec::new();
    for m in &p.maps {
        all.push(m.view.clone());
        all.push(m.bare.clone());
    }
    all.push("data/cawridge.gpx".into());
    std::fs::write(out.join("offline.json"), serde_json::to_string(&all)?)?;
    let sw = std::fs::read_to_string("assets/site/sw.js")?.replace("__VERSION__", &built.replace([' ', ':'], "-")).replace("__FILES__", &serde_json::to_string(&core)?);
    std::fs::write(out.join("sw.js"), sw)?;
    let manifest = serde_json::json!({
        "name": "Caw Ridge field guide",
        "short_name": "Caw Ridge",
        "description": "Maps, terrain, regulations, weather and safety for hunting Caw Ridge, Alberta.",
        "start_url": "index.html",
        "scope": "./",
        "display": "standalone",
        "background_color": "#f6f3ea",
        "theme_color": "#1d1d1b",
        "icons": [
            {"src": "assets/icon-180.png", "sizes": "180x180", "type": "image/png"},
            {"src": "assets/icon-512.png", "sizes": "512x512", "type": "image/png"},
            {"src": "assets/icon.svg", "sizes": "any", "type": "image/svg+xml"}
        ]
    });
    std::fs::write(out.join("manifest.webmanifest"), serde_json::to_string_pretty(&manifest)?)?;
    eprintln!("wrote {} pages", pages.len() + 1);
    Ok(())
}

fn rows_filter(rows: &[SunRow]) -> Vec<&SunRow> {
    rows.iter().collect()
}

fn rows_to_days(rows: &[&SunRow]) -> Vec<LightDay> {
    rows.iter()
        .map(|r| {
            let (_, m, d) = sun::civil_from_days(r.days);
            LightDay {
                label: format!("{} {}", d, sun::month_name(m)),
                tick: d == 1 || d == 15,
                t: [r.legal_start as f32, r.rise as f32, r.set as f32, r.legal_end as f32],
                tip: format!("{}|Legal light {} to {}|Sunrise {}, sunset {} {}", date_long(r.days), sun::hm(r.legal_start), sun::hm(r.legal_end), sun::hm(r.rise), sun::hm(r.set), r.zone),
            }
        })
        .collect()
}
