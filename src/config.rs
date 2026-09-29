//! The one place where the trip is described. Change the waypoint or the boxes here and
//! every map, table and page follows.

/// The waypoint the whole site is built around.
pub const WPT_LAT: f64 = 54.062707;
pub const WPT_LON: f64 = -119.390728;

/// A lon/lat box: west, south, east, north.
#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub w: f64,
    pub s: f64,
    pub e: f64,
    pub n: f64,
}

impl BBox {
    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        lon >= self.w && lon <= self.e && lat >= self.s && lat <= self.n
    }
}

/// Everything that is downloaded covers this box: Grande Cache and Highway 40 in the
/// southeast corner, the ridge in the middle, the Kakwa country to the northwest.
pub const REGION: BBox = BBox { w: -119.85, s: 53.80, e: -118.95, n: 54.30 };

/// The ridge and the roads that reach it.
pub const RIDGE: BBox = BBox { w: -119.58, s: 53.97, e: -119.20, n: 54.16 };

/// The ground within a morning's walk of the waypoint.
pub const CLOSE: BBox = BBox { w: -119.47, s: 54.02, e: -119.31, n: 54.105 };

pub const MRDEM_DTM: &str =
    "https://canelevation-dem.s3.ca-central-1.amazonaws.com/mrdem-30/mrdem-30-dtm.tif";

pub const OVERPASS: [&str; 3] = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
];

/// Window searched for a clear satellite pass. The newest clear day inside it wins.
pub const SAT_FROM: &str = "2026-08-15";
pub const SAT_TO: &str = "2026-09-28";

const AB_TITAN: &str = "https://geospatial.alberta.ca/titan/rest/services";

/// Government of Alberta map layers, read through their public ArcGIS REST services.
pub const ARCGIS: [(&str, &str); 7] = [
    ("wmu", "https://geospatial.alberta.ca/mimas/rest/services/boundaries/fishwild_wildlife_mgmt_unit_public/FeatureServer/0"),
    ("parks", "https://geospatial.alberta.ca/titan/rest/services/boundary/parks_protected_areas_alberta/MapServer/0"),
    ("goat_sheep", "https://geospatial.alberta.ca/titan/rest/services/biota/wildlife_sensitivity_mammals_10tm_nad83_aep/MapServer/4"),
    ("caribou_rpc", "https://geospatial.alberta.ca/titan/rest/services/biota/wildlife_sensitivity_mammals_10tm_nad83_aep/MapServer/170"),
    ("caribou_alp", "https://geospatial.alberta.ca/titan/rest/services/biota/wildlife_sensitivity_mammals_10tm_nad83_aep/MapServer/162"),
    ("grizzly_core", "https://geospatial.alberta.ca/titan/rest/services/biota/wildlife_sensitivity_mammals_10tm_nad83_aep/MapServer/184"),
    ("coal", "https://geospatial.alberta.ca/titan/rest/services/energy/mineral_agreement/MapServer/4"),
];
#[allow(dead_code)]
const _UNUSED: &str = AB_TITAN;

pub const LANDCOVER: &str =
    "https://datacube-prod-data-public.s3.ca-central-1.amazonaws.com/store/land/landcover/landcover-2020-classification.tif";

pub const SNOW_BASE: &str = "https://datacube-prod-data-public.s3.ca-central-1.amazonaws.com/store/land/snow-dynamics-yearly/";

/// Winters covered by the national snow dynamics product, as "first year, second year".
pub const SNOW_WINTERS: [&str; 7] = ["1819", "1920", "2021", "2122", "2223", "2324", "2425"];

/// Decimal year used for the magnetic declination printed on the maps.
pub const FIELD_YEAR: f64 = 2026.8;

/// The day the sun maps are computed for.
pub const SUN_DATE: (i32, u32, u32) = (2026, 10, 5);

/// Eye height of a person glassing, and the height of an animal's back, in metres.
pub const EYE_M: f32 = 1.7;
pub const ANIMAL_M: f32 = 1.0;

pub const TOWN: (f64, f64) = (-119.1186, 53.8881);
