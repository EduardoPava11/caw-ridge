// A plain pan and zoom viewer for one large map image.
(function () {
  "use strict";
  var stage = document.querySelector(".stage");
  var img = stage.querySelector("img");
  var id = new URLSearchParams(location.search).get("m") || "ridge_topo";
  var s = 1, x = 0, y = 0, min = 0.05, natW = 1, natH = 1;

  function place() { img.style.transform = "translate(" + x + "px," + y + "px) scale(" + s + ")"; }
  function fit() {
    var r = stage.getBoundingClientRect();
    s = Math.min(r.width / natW, r.height / natH);
    min = s * 0.6;
    x = (r.width - natW * s) / 2;
    y = (r.height - natH * s) / 2;
    place();
  }
  function zoomAt(f, cx, cy) {
    var r = stage.getBoundingClientRect();
    var px = cx - r.left, py = cy - r.top;
    var n = Math.max(min, Math.min(4, s * f));
    x = px - (px - x) * (n / s);
    y = py - (py - y) * (n / s);
    s = n;
    place();
  }

  fetch("maps.json", { cache: "no-cache" }).then(function (r) { return r.json(); }).then(function (maps) {
    var m = maps.filter(function (k) { return k.id === id; })[0] || maps[0];
    document.title = m.title + " | Caw Ridge";
    document.querySelector("[data-title]").textContent = m.title;
    document.querySelector("[data-blurb]").textContent = m.blurb;
    var dl = document.querySelector("[data-download]");
    dl.setAttribute("href", m.download);
    dl.setAttribute("download", "");
    img.alt = m.title;
    img.onload = function () { natW = img.naturalWidth; natH = img.naturalHeight; fit(); };
    img.src = m.view;
  });

  var pointers = new Map(), lastDist = 0;
  stage.addEventListener("pointerdown", function (e) {
    stage.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    stage.classList.add("drag");
    lastDist = 0;
  });
  stage.addEventListener("pointermove", function (e) {
    if (!pointers.has(e.pointerId)) { return; }
    var p = pointers.get(e.pointerId);
    if (pointers.size === 1) {
      x += e.clientX - p.x; y += e.clientY - p.y; place();
    }
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {
      var a = Array.from(pointers.values());
      var d = Math.hypot(a[0].x - a[1].x, a[0].y - a[1].y);
      if (lastDist) { zoomAt(d / lastDist, (a[0].x + a[1].x) / 2, (a[0].y + a[1].y) / 2); }
      lastDist = d;
    }
  });
  function up(e) { pointers.delete(e.pointerId); lastDist = 0; if (!pointers.size) { stage.classList.remove("drag"); } }
  stage.addEventListener("pointerup", up);
  stage.addEventListener("pointercancel", up);
  stage.addEventListener("wheel", function (e) { e.preventDefault(); zoomAt(Math.exp(-e.deltaY * 0.0016), e.clientX, e.clientY); }, { passive: false });
  stage.addEventListener("dblclick", function (e) { zoomAt(1.8, e.clientX, e.clientY); });
  window.addEventListener("resize", fit);
  document.addEventListener("keydown", function (e) {
    var r = stage.getBoundingClientRect();
    if (e.key === "+" || e.key === "=") { zoomAt(1.4, r.left + r.width / 2, r.top + r.height / 2); }
    if (e.key === "-") { zoomAt(1 / 1.4, r.left + r.width / 2, r.top + r.height / 2); }
    if (e.key === "0") { fit(); }
    var step = 80;
    if (e.key === "ArrowLeft") { x += step; place(); }
    if (e.key === "ArrowRight") { x -= step; place(); }
    if (e.key === "ArrowUp") { y += step; place(); }
    if (e.key === "ArrowDown") { y -= step; place(); }
  });
  document.querySelector("[data-in]").addEventListener("click", function () { var r = stage.getBoundingClientRect(); zoomAt(1.5, r.left + r.width / 2, r.top + r.height / 2); });
  document.querySelector("[data-out]").addEventListener("click", function () { var r = stage.getBoundingClientRect(); zoomAt(1 / 1.5, r.left + r.width / 2, r.top + r.height / 2); });
  document.querySelector("[data-fit]").addEventListener("click", fit);
})();
