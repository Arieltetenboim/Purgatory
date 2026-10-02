/* Shared expected-drop chart. Item Lab and Mob Lab both call these functions.
   A future runtime simulator must not copy this formula; it calls the server resolver. */
(function (root, factory) {
  const api = factory();
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  root.dropExpectation = api.dropExpectation;
  root.formatExact = api.formatExact;
  root.DROP_BPS = api.DROP_BPS;
  root.DROP_N_MAX = api.DROP_N_MAX;
  root.renderExpectationChart = api.renderExpectationChart;
  root.filterItems = api.filterItems;
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  const DROP_BPS = 10000;
  const DROP_N_MAX = 1000000;

  function dropExpectation(chanceBps, quantityMin, quantityMax, kills) {
    const n = Math.max(0, Math.min(DROP_N_MAX, Math.round(Number(kills) || 0)));
    const p = Math.min(DROP_BPS, Math.max(0, Number(chanceBps) || 0)) / DROP_BPS;
    const mean = (Number(quantityMin) + Number(quantityMax)) / 2;
    return {
      kills: n,
      chance: p,
      mean,
      successes: n * p,
      units: n * p * mean,
    };
  }

  function formatExact(value) {
    if (!Number.isFinite(value)) return "—";
    if (Object.is(value, 0)) return "0";
    const text = Math.abs(value) < 1e-6 || Math.abs(value) >= 1e12
      ? value.toExponential(8)
      : value.toPrecision(12);
    if (text.includes("e") || text.includes("E")) return text.replace(/(\.\d*?[1-9])0+e/, "$1e").replace(/\.0+e/, "e");
    return text.replace(/(\.\d*?[1-9])0+$/, "$1").replace(/\.0+$/, "");
  }

  function filterItems(items, query, category) {
    const needle = String(query || "").trim().toLowerCase();
    return (items || []).filter((item) => {
      if (category && item.category !== category) return false;
      if (!needle) return true;
      const tags = Array.isArray(item.tags) ? item.tags.join(" ") : "";
      const hay = `${item.display_name || ""} ${item.label || ""} ${item.content_id} ${tags}`.toLowerCase();
      return hay.includes(needle);
    });
  }

  function renderExpectationChart(svg, spec) {
    if (!svg) return null;
    const bound = Math.min(DROP_N_MAX, Math.max(0, Math.round(Number(spec.kills) || 0)));
    const expected = spec.entry
      ? dropExpectation(spec.chanceBps, spec.quantityMin, spec.quantityMax, bound)
      : { successes: 0, units: 0, chance: 0, mean: 0, kills: bound };
    const monster = spec.monster || "no source";
    const name = spec.itemName || "no item";
    if (spec.title) {
      spec.title.textContent =
        `${monster} / ${name} / ${formatExact(expected.chance * 100)}% / qty ${
          spec.entry ? spec.quantityMin + "–" + spec.quantityMax : "—"
        } / mean ${formatExact(expected.mean || 0)}`;
    }
    const width = 640;
    const height = 280;
    const pad = { l: 72, r: 16, t: 16, b: 32 };
    const plotW = width - pad.l - pad.r;
    const plotH = height - pad.t - pad.b;
    const yMax = expected.units > 0 ? expected.units : 1;
    const xAt = (n) => pad.l + (bound === 0 ? 0 : (n / bound) * plotW);
    const yAt = (units) => pad.t + plotH - (units / yMax) * plotH;
    let axis = "";
    for (let i = 0; i <= 4; i++) {
      const n = Math.round((bound * i) / 4);
      const units = (yMax * i) / 4;
      axis += `<line x1="${xAt(n)}" y1="${pad.t}" x2="${xAt(n)}" y2="${pad.t + plotH}" stroke="#20323d"/>`;
      axis += `<text x="${xAt(n)}" y="${height - 8}" fill="#9aadb8" font-size="11" text-anchor="middle">${n}</text>`;
      axis += `<text x="${pad.l - 8}" y="${yAt(expected.units > 0 ? units : 0)}" fill="#9aadb8" font-size="11" text-anchor="end">${formatExact(expected.units > 0 ? units : (i === 0 ? 0 : 1))}</text>`;
    }
    const line = `<line x1="${xAt(0)}" y1="${yAt(0)}" x2="${xAt(bound)}" y2="${yAt(expected.units)}" stroke="#56c7da" stroke-width="2"/>`;
    svg.innerHTML = `
      <text x="${pad.l}" y="12" fill="#9aadb8" font-size="11">Expected units</text>
      <text x="${width - 8}" y="${height - 8}" fill="#9aadb8" font-size="11" text-anchor="end">Eligible kills</text>
      ${axis}${line}
      <line id="${spec.prefix || "drop"}CrossX" stroke="#e6b75e" stroke-dasharray="3 3" visibility="hidden"/>
      <line id="${spec.prefix || "drop"}CrossY" stroke="#e6b75e" stroke-dasharray="3 3" visibility="hidden"/>
      <circle id="${spec.prefix || "drop"}Marker" r="4" fill="#56c7da" visibility="hidden"/>`;
    const prefix = spec.prefix || "drop";
    const inspect = (n) => spec.entry
      ? dropExpectation(spec.chanceBps, spec.quantityMin, spec.quantityMax, n)
      : { kills: n, successes: 0, units: 0, chance: 0, mean: 0 };
    const show = (n, event) => {
      const point = inspect(n);
      const marker = svg.querySelector("#" + prefix + "Marker");
      const crossX = svg.querySelector("#" + prefix + "CrossX");
      const crossY = svg.querySelector("#" + prefix + "CrossY");
      const x = xAt(point.kills);
      const y = yAt(point.units);
      marker.setAttribute("cx", x);
      marker.setAttribute("cy", y);
      marker.setAttribute("visibility", "visible");
      crossX.setAttribute("x1", x); crossX.setAttribute("x2", x);
      crossX.setAttribute("y1", pad.t); crossX.setAttribute("y2", pad.t + plotH);
      crossX.setAttribute("visibility", "visible");
      crossY.setAttribute("y1", y); crossY.setAttribute("y2", y);
      crossY.setAttribute("x1", pad.l); crossY.setAttribute("x2", pad.l + plotW);
      crossY.setAttribute("visibility", "visible");
      if (spec.hover) {
        spec.hover.hidden = false;
        if (event) {
          spec.hover.style.left = `${event.offsetX + 12}px`;
          spec.hover.style.top = `${event.offsetY + 12}px`;
        }
        spec.hover.textContent = [
          `N ${point.kills}`,
          monster,
          name,
          `chance ${formatExact(point.chance * 100)}%`,
          `mean quantity ${formatExact(point.mean)}`,
          `expected successes ${formatExact(point.successes)}`,
          `expected units ${formatExact(point.units)}`,
        ].join("\n");
      }
      return point;
    };
    svg.onmousemove = (event) => {
      const rect = svg.getBoundingClientRect();
      const local = ((event.clientX - rect.left) / rect.width) * width;
      const ratio = Math.min(1, Math.max(0, (local - pad.l) / plotW));
      show(Math.round(ratio * bound), event);
    };
    svg.onmouseleave = () => {
      if (spec.hover) spec.hover.hidden = true;
      [prefix + "Marker", prefix + "CrossX", prefix + "CrossY"].forEach((id) => {
        svg.querySelector("#" + id)?.setAttribute("visibility", "hidden");
      });
    };
    return { expected, inspect: (n) => inspect(Math.max(0, Math.min(DROP_N_MAX, Math.round(Number(n) || 0)))) };
  }

  return { DROP_BPS, DROP_N_MAX, dropExpectation, formatExact, filterItems, renderExpectationChart };
});
