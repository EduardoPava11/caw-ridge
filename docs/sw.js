// Offline support. Pages and assets are answered from the cache at once and refreshed in
// the background, so the site opens with no signal and still picks up new builds.
var CORE = "cawridge-core-28-Sep-2026,-22-36-MDT";
var MAPS = "cawridge-maps-v1";
var FILES = ["./","index.html","maps.html","viewer.html","explore.html","terrain.html","access.html","regulations.html","wildlife.html","weather.html","safety.html","sources.html","assets/style.css","assets/app.js","assets/viewer.js","assets/explore.js","assets/icon.svg","assets/icon-180.png","vendor/leaflet/leaflet.js","vendor/leaflet/leaflet.css","maps.json","offline.json","data/cawridge.geojson","data/elev_ridge.png","maps/hero.jpg","manifest.webmanifest","assets/fonts/IBMPlexMono-Medium.woff2","assets/fonts/BarlowSemiCondensed-Medium.woff2","assets/fonts/Barlow-Regular.woff2","assets/fonts/BarlowCondensed-Bold.woff2","assets/fonts/BarlowSemiCondensed-SemiBold.woff2","assets/fonts/Barlow-Medium.woff2","assets/fonts/BarlowCondensed-Medium.woff2","assets/fonts/Barlow-Italic.woff2","assets/fonts/BarlowSemiCondensed-Regular.woff2","assets/fonts/BarlowCondensed-SemiBold.woff2","assets/fonts/Barlow-SemiBold.woff2","assets/fonts/IBMPlexMono-Regular.woff2","maps/thumb/region_topo.jpg","maps/mid/region_topo.jpg","maps/thumb/region_sat.jpg","maps/mid/region_sat.jpg","maps/thumb/region_wildlife.jpg","maps/mid/region_wildlife.jpg","maps/thumb/region_coal.jpg","maps/mid/region_coal.jpg","maps/thumb/ridge_topo.jpg","maps/mid/ridge_topo.jpg","maps/thumb/close_topo.jpg","maps/mid/close_topo.jpg","maps/thumb/ridge_sat.jpg","maps/mid/ridge_sat.jpg","maps/thumb/ridge_cir.jpg","maps/mid/ridge_cir.jpg","maps/thumb/ridge_slope.jpg","maps/mid/ridge_slope.jpg","maps/thumb/ridge_aspect.jpg","maps/mid/ridge_aspect.jpg","maps/thumb/ridge_cover.jpg","maps/mid/ridge_cover.jpg","maps/thumb/ridge_sunrise.jpg","maps/mid/ridge_sunrise.jpg","maps/thumb/ridge_sunhours.jpg","maps/mid/ridge_sunhours.jpg","maps/thumb/ridge_snow.jpg","maps/mid/ridge_snow.jpg","maps/thumb/ridge_walk.jpg","maps/mid/ridge_walk.jpg","maps/thumb/ridge_view.jpg","maps/mid/ridge_view.jpg","maps/thumb/ridge_glass.jpg","maps/mid/ridge_glass.jpg","maps/thumb/close_sat.jpg","maps/mid/close_sat.jpg","maps/thumb/close_cir.jpg","maps/mid/close_cir.jpg","maps/thumb/close_slope.jpg","maps/mid/close_slope.jpg","maps/thumb/close_aspect.jpg","maps/mid/close_aspect.jpg","maps/thumb/close_cover.jpg","maps/mid/close_cover.jpg","maps/thumb/close_sunrise.jpg","maps/mid/close_sunrise.jpg","maps/thumb/close_sunhours.jpg","maps/mid/close_sunhours.jpg","maps/thumb/close_snow.jpg","maps/mid/close_snow.jpg","maps/thumb/close_walk.jpg","maps/mid/close_walk.jpg","maps/thumb/close_view.jpg","maps/mid/close_view.jpg","maps/thumb/close_glass.jpg","maps/mid/close_glass.jpg"];

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
  var page = req.mode === "navigate" || /\.(html|json)$/.test(url.pathname) || url.pathname.endsWith("/");
  if (page) {
    // Pages: the network first, so a new build shows at once; the cache if there is no
    // answer within three seconds.
    e.respondWith(new Promise(function (resolve) {
      var settled = false;
      function fromCache() {
        return caches.match(req, { ignoreSearch: true }).then(function (hit) { return hit || caches.match("index.html"); });
      }
      var timer = setTimeout(function () {
        fromCache().then(function (hit) { if (hit && !settled) { settled = true; resolve(hit); } });
      }, 3000);
      fetch(req).then(function (res) {
        clearTimeout(timer);
        if (res && res.ok && res.type === "basic") {
          var copy = res.clone();
          caches.open(CORE).then(function (c) { c.put(req, copy); });
        }
        if (!settled) { settled = true; resolve(res); }
      }).catch(function () {
        clearTimeout(timer);
        fromCache().then(function (hit) { if (!settled) { settled = true; resolve(hit || Response.error()); } });
      });
    }));
    return;
  }
  e.respondWith(caches.match(req, { ignoreSearch: url.pathname.endsWith(".html") }).then(function (hit) {
    var net = fetch(req).then(function (res) {
      if (res && res.ok && res.type === "basic") {
        var copy = res.clone();
        // Large files are only kept once the reader has asked to save them, or has opened them.
        caches.open(big ? MAPS : CORE).then(function (c) { c.put(req, copy); });
      }
      return res;
    }).catch(function () { return hit || caches.match("index.html"); });
    return hit || net;
  }));
});
