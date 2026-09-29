// Offline support. Pages and assets are answered from the cache at once and refreshed in
// the background, so the site opens with no signal and still picks up new builds.
var CORE = "cawridge-core-__VERSION__";
var MAPS = "cawridge-maps-v1";
var FILES = __FILES__;

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
