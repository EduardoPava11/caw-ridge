// What the analysis knows about every 20 m of the ridge, read from images whose colour
// channels hold the numbers. Shared by the 3D view and the explorer.
(function () {
  "use strict";
  var M = null, grids = {};
  var DIR8 = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
  var DIR16 = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];
  var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

  function merc(lat) { return Math.log(Math.tan(Math.PI / 4 + lat * Math.PI / 360)); }
  function unmerc(y) { return (2 * Math.atan(Math.exp(y)) - Math.PI / 2) * 180 / Math.PI; }

  function load(name, spec) {
    return new Promise(function (resolve, reject) {
      var im = new Image();
      im.onload = function () {
        var c = document.createElement("canvas");
        c.width = im.width; c.height = im.height;
        var ctx = c.getContext("2d", { willReadFrequently: true });
        ctx.drawImage(im, 0, 0);
        grids[name] = { data: ctx.getImageData(0, 0, im.width, im.height).data, w: im.width, h: im.height, b: spec.bbox };
        resolve();
      };
      im.onerror = function () { reject(new Error("could not load " + spec.src)); };
      im.src = spec.src;
    });
  }

  // Fractional pixel position of a point in a grid, or null outside it.
  function at(g, lat, lon) {
    var b = g.b;
    var fx = (lon - b.w) / (b.e - b.w) * g.w, fy = (merc(b.n) - merc(lat)) / (merc(b.n) - merc(b.s)) * g.h;
    if (fx < 0 || fy < 0 || fx >= g.w || fy >= g.h) { return null; }
    return [fx, fy];
  }
  function px(g, x, y) {
    var i = (Math.min(g.h - 1, Math.max(0, y)) * g.w + Math.min(g.w - 1, Math.max(0, x))) * 4;
    return [g.data[i], g.data[i + 1], g.data[i + 2]];
  }
  function elevAt(g, x, y) { var p = px(g, x, y); return p[0] * 256 + p[1] + p[2] / 256 - 32768; }

  // Height by bilinear interpolation, from the finer grid where it reaches.
  function height(lat, lon) {
    var names = ["elev_close", "elev_ridge"];
    for (var k = 0; k < names.length; k++) {
      var g = grids[names[k]];
      if (!g) { continue; }
      var p = at(g, lat, lon);
      if (!p) { continue; }
      var fx = p[0] - 0.5, fy = p[1] - 0.5;
      var x0 = Math.floor(fx), y0 = Math.floor(fy), tx = fx - x0, ty = fy - y0;
      var a = elevAt(g, x0, y0), b = elevAt(g, x0 + 1, y0), c = elevAt(g, x0, y0 + 1), d = elevAt(g, x0 + 1, y0 + 1);
      return (a * (1 - tx) + b * tx) * (1 - ty) + (c * (1 - tx) + d * tx) * ty;
    }
    return null;
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
  function clock(h) { var m = Math.round(h * 60); return Math.floor(m / 60) + ":" + String(m % 60).padStart(2, "0"); }
  function dist(m) { return m < 1000 ? Math.round(m) + " m" : (m / 1000).toFixed(2) + " km"; }
  function num(v) { return Math.round(v).toLocaleString("en-CA"); }

  // Everything known about one point.
  function query(lat, lon) {
    var q = { lat: lat, lon: lon, height: height(lat, lon), utm: utm(lat, lon) };
    q.toWaypoint = haversine([lat, lon], M.waypoint);
    q.bearingToWaypoint = bearing([lat, lon], M.waypoint);
    var g = grids.a;
    var p = g && at(g, lat, lon);
    if (!p) { return q; }
    var x = Math.floor(p[0]), y = Math.floor(p[1]);
    var a = px(grids.a, x, y), b = px(grids.b, x, y), c = px(grids.c, x, y), d = px(grids.d, x, y);
    q.slope = a[0] / 2;
    q.aspect = a[1] === 255 ? null : a[1] * 1.5;
    q.cover = M.cover[String(a[2])] || null;
    q.open = M.open.indexOf(a[2]) >= 0;
    q.landform = M.landforms[b[0]] || null;
    q.firstSun = b[1] === 255 ? null : b[1] / 12 + 4;
    q.sunHours = b[2] / 16;
    q.snowDay = c[0] === 0 ? null : c[0];
    q.walkMinutes = c[1] === 255 ? null : c[1] * 3;
    q.inViewKnown = (c[2] & 2) !== 0;
    q.inView = (c[2] & 1) !== 0;
    q.glassKnown = (c[2] & 4) !== 0;
    q.glassCount = c[2] >> 4;
    q.shelter = d[0] === 0 ? null : (d[0] - 128) / 4;
    q.tpiSmall = (d[1] - 128) / 32;
    q.tpiLarge = (d[2] - 128) / 32;
    return q;
  }

  // The same, as rows of words for a panel.
  function describe(q) {
    var rows = [];
    function add(k, v, note) { rows.push({ k: k, v: v, note: note || "" }); }
    if (q.height !== null) { add("Height", num(q.height) + " m", num(q.height * 3.28084) + " ft"); }
    if (q.slope !== undefined) {
      var pc = Math.tan(q.slope * Math.PI / 180) * 100;
      var feel = q.slope < 5 ? "level" : q.slope < 10 ? "gentle" : q.slope < 20 ? "steady walking" : q.slope < 30 ? "steep and slow" : q.slope < 35 ? "hands may be needed" : q.slope < 40 ? "very steep" : "escape terrain";
      add("Slope", q.slope.toFixed(0) + "° (" + pc.toFixed(0) + "%)", feel);
      add("Faces", q.aspect === null ? "level ground" : DIR8[Math.round(q.aspect / 45) % 8], q.aspect === null ? "" : Math.round(q.aspect) + "° true");
      if (q.landform) { add("Landform", q.landform.name, q.landform.meaning); }
      if (q.cover) { add("Cover", q.cover, q.open ? "open ground" : "you cannot glass into it"); }
      if (q.shelter !== null) {
        var s = q.shelter;
        var w = s <= -10 ? "very exposed" : s <= -5 ? "exposed" : s <= -2 ? "somewhat exposed" : s < 2 ? "neither sheltered nor exposed" : s < 5 ? "somewhat sheltered" : s < 10 ? "sheltered" : "deep in the lee";
        add("In a " + M.windWords + " wind", w, s > 2 ? "snow drifts in here" : s < -2 ? "blows bare" : "");
      }
      add("First sun", q.firstSun === null ? "none that day" : clock(q.firstSun), "on " + M.sunDate + ", " + q.sunHours.toFixed(1) + " h of sun");
      if (q.snowDay !== null) {
        var day = new Date(Date.UTC(2026, 7, 31 + q.snowDay));
        add("First snow, usually", day.getUTCDate() + " " + MONTHS[day.getUTCMonth()], "median of recent winters");
      }
      if (q.walkMinutes !== null) {
        var wm = q.walkMinutes;
        add("Walk from waypoint", wm < 60 ? wm + " min" : Math.floor(wm / 60) + " h " + String(wm % 60).padStart(2, "0") + " min", "by slope alone, no pack");
      }
      if (q.inViewKnown) { add("From the waypoint", q.inView ? "in view" : "hidden"); }
      if (q.glassKnown) { add("Glassing points", q.glassCount === 0 ? "seen by none" : "seen by " + q.glassCount, q.glassCount === 0 ? "dead ground" : ""); }
    }
    if (q.toWaypoint > 1) {
      var mag = (q.bearingToWaypoint - M.declination + 360) % 360;
      add("To the waypoint", dist(q.toWaypoint), Math.round(q.bearingToWaypoint) + "° true (" + DIR16[Math.round(q.bearingToWaypoint / 22.5) % 16] + "), " + Math.round(mag) + "° magnetic");
    }
    add("Position", q.lat.toFixed(5) + ", " + q.lon.toFixed(5), q.utm.zone + "U " + String(Math.round(q.utm.e)).padStart(6, "0") + " E " + Math.round(q.utm.n) + " N");
    return rows;
  }

  // Build the rows into a panel element. Text only: nothing here is trusted as markup.
  function render(box, title, rows, buttons) {
    box.textContent = "";
    var h = document.createElement("h3"); h.textContent = title; box.appendChild(h);
    var dl = document.createElement("dl");
    rows.forEach(function (r) {
      var dt = document.createElement("dt"); dt.textContent = r.k;
      var dd = document.createElement("dd"); dd.textContent = r.v;
      if (r.note) { var s = document.createElement("small"); s.textContent = r.note; dd.appendChild(s); }
      dl.appendChild(dt); dl.appendChild(dd);
    });
    box.appendChild(dl);
    if (buttons && buttons.length) {
      var row = document.createElement("div"); row.className = "acts";
      buttons.forEach(function (b) {
        var el = document.createElement("button"); el.type = "button"; el.textContent = b.label;
        el.addEventListener("click", b.run);
        row.appendChild(el);
      });
      box.appendChild(row);
    }
  }

  // Heights along the straight line from a to b, and whether b can be seen from a.
  function profile(a, b, n) {
    var d = haversine(a, b), out = [];
    for (var i = 0; i <= n; i++) {
      var t = i / n;
      var lat = unmerc(merc(a[0]) + (merc(b[0]) - merc(a[0])) * t), lon = a[1] + (b[1] - a[1]) * t;
      out.push({ d: d * t, z: height(lat, lon), lat: lat, lon: lon });
    }
    return out;
  }
  function sight(a, b, eye, target) {
    var d = haversine(a, b), n = Math.max(8, Math.min(800, Math.round(d / 15)));
    var p = profile(a, b, n);
    if (p[0].z === null || p[n].z === null) { return null; }
    var R = 6371000, k = 0.87;
    var z0 = p[0].z + eye, z1 = p[n].z + target - d * d / (2 * R) * k;
    for (var i = 1; i < n; i++) {
      if (p[i].z === null) { continue; }
      var t = i / n, drop = p[i].d * p[i].d / (2 * R) * k;
      if (p[i].z - drop > z0 + (z1 - z0) * t) { return { clear: false, at: p[i] }; }
    }
    return { clear: true };
  }

  var ready = fetch("data/terrain/terrain.json").then(function (r) { return r.json(); }).then(function (m) {
    M = m;
    return Promise.all(Object.keys(m.grids).map(function (k) { return load(k, m.grids[k]); }));
  }).then(function () { return M; });

  window.CawTerrain = {
    ready: ready, query: query, describe: describe, render: render, height: height, profile: profile, sight: sight,
    haversine: haversine, bearing: bearing, utm: utm, merc: merc, unmerc: unmerc, dist: dist, num: num, clock: clock,
    DIR8: DIR8, DIR16: DIR16, manifest: function () { return M; },
    grid: function (name) { return grids[name]; },
    elevPixel: function (name, x, y) { return elevAt(grids[name], x, y); }
  };
})();
