// The explorer: this site's own maps laid over the ground, with your position on them.
(function () {
  "use strict";
  var WPT = window.CAW.waypoint, DECL = window.CAW.declination;

  // Transverse Mercator (Krueger series to the third order), enough for a metre.
  function utm(lat, lon) {
    var a = 6378137, f = 1 / 298.257222101, k0 = 0.9996;
    var zone = Math.floor((lon + 180) / 6) + 1;
    var lon0 = (zone * 6 - 183) * Math.PI / 180;
    var n = f / (2 - f), n2 = n * n, n3 = n2 * n;
    var A = a / (1 + n) * (1 + n2 / 4 + n2 * n2 / 64);
    var al = [n / 2 - 2 / 3 * n2 + 5 / 16 * n3, 13 / 48 * n2 - 3 / 5 * n3, 61 / 240 * n3];
    var e = Math.sqrt(2 * f - f * f);
    var p = lat * Math.PI / 180, l = lon * Math.PI / 180 - lon0;
    var tau = Math.tan(p);
    var sig = Math.sinh(e * Math.atanh(e * tau / Math.sqrt(1 + tau * tau)));
    var taup = tau * Math.sqrt(1 + sig * sig) - sig * Math.sqrt(1 + tau * tau);
    var xi = Math.atan2(taup, Math.cos(l));
    var eta = Math.asinh(Math.sin(l) / Math.sqrt(taup * taup + Math.cos(l) * Math.cos(l)));
    var x = eta, y = xi;
    for (var j = 0; j < 3; j++) {
      var k = 2 * (j + 1);
      y += al[j] * Math.sin(k * xi) * Math.cosh(k * eta);
      x += al[j] * Math.cos(k * xi) * Math.sinh(k * eta);
    }
    return { zone: zone, e: 500000 + k0 * A * x, n: k0 * A * y };
  }
  function haversine(a, b) {
    var R = 6371008.8, r = Math.PI / 180;
    var dp = (b[0] - a[0]) * r, dl = (b[1] - a[1]) * r;
    var h = Math.sin(dp / 2) * Math.sin(dp / 2) + Math.cos(a[0] * r) * Math.cos(b[0] * r) * Math.sin(dl / 2) * Math.sin(dl / 2);
    return 2 * R * Math.asin(Math.sqrt(h));
  }
  function bearing(a, b) {
    var r = Math.PI / 180;
    var y = Math.sin((b[1] - a[1]) * r) * Math.cos(b[0] * r);
    var x = Math.cos(a[0] * r) * Math.sin(b[0] * r) - Math.sin(a[0] * r) * Math.cos(b[0] * r) * Math.cos((b[1] - a[1]) * r);
    return (Math.atan2(y, x) / r + 360) % 360;
  }
  var DIR16 = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];

  var map = L.map("map", { zoomControl: true, minZoom: 9, maxZoom: 17, zoomSnap: 0.25, maxBounds: [[53.3, -120.8], [54.8, -118.0]] });
  map.attributionControl.setPrefix(false);
  var own = "Maps by this site from NRCan, Alberta, OpenStreetMap and Copernicus data";

  function stack(ids) {
    var g = L.layerGroup();
    ids.forEach(function (id) {
      var m = window.CAW.maps[id];
      if (m) { L.imageOverlay(m.bare, [[m.s, m.w], [m.n, m.e]], { attribution: own, interactive: false }).addTo(g); }
    });
    return g;
  }
  var bases = {};
  window.CAW.layers.forEach(function (l) { bases[l.name] = stack(l.ids); });
  bases["Online: OpenTopoMap"] = L.tileLayer("https://{s}.tile.opentopomap.org/{z}/{x}/{y}.png", { maxZoom: 17, attribution: "Map data OpenStreetMap contributors, SRTM. Style OpenTopoMap (CC-BY-SA)" });
  bases["Online: Esri imagery"] = L.tileLayer("https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}", { maxZoom: 17, attribution: "Imagery Esri, Maxar, Earthstar Geographics" });
  bases["Online: OpenStreetMap"] = L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", { maxZoom: 17, attribution: "OpenStreetMap contributors" });
  var first = window.CAW.layers[0].name;
  var wanted = new URLSearchParams(location.search).get("layer");
  var start = first;
  window.CAW.layers.forEach(function (l) { if (l.ids.indexOf(wanted) >= 0) { start = l.name; } });
  bases[start].addTo(map);

  function icon(cls) { return L.divIcon({ className: "", html: '<div class="pin ' + cls + '"></div>', iconSize: [22, 22], iconAnchor: [11, 11], popupAnchor: [0, -10] }); }
  function popup(p) {
    var u = utm(p.lat, p.lon);
    var box = document.createElement("div");
    var b = document.createElement("b"); b.textContent = p.name; box.appendChild(b);
    if (p.note) { var n = document.createElement("div"); n.textContent = p.note; box.appendChild(n); }
    var c = document.createElement("span"); c.className = "mono";
    c.textContent = p.lat.toFixed(5) + ", " + p.lon.toFixed(5) + "  |  " + u.zone + "U " + Math.round(u.e) + " E " + Math.round(u.n) + " N  |  " + Math.round(p.elev) + " m";
    box.appendChild(c);
    return box;
  }
  var groups = { "Waypoint and glassing points": L.layerGroup(), "Roads, camps and services": L.layerGroup(), "Drive and walking line": L.layerGroup() };
  window.CAW.places.forEach(function (p) {
    var cls = p.kind === "waypoint" ? "" : (p.kind === "glassing" ? "g" : (p.kind === "camp" ? "c" : "s"));
    var m = L.marker([p.lat, p.lon], { icon: icon(cls), title: p.name, zIndexOffset: p.kind === "waypoint" ? 1000 : 0 }).bindPopup(popup(p));
    m.addTo(p.kind === "waypoint" || p.kind === "glassing" ? groups["Waypoint and glassing points"] : groups["Roads, camps and services"]);
  });
  fetch("data/cawridge.geojson").then(function (r) { return r.json(); }).then(function (g) {
    L.geoJSON(g, {
      filter: function (f) { return f.geometry.type === "LineString"; },
      style: function (f) {
        var k = f.properties.kind;
        if (k === "walk") { return { color: "#d7261e", weight: 3, dashArray: "2 7", lineCap: "round" }; }
        return { color: "#e0591b", weight: 4, opacity: 0.85 };
      },
      onEachFeature: function (f, layer) {
        var t = f.properties.name + (f.properties.km ? ", " + f.properties.km + " km" : "");
        layer.bindPopup(document.createTextNode(t));
      }
    }).addTo(groups["Drive and walking line"]);
  });
  groups["Waypoint and glassing points"].addTo(map);
  groups["Roads, camps and services"].addTo(map);
  L.control.layers(bases, groups, { collapsed: window.innerWidth < 900, position: "topright" }).addTo(map);
  L.control.scale({ imperial: false, maxWidth: 160 }).addTo(map);
  map.fitBounds([[54.02, -119.47], [54.105, -119.31]]);

  // Heights, read from a small image of the elevation model.
  var elev = null;
  (function () {
    var e = window.CAW.elev, im = new Image();
    im.onload = function () {
      var c = document.createElement("canvas"); c.width = im.width; c.height = im.height;
      var ctx = c.getContext("2d", { willReadFrequently: true }); ctx.drawImage(im, 0, 0);
      elev = { ctx: ctx, w: im.width, h: im.height, b: e };
    };
    im.src = e.src;
  })();
  function merc(lat) { return Math.log(Math.tan(Math.PI / 4 + lat * Math.PI / 360)); }
  function height(lat, lon) {
    if (!elev) { return null; }
    var b = elev.b;
    var fx = (lon - b.w) / (b.e - b.w), fy = (merc(b.n) - merc(lat)) / (merc(b.n) - merc(b.s));
    if (fx < 0 || fy < 0 || fx >= 1 || fy >= 1) { return null; }
    var d = elev.ctx.getImageData(Math.floor(fx * elev.w), Math.floor(fy * elev.h), 1, 1).data;
    return d[0] * 256 + d[1] + d[2] / 256 - 32768;
  }

  var readout = document.querySelector(".readout");
  function describe(title, lat, lon, acc) {
    var u = utm(lat, lon);
    var d = haversine([lat, lon], WPT), b = bearing([lat, lon], WPT);
    var mag = (b - DECL + 360) % 360;
    var h = height(lat, lon);
    readout.textContent = "";
    var t = document.createElement("b"); t.textContent = title; readout.appendChild(t);
    var lines = [
      lat.toFixed(5) + ", " + lon.toFixed(5) + (acc ? "  (within " + Math.round(acc) + " m)" : ""),
      u.zone + "U " + String(Math.round(u.e)).padStart(6, "0") + " E " + Math.round(u.n) + " N",
      "To waypoint: " + (d < 1000 ? Math.round(d) + " m" : (d / 1000).toFixed(2) + " km") + " at " + Math.round(b) + "° true (" + DIR16[Math.round(b / 22.5) % 16] + "), " + Math.round(mag) + "° magnetic"
    ];
    // At the waypoint itself a bearing means nothing.
    if (d < 1) { lines.pop(); }
    if (h !== null) { lines.splice(2, 0, "Height " + Math.round(h).toLocaleString("en-CA") + " m"); }
    lines.forEach(function (s) { var e = document.createElement("div"); e.textContent = s; readout.appendChild(e); });
  }
  var tapped;
  map.on("click", function (e) {
    if (tapped) { tapped.remove(); }
    tapped = L.circleMarker(e.latlng, { radius: 6, color: "#1d1d1b", weight: 2, fillColor: "#fff", fillOpacity: 1 }).addTo(map);
    describe("Tapped point", e.latlng.lat, e.latlng.lng);
  });
  describe("Waypoint", WPT[0], WPT[1]);

  // Your position.
  var me, ring, following = false;
  var Locate = L.Control.extend({
    options: { position: "topleft" },
    onAdd: function () {
      var d = L.DomUtil.create("div", "leaflet-bar");
      var a = L.DomUtil.create("a", "", d);
      a.href = "#"; a.title = "Show my position"; a.setAttribute("role", "button"); a.setAttribute("aria-label", "Show my position");
      a.textContent = "◎"; a.style.fontSize = "20px";
      L.DomEvent.on(a, "click", function (ev) {
        L.DomEvent.stop(ev);
        following = true;
        map.locate({ watch: true, enableHighAccuracy: true, maximumAge: 5000 });
      });
      return d;
    }
  });
  map.addControl(new Locate());
  map.on("locationfound", function (e) {
    if (!me) {
      me = L.marker(e.latlng, { icon: icon("me"), zIndexOffset: 2000 }).addTo(map);
      ring = L.circle(e.latlng, { radius: e.accuracy, color: "#2a78d6", weight: 1, fillOpacity: 0.12 }).addTo(map);
    }
    me.setLatLng(e.latlng); ring.setLatLng(e.latlng); ring.setRadius(e.accuracy);
    if (following) { map.setView(e.latlng, Math.max(map.getZoom(), 14)); following = false; }
    describe("Your position", e.latlng.lat, e.latlng.lng, e.accuracy);
  });
  map.on("locationerror", function (e) {
    readout.textContent = "";
    var t = document.createElement("b"); t.textContent = "Position not available"; readout.appendChild(t);
    var m = document.createElement("div"); m.textContent = e.message; readout.appendChild(m);
  });
})();
