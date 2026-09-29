//! Files for the GPS, the phone and Google Earth.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use crate::cog::Res;
use crate::draw::esc;
use crate::products::{MapOut, Products};

#[derive(Clone)]
pub struct Place {
    pub name: String,
    pub kind: &'static str,
    pub lon: f64,
    pub lat: f64,
    pub elev: f32,
    pub note: String,
}

pub fn gpx(places: &[Place], p: &Products) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"cawridge\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n<metadata><name>Caw Ridge</name><desc>Waypoints, the drive from Grande Cache and the computed walking line. Planning aid only.</desc></metadata>\n");
    for pl in places {
        let _ = writeln!(
            s,
            "<wpt lat=\"{:.6}\" lon=\"{:.6}\"><ele>{:.0}</ele><name>{}</name><desc>{}</desc><type>{}</type></wpt>",
            pl.lat,
            pl.lon,
            pl.elev,
            esc(&pl.name),
            esc(&pl.note),
            pl.kind
        );
    }
    s.push_str("<trk><name>Drive: Grande Cache to the end of the mapped track</name>");
    for l in &p.drive {
        s.push_str("<trkseg>");
        for pt in &l.pts {
            let _ = write!(s, "<trkpt lat=\"{:.6}\" lon=\"{:.6}\"/>", pt.1, pt.0);
        }
        s.push_str("</trkseg>");
    }
    s.push_str("</trk>\n<trk><name>Walk: end of track to the waypoint (computed line, not a trail)</name><trkseg>");
    for pt in &p.walk {
        let _ = write!(s, "<trkpt lat=\"{:.6}\" lon=\"{:.6}\"/>", pt.1, pt.0);
    }
    s.push_str("</trkseg></trk>\n</gpx>\n");
    s
}

pub fn geojson(places: &[Place], p: &Products) -> String {
    let mut feats = Vec::new();
    for pl in places {
        feats.push(serde_json::json!({
            "type": "Feature",
            "properties": {"name": pl.name, "kind": pl.kind, "elevation_m": pl.elev.round(), "note": pl.note},
            "geometry": {"type": "Point", "coordinates": [pl.lon, pl.lat]}
        }));
    }
    for l in &p.drive {
        feats.push(serde_json::json!({
            "type": "Feature",
            "properties": {"name": if l.name.is_empty() { "Unnamed".to_string() } else { l.name.clone() }, "kind": format!("drive-{}", l.kind), "km": (l.km * 10.0).round() / 10.0},
            "geometry": {"type": "LineString", "coordinates": l.pts.iter().map(|q| vec![q.0, q.1]).collect::<Vec<_>>()}
        }));
    }
    feats.push(serde_json::json!({
        "type": "Feature",
        "properties": {"name": "Computed walking line", "kind": "walk"},
        "geometry": {"type": "LineString", "coordinates": p.walk.iter().map(|q| vec![q.0, q.1]).collect::<Vec<_>>()}
    }));
    serde_json::json!({"type": "FeatureCollection", "features": feats}).to_string()
}

fn kml_body(places: &[Place], p: &Products) -> String {
    let mut s = String::new();
    for pl in places {
        let _ = writeln!(
            s,
            "<Placemark><name>{}</name><description>{}</description><Point><coordinates>{:.6},{:.6},{:.0}</coordinates></Point></Placemark>",
            esc(&pl.name),
            esc(&pl.note),
            pl.lon,
            pl.lat,
            pl.elev
        );
    }
    let line = |name: &str, colour: &str, pts: &[(f64, f64)]| -> String {
        let mut c = String::new();
        for q in pts {
            let _ = write!(c, "{:.6},{:.6},0 ", q.0, q.1);
        }
        format!("<Placemark><name>{}</name><Style><LineStyle><color>{colour}</color><width>3</width></LineStyle></Style><LineString><tessellate>1</tessellate><coordinates>{c}</coordinates></LineString></Placemark>\n", esc(name))
    };
    let drive: Vec<(f64, f64)> = p.drive.iter().flat_map(|l| l.pts.clone()).collect();
    s.push_str(&line("Drive from Grande Cache", "ff1b59e0", &drive));
    s.push_str(&line("Computed walking line", "ff1e26d7", &p.walk));
    s
}

pub fn kml(places: &[Place], p: &Products) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document><name>Caw Ridge</name>\n{}</Document></kml>\n", kml_body(places, p))
}

/// A KMZ that lays one of the map images over the ground in Google Earth and in phone
/// apps that read KMZ overlays.
pub fn kmz(out: &Path, site: &Path, m: &MapOut, places: &[Place], p: &Products) -> Res<()> {
    let img = std::fs::read(site.join(&m.bare))?;
    let doc = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document><name>{}</name>\n<GroundOverlay><name>{}</name><Icon><href>map.jpg</href></Icon><LatLonBox><north>{}</north><south>{}</south><east>{}</east><west>{}</west></LatLonBox></GroundOverlay>\n{}</Document></kml>\n",
        esc(&m.title),
        esc(&m.title),
        m.bbox.n,
        m.bbox.s,
        m.bbox.e,
        m.bbox.w,
        kml_body(places, p)
    );
    let f = std::fs::File::create(out)?;
    let mut z = zip::ZipWriter::new(f);
    let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    z.start_file("doc.kml", opt)?;
    z.write_all(doc.as_bytes())?;
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    z.start_file("map.jpg", stored)?;
    z.write_all(&img)?;
    z.finish()?;
    Ok(())
}
