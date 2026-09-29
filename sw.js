// Offline support. With a connection the network always wins, so a new build is never
// mixed with pieces of an old one. With no answer inside three seconds, or no
// connection at all, the saved copy is used.
var CORE = "cawridge-core-29-Sep-2026,-17-27-MDT";
var MAPS = "cawridge-maps-v2";
var FILES = ["./","index.html","maps.html","viewer.html","explore.html","terrain3d.html","terrain.html","access.html","regulations.html","wildlife.html","weather.html","safety.html","sources.html","assets/style.css","assets/app.js","assets/viewer.js","assets/explore.js","assets/terrain-data.js","assets/terrain3d.js","assets/icon.svg","assets/icon-180.png","vendor/leaflet/leaflet.js","vendor/leaflet/leaflet.css","vendor/three/three.module.min.js","vendor/three/three.core.min.js","vendor/three/OrbitControls.js","maps.json","offline.json","data/cawridge.geojson","data/terrain/terrain.json","data/terrain/elev_ridge.png","data/terrain/elev_close.png","data/terrain/a.png","data/terrain/b.png","data/terrain/c.png","data/terrain/d.png","maps/hero.jpg","manifest.webmanifest","assets/fonts/IBMPlexMono-Medium.woff2","assets/fonts/BarlowSemiCondensed-Medium.woff2","assets/fonts/Barlow-Regular.woff2","assets/fonts/BarlowCondensed-Bold.woff2","assets/fonts/BarlowSemiCondensed-SemiBold.woff2","assets/fonts/Barlow-Medium.woff2","assets/fonts/BarlowCondensed-Medium.woff2","assets/fonts/Barlow-Italic.woff2","assets/fonts/BarlowSemiCondensed-Regular.woff2","assets/fonts/BarlowCondensed-SemiBold.woff2","assets/fonts/Barlow-SemiBold.woff2","assets/fonts/IBMPlexMono-Regular.woff2","maps/thumb/region_topo.jpg","maps/mid/region_topo.jpg","maps/thumb/region_sat.jpg","maps/mid/region_sat.jpg","maps/thumb/region_wildlife.jpg","maps/mid/region_wildlife.jpg","maps/thumb/region_coal.jpg","maps/mid/region_coal.jpg","maps/thumb/ridge_topo.jpg","maps/mid/ridge_topo.jpg","maps/thumb/close_topo.jpg","maps/mid/close_topo.jpg","maps/thumb/ridge_sat.jpg","maps/mid/ridge_sat.jpg","maps/thumb/ridge_cir.jpg","maps/mid/ridge_cir.jpg","maps/thumb/ridge_slope.jpg","maps/mid/ridge_slope.jpg","maps/thumb/ridge_aspect.jpg","maps/mid/ridge_aspect.jpg","maps/thumb/ridge_cover.jpg","maps/mid/ridge_cover.jpg","maps/thumb/ridge_sunrise.jpg","maps/mid/ridge_sunrise.jpg","maps/thumb/ridge_sunhours.jpg","maps/mid/ridge_sunhours.jpg","maps/thumb/ridge_snow.jpg","maps/mid/ridge_snow.jpg","maps/thumb/ridge_walk.jpg","maps/mid/ridge_walk.jpg","maps/thumb/ridge_view.jpg","maps/mid/ridge_view.jpg","maps/thumb/ridge_glass.jpg","maps/mid/ridge_glass.jpg","maps/thumb/ridge_landform.jpg","maps/mid/ridge_landform.jpg","maps/thumb/ridge_shelter.jpg","maps/mid/ridge_shelter.jpg","maps/thumb/ridge_height.jpg","maps/mid/ridge_height.jpg","maps/thumb/close_sat.jpg","maps/mid/close_sat.jpg","maps/thumb/close_cir.jpg","maps/mid/close_cir.jpg","maps/thumb/close_slope.jpg","maps/mid/close_slope.jpg","maps/thumb/close_aspect.jpg","maps/mid/close_aspect.jpg","maps/thumb/close_cover.jpg","maps/mid/close_cover.jpg","maps/thumb/close_sunrise.jpg","maps/mid/close_sunrise.jpg","maps/thumb/close_sunhours.jpg","maps/mid/close_sunhours.jpg","maps/thumb/close_snow.jpg","maps/mid/close_snow.jpg","maps/thumb/close_walk.jpg","maps/mid/close_walk.jpg","maps/thumb/close_view.jpg","maps/mid/close_view.jpg","maps/thumb/close_glass.jpg","maps/mid/close_glass.jpg","maps/thumb/close_landform.jpg","maps/mid/close_landform.jpg","maps/thumb/close_shelter.jpg","maps/mid/close_shelter.jpg","maps/thumb/close_height.jpg","maps/mid/close_height.jpg"];

self.addEventListener("install", function (e) {
  e.waitUntil(caches.open(CORE).then(function (c) { return c.addAll(FILES); }).then(function () { return self.skipWaiting(); }));
});
self.addEventListener("activate", function (e) {
  e.waitUntil(caches.keys().then(function (keys) {
    return Promise.all(keys.filter(function (k) { return k !== CORE && k !== MAPS; }).map(function (k) { return caches.delete(k); }));
  }).then(function () { return self.clients.claim(); }));
});
self.addEventListener("fetch", function (e) {
  var req = e.request;
  if (req.method !== "GET") { return; }
  var url = new URL(req.url);
  if (url.origin !== location.origin) { return; }
  var big = /\/maps\//.test(url.pathname) || /\.(kmz|gpx|kml)$/.test(url.pathname);
  var page = req.mode === "navigate";
  // The address without its version stamp is the key, so each file is kept once.
  var key = url.origin + url.pathname;
  e.respondWith(new Promise(function (resolve) {
    var settled = false;
    function saved() {
      return caches.match(key, { ignoreSearch: true }).then(function (hit) { return hit || (page ? caches.match("index.html") : undefined); });
    }
    var timer = setTimeout(function () {
      saved().then(function (hit) { if (hit && !settled) { settled = true; resolve(hit); } });
    }, 3000);
    fetch(req).then(function (res) {
      clearTimeout(timer);
      if (res && res.ok && res.type === "basic") {
        var copy = res.clone();
        caches.open(big ? MAPS : CORE).then(function (c) { c.put(key, copy); });
      }
      if (!settled) { settled = true; resolve(res); }
    }).catch(function () {
      clearTimeout(timer);
      saved().then(function (hit) { if (!settled) { settled = true; resolve(hit || Response.error()); } });
    });
  }));
});
