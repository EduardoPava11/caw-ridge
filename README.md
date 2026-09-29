# Caw Ridge

A hunter's field guide to Caw Ridge, Alberta, as a static website. One Rust program
downloads open data, analyses the terrain, draws every map and writes every page.

The guide is built around two points. The **waypoint** (54.095806, -119.324028) is
the place to get to: the turnoff on Beaverdam Road where the old exploration road
leaves the gravel, at 1,285 m in the timber. The **ridge point** (54.062707,
-119.390728) is 8 km up that road, at 1,991 m in the alpine.

The site is published at https://eduardopava11.github.io/caw-ridge/ from the
`gh-pages` branch. The built site is not kept on `main`.

## What is in it

| Page | What it holds |
|---|---|
| Overview | The facts at a glance, the eight things to know first, the forecast |
| 3D | The ridge as a mesh you can turn, with any map draped over it. Stand on any point and look around, raise the glass, measure a shot and its line of sight, light it with the real sun, and tap anywhere to ask about the ground |
| Maps | 34 sheets at three scales: topographic, satellite, height, slope, aspect, landforms and saddles, ground cover, wind shelter, first sun, hours of sun, first snow, walking time, visibility, wildlife ranges, coal leases |
| Explore | Every sheet over the ground in one flat map, with GPS position. Tap anywhere for height, slope, aspect, landform, cover, shelter, sun, snow, walking time and bearings |
| Terrain | Glassing points ranked by the open ground they see, every saddle and summit with coordinates, landforms, height bands, slope and cover, wind shelter, sun and thermals, walking times |
| Access | The drive from Grande Cache leg by leg with its profile, road reports, camping, services, land status |
| Regulations | WMU 446 seasons for 2026, mountain rules, licences, registration, contacts |
| Wildlife | What lives on the ridge, what is protected, how to tell caribou from elk and grizzly from black bear |
| Weather | Live forecast, ten autumns of climate at ridge height, wind, first snow by winter, legal light for every day |
| Safety | Emergency numbers, bears and meat care, communications, a packing list |
| Sources | Every data source, how the maps are made, what is not known |

Also: a GPX file for a GPS, KMZ overlays for Google Earth, and offline support so the
whole guide works with no signal once saved to a phone.

## Build it

You need Rust (1.80 or newer). Nothing else: no GDAL, no Python, no Node.

```sh
cargo run --release            # fetch the data, then build the site
cargo run --release -- fetch   # download the data into data/
cargo run --release -- build   # draw the maps and write the site into docs/
cargo run --release -- pages   # rewrite the pages only, keeping the maps
cargo test --release           # projections, sun, calendar, magnetic model
```

A full build takes about a minute and a quarter and writes into `docs/`. To look at
the result:

```sh
cd docs && python3 -m http.server 8000
```

To build and publish in one step:

```sh
./deploy.sh          # fetch fresh data, build, publish
./deploy.sh build    # build from the data already on disk, publish
```

## Point it somewhere else

Everything about the place is in `src/config.rs`: the waypoint, the ridge point, the
three map boxes, the date for the sun maps. Change them, run `cargo run --release`, and the maps and
the numbers on the pages follow. The prose about regulations, wildlife and access in
`src/site.rs` is specific to Caw Ridge and WMU 446 and would need rewriting by hand.
The build stops with an error if the waypoint falls outside WMU 446, rather than
print the wrong rules.

## How it works

| File | Job |
|---|---|
| `src/cog.rs` | Reads windows out of Cloud Optimized GeoTIFFs over HTTP range requests |
| `src/geo.rs` | Lambert conformal conic, UTM and Web Mercator projections |
| `src/dem.rs` | The elevation window and its bicubic sampler |
| `src/sat.rs` | Finds the newest clear Sentinel-2 pass and mosaics its tiles |
| `src/vector.rs` | Roads, water, boundaries and names from three services |
| `src/view.rs` | Map views, slope, aspect, relief shading |
| `src/contour.rs` | Contour lines by marching squares |
| `src/analysis.rs` | Sight lines, sun exposure, walking time, drainage, landforms, summits and saddles, wind shelter |
| `src/route.rs` | Routing over the road network, elevation profiles |
| `src/sun.rs` | Sunrise, sunset, twilight, moon phase, the calendar |
| `src/wmm.rs` | Magnetic declination from the World Magnetic Model 2025 |
| `src/climate.rs` | Weather history and forecast |
| `src/draw.rs` | Rasters, the SVG builder, label placement |
| `src/maps.rs` | The map sheets: line work, lettering, the collar |
| `src/products.rs` | Runs the analysis and renders every map |
| `src/chart.rs` | Charts as inline SVG |
| `src/export.rs` | GPX, KML, KMZ, GeoJSON |
| `src/site.rs` | The pages |
| `assets/site/terrain-data.js` | Reads the terrain grids in the browser and answers "what is here?" |
| `assets/site/terrain3d.js` | The 3D view, on three.js |

The analysis is handed to the browser as PNG images in `docs/data/terrain/`, with the
numbers packed into the colour channels. `terrain.json` beside them says how to read
each one.

## Data and credit

Elevation, land cover and snow records from Natural Resources Canada. Boundaries,
wildlife ranges and coal agreements from the Government of Alberta. Roads, trails and
rivers from OpenStreetMap contributors. Imagery from Copernicus Sentinel-2. Weather
from Open-Meteo. Magnetic model from NOAA and the British Geological Survey. Full
list with licences in `docs/sources.html`.

`research/` holds the notes behind the regulations and access pages, with the source
of every statement.

## A warning

This is a planning aid. Hunting regulations change every year, boundaries on any map
are approximate, and roads wash out. The Alberta Guide to Hunting Regulations and the
Wildlife Regulation govern. Confirm before you hunt.
