// The explorer: this site's own maps laid over the ground, with your position on them.
(function () {
  "use strict";
  var WPT = window.CAW.waypoint, DECL = window.CAW.declination;

  var T = window.CawTerrain;
  var utm = T.utm, haversine = T.haversine, bearing = T.bearing, DIR16 = T.DIR16;

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
  var groups = { "Waypoint and glassing points": L.layerGroup(), "Saddles and summits": L.layerGroup(), "Roads, camps and services": L.layerGroup(), "Drive and walking line": L.layerGroup() };
  var PIN = { waypoint: "", glassing: "g", camp: "c", summit: "t", saddle: "p" };
  window.CAW.places.forEach(function (p) {
    var cls = PIN[p.kind] === undefined ? "s" : PIN[p.kind];
    var m = L.marker([p.lat, p.lon], { icon: icon(cls), title: p.name, zIndexOffset: p.kind === "waypoint" ? 1000 : 0 }).bindPopup(popup(p));
    var g = p.kind === "waypoint" || p.kind === "glassing" ? "Waypoint and glassing points" : (p.kind === "summit" || p.kind === "saddle" ? "Saddles and summits" : "Roads, camps and services");
    m.on("click", function () { describe(p.name, p.lat, p.lon); });
    m.addTo(groups[g]);
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
  groups["Saddles and summits"].addTo(map);
  groups["Roads, camps and services"].addTo(map);
  L.control.layers(bases, groups, { collapsed: window.innerWidth < 900, position: "topright" }).addTo(map);
  L.control.scale({ imperial: false, maxWidth: 160 }).addTo(map);
  map.fitBounds([[54.02, -119.47], [54.105, -119.31]]);

  // What is known about a point, in the panel at the bottom left.
  var box = document.querySelector("#info"), wrap = document.querySelector("#infowrap");
  document.querySelector("[data-close-info]").addEventListener("click", function () { wrap.hidden = true; });
  function describe(title, lat, lon, acc) {
    T.ready.then(function () {
      var rows = T.describe(T.query(lat, lon));
      if (acc) { rows.unshift({ k: "Accuracy", v: "within " + Math.round(acc) + " m", note: "from your device" }); }
      T.render(box, title, rows, [
        { label: "See it in 3D", run: function () { location.href = "terrain3d.html?lat=" + lat.toFixed(5) + "&lon=" + lon.toFixed(5); } },
        { label: "Stand here", run: function () { location.href = "terrain3d.html?stand=1&lat=" + lat.toFixed(5) + "&lon=" + lon.toFixed(5); } }
      ]);
      wrap.hidden = false;
    });
  }
  var tapped;
  map.on("click", function (e) {
    if (tapped) { tapped.remove(); }
    tapped = L.circleMarker(e.latlng, { radius: 6, color: "#1d1d1b", weight: 2, fillColor: "#fff", fillOpacity: 1 }).addTo(map);
    describe("Tapped point", e.latlng.lat, e.latlng.lng);
  });
  var asked = new URLSearchParams(location.search);
  if (Number(asked.get("lat")) && Number(asked.get("lon"))) {
    var ll = L.latLng(Number(asked.get("lat")), Number(asked.get("lon")));
    tapped = L.circleMarker(ll, { radius: 6, color: "#1d1d1b", weight: 2, fillColor: "#fff", fillOpacity: 1 }).addTo(map);
    map.setView(ll, 14);
    describe("Chosen point", ll.lat, ll.lng);
  } else {
    describe("Waypoint", WPT[0], WPT[1]);
  }

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
    T.render(box, "Position not available", [{ k: "Reason", v: e.message, note: "" }], []);
    wrap.hidden = false;
  });
})();
