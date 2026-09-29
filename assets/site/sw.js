// Offline support. With a connection the network always wins, so a new build is never
// mixed with pieces of an old one. With no answer inside three seconds, or no
// connection at all, the saved copy is used.
var CORE = "cawridge-core-__VERSION__";
var MAPS = "cawridge-maps-v2";
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
