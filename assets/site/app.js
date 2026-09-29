// Shared behaviour: theme, chart hover, checklists, the live forecast, offline saving.
(function () {
  "use strict";

  // Theme. The toggle wins over the system setting in both directions.
  var root = document.documentElement;
  var key = "cawridge-theme";
  function stored() { try { return localStorage.getItem(key); } catch (e) { return null; } }
  function apply(t) { if (t) { root.setAttribute("data-theme", t); } else { root.removeAttribute("data-theme"); } }
  apply(stored());
  function isDark() {
    var t = root.getAttribute("data-theme");
    if (t) { return t === "dark"; }
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
  }
  document.addEventListener("click", function (e) {
    var b = e.target.closest && e.target.closest(".theme");
    if (!b) { return; }
    var next = isDark() ? "light" : "dark";
    apply(next);
    try { localStorage.setItem(key, next); } catch (err) { /* private mode */ }
    label();
  });
  function label() {
    var b = document.querySelector(".theme");
    if (b) { b.textContent = isDark() ? "Day" : "Night"; b.setAttribute("aria-label", isDark() ? "Switch to the day theme" : "Switch to the night theme"); }
  }

  // One tooltip for every chart.
  var tip;
  function showTip(lines, x, y) {
    if (!tip) { tip = document.createElement("div"); tip.className = "tip"; tip.setAttribute("role", "status"); document.body.appendChild(tip); }
    tip.textContent = "";
    lines.forEach(function (l, i) {
      var el = document.createElement(i === 0 ? "b" : "span");
      el.textContent = l;
      tip.appendChild(el);
    });
    tip.classList.add("on");
    var r = tip.getBoundingClientRect();
    var px = Math.min(window.innerWidth - r.width - 8, Math.max(8, x + 14));
    var py = y - r.height - 14;
    if (py < 8) { py = y + 18; }
    tip.style.left = px + "px";
    tip.style.top = py + "px";
  }
  function hideTip() { if (tip) { tip.classList.remove("on"); } }

  function wireCharts() {
    document.querySelectorAll(".chart .mark").forEach(function (m) {
      var lines = (m.getAttribute("data-tip") || "").split("|");
      // Values lead, labels follow.
      var ordered = lines.length > 1 ? lines.slice(1, 2).concat(lines.slice(0, 1), lines.slice(2)) : lines;
      m.addEventListener("pointermove", function (e) { showTip(ordered, e.clientX, e.clientY); });
      m.addEventListener("pointerleave", hideTip);
      m.addEventListener("focus", function () { var r = m.getBoundingClientRect(); showTip(ordered, r.left + r.width / 2, r.top + 20); });
      m.addEventListener("blur", hideTip);
    });
    document.querySelectorAll(".chart .hover").forEach(function (g) {
      var svg = g.ownerSVGElement;
      var pts = (g.getAttribute("data-points") || "").split(";").filter(Boolean).map(function (s) { return s.split(",").map(Number); });
      var xu = g.getAttribute("data-x-unit"), yu = g.getAttribute("data-y-unit");
      var cross = g.querySelector(".cross"), dot = g.querySelector(".dot"), hit = g.querySelector(".hit");
      var idx = 0;
      function at(i, cx, cy) {
        idx = Math.max(0, Math.min(pts.length - 1, i));
        var p = pts[idx];
        cross.setAttribute("x1", p[0]); cross.setAttribute("x2", p[0]);
        dot.setAttribute("cx", p[0]); dot.setAttribute("cy", p[1]);
        g.classList.add("on");
        var box = svg.getBoundingClientRect();
        var sx = box.width / svg.viewBox.baseVal.width;
        showTip([Math.round(p[3]).toLocaleString("en-CA") + " " + yu, p[2].toFixed(1) + " " + xu],
          cx === undefined ? box.left + p[0] * sx : cx, cy === undefined ? box.top + p[1] * sx : cy);
      }
      hit.addEventListener("pointermove", function (e) {
        var box = svg.getBoundingClientRect();
        var x = (e.clientX - box.left) / box.width * svg.viewBox.baseVal.width;
        var best = 0, bd = 1e9;
        for (var i = 0; i < pts.length; i++) { var d = Math.abs(pts[i][0] - x); if (d < bd) { bd = d; best = i; } }
        at(best, e.clientX, e.clientY);
      });
      hit.addEventListener("pointerleave", function () { g.classList.remove("on"); hideTip(); });
      hit.addEventListener("focus", function () { at(idx); });
      hit.addEventListener("blur", function () { g.classList.remove("on"); hideTip(); });
      hit.addEventListener("keydown", function (e) {
        var step = Math.max(1, Math.round(pts.length / 60));
        if (e.key === "ArrowRight") { at(idx + step); e.preventDefault(); }
        if (e.key === "ArrowLeft") { at(idx - step); e.preventDefault(); }
      });
    });
  }

  // Checklists remember their ticks on this device.
  function wireChecks() {
    document.querySelectorAll(".check input[type=checkbox]").forEach(function (c) {
      var k = "cawridge-check-" + c.id;
      try { c.checked = localStorage.getItem(k) === "1"; } catch (e) { /* ignore */ }
      c.addEventListener("change", function () { try { localStorage.setItem(k, c.checked ? "1" : "0"); } catch (e) { /* ignore */ } });
    });
    var reset = document.querySelector("[data-reset-checks]");
    if (reset) {
      reset.addEventListener("click", function () {
        document.querySelectorAll(".check input[type=checkbox]").forEach(function (c) { c.checked = false; try { localStorage.removeItem("cawridge-check-" + c.id); } catch (e) { /* ignore */ } });
      });
    }
  }

  // Live forecast. The page already carries the forecast from build time; this replaces it
  // with a fresh one when there is a connection.
  var WORDS = { 0: "Clear", 1: "Mostly clear", 2: "Partly cloudy", 3: "Overcast", 45: "Fog", 48: "Fog", 51: "Drizzle", 53: "Drizzle", 55: "Drizzle", 56: "Freezing drizzle", 57: "Freezing drizzle", 61: "Light rain", 63: "Rain", 65: "Heavy rain", 66: "Freezing rain", 67: "Freezing rain", 71: "Light snow", 73: "Snow", 75: "Heavy snow", 77: "Snow grains", 80: "Rain showers", 81: "Rain showers", 82: "Rain showers", 85: "Snow showers", 86: "Snow showers", 95: "Thunderstorm", 96: "Thunderstorm, hail", 99: "Thunderstorm, hail" };
  var DIRS = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
  var DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  var MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  function el(tag, cls, text) { var e = document.createElement(tag); if (cls) { e.className = cls; } if (text !== undefined) { e.textContent = text; } return e; }
  function liveForecast() {
    if (!window.fetch) { return; }
    document.querySelectorAll("[data-forecast]").forEach(function (box) {
      fetch(box.getAttribute("data-forecast")).then(function (r) { return r.json(); }).then(function (j) {
        var d = j.daily;
        if (!d || !d.time) { return; }
        box.textContent = "";
        d.time.forEach(function (t, i) {
          var p = t.split("-").map(Number);
          var date = new Date(Date.UTC(p[0], p[1] - 1, p[2]));
          var cell = el("div");
          cell.appendChild(el("div", "d", DAYS[date.getUTCDay()] + " " + p[2] + " " + MONTHS[p[1] - 1]));
          var temp = el("div", "t", Math.round(d.temperature_2m_max[i]) + "\u00b0 ");
          temp.appendChild(el("i", "", Math.round(d.temperature_2m_min[i]) + "\u00b0"));
          cell.appendChild(temp);
          cell.appendChild(el("div", "w", WORDS[d.weather_code[i]] || "Mixed"));
          var snow = d.snowfall_sum[i], rain = d.precipitation_sum[i];
          cell.appendChild(el("div", "s", snow >= 0.1 ? snow.toFixed(1) + " cm snow" : (rain >= 0.1 ? rain.toFixed(1) + " mm rain" : "Dry")));
          cell.appendChild(el("div", "w", DIRS[Math.round(d.wind_direction_10m_dominant[i] / 45) % 8] + " " + Math.round(d.wind_speed_10m_max[i]) + ", gusts " + Math.round(d.wind_gusts_10m_max[i])));
          box.appendChild(cell);
        });
        var stamp = box.nextElementSibling;
        if (stamp && stamp.hasAttribute("data-forecast-stamp")) { stamp.textContent = "Live forecast, fetched just now on this device."; }
      }).catch(function () { /* offline: the build time forecast stays */ });
    });
  }

  // Offline: register the worker, and let a button pull every map into the cache.
  function offline() {
    if (!("serviceWorker" in navigator)) { return; }
    navigator.serviceWorker.register("sw.js").catch(function () { /* file:// or blocked */ });
    var b = document.querySelector("[data-save-offline]");
    if (!b) { return; }
    var out = document.querySelector("[data-save-status]");
    b.addEventListener("click", function () {
      b.disabled = true;
      fetch("offline.json").then(function (r) { return r.json(); }).then(function (list) {
        var done = 0, failed = 0;
        function say() { if (out) { out.textContent = "Saved " + done + " of " + list.length + " files" + (failed ? ", " + failed + " failed" : "") + "."; } }
        return caches.open("cawridge-maps-v1").then(function (c) {
          var chain = Promise.resolve();
          list.forEach(function (u) {
            chain = chain.then(function () {
              return fetch(u).then(function (r) { if (r.ok) { done++; return c.put(u, r); } failed++; }).catch(function () { failed++; }).then(say);
            });
          });
          return chain;
        });
      }).then(function () {
        b.disabled = false;
        if (out) { out.textContent += " The site now works with no signal on this device."; }
      }).catch(function () { b.disabled = false; if (out) { out.textContent = "Could not save. Try again with a connection."; } });
    });
  }

  function ready(f) { if (document.readyState !== "loading") { f(); } else { document.addEventListener("DOMContentLoaded", f); } }
  ready(function () { label(); wireCharts(); wireChecks(); liveForecast(); offline(); });
})();
// Mark today's row in the sun table.
(function () {
  "use strict";
  function mark() {
    var now = new Date();
    var id = now.getFullYear() + "-" + String(now.getMonth() + 1).padStart(2, "0") + "-" + String(now.getDate()).padStart(2, "0");
    var row = document.querySelector('tr[data-date="' + id + '"]');
    if (row) { row.classList.add("today"); }
  }
  if (document.readyState !== "loading") { mark(); } else { document.addEventListener("DOMContentLoaded", mark); }
})();
