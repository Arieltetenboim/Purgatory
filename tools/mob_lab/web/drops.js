const DROP_BPS = 10000;
const DROP_N_MAX = 1000000;
const dropUi = { items: [], selected: 0, kills: 1000 };

function dropExpectation(chanceBps, quantityMin, quantityMax, kills) {
  const n = Math.max(0, Math.round(Number(kills) || 0));
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
  if (Number.isInteger(value)) return String(value);
  return value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
}

window.validateMonsterDrops = function(doc) {
  const errors = [];
  const drops = doc?.drops;
  if (drops == null) return errors;
  if (!Array.isArray(drops)) return ["drops must be a list."];
  const seen = new Set();
  drops.forEach((entry, index) => {
    const item = dropUi.items.find((row) => row.content_id === Number(entry.item));
    if (!item) errors.push(`drops[${index}] references an unknown item.`);
    else if (seen.has(item.content_id)) errors.push(`drops[${index}] duplicates an item.`);
    else seen.add(item.content_id);
    const chance = Number(entry.chance_bps);
    const low = Number(entry.quantity_min);
    const high = Number(entry.quantity_max);
    if (!Number.isInteger(chance) || chance < 0 || chance > DROP_BPS) {
      errors.push(`drops[${index}].chance_bps must be an integer from 0 to 10000.`);
    }
    if (!Number.isInteger(low) || low < 1 || !Number.isInteger(high) || high < low) {
      errors.push(`drops[${index}] quantity must be a positive range.`);
    }
    if (item && item.stack_limit === 1 && (low !== 1 || high !== 1)) {
      errors.push(`drops[${index}] quantity must be 1 because the item does not stack.`);
    }
    if (item && high > item.stack_limit) {
      errors.push(`drops[${index}].quantity_max exceeds the stack limit.`);
    }
  });
  return errors;
};

function dropRows() {
  const doc = window.mobLabState?.doc;
  return Array.isArray(doc?.drops) ? doc.drops : [];
}

function commitDrops(rows) {
  const doc = window.mobLabState.doc;
  if (rows.length) doc.drops = rows;
  else delete doc.drops;
  window.mobNoteDrops();
  renderMonsterDrops();
}

function renderChart() {
  const svg = document.getElementById("dropChart");
  const rows = dropRows();
  const entry = rows[dropUi.selected] || null;
  const item = entry ? dropUi.items.find((row) => row.content_id === Number(entry.item)) : null;
  const bound = Math.min(DROP_N_MAX, Math.max(0, dropUi.kills));
  const expected = entry
    ? dropExpectation(entry.chance_bps, entry.quantity_min, entry.quantity_max, bound)
    : { successes: 0, units: 0, chance: 0, mean: 0, kills: bound };
  const monster = window.mobLabState?.doc?.id || "monster";
  const name = item ? (item.display_name || item.label) : "no item";
  document.getElementById("dropChartTitle").textContent =
    `${monster} / ${name} / ${formatExact(expected.chance * 100)}% / qty ${entry ? entry.quantity_min + "–" + entry.quantity_max : "—"} / mean ${formatExact(expected.mean || 0)}`;
  document.getElementById("dropChartDraft").textContent = window.mobLabState?.dirty ? "Unsaved draft" : "";
  const width = 640;
  const height = 280;
  const pad = { l: 56, r: 16, t: 16, b: 32 };
  const plotW = width - pad.l - pad.r;
  const plotH = height - pad.t - pad.b;
  const yMax = expected.units > 0 ? expected.units : 1;
  const xAt = (n) => pad.l + (bound === 0 ? 0 : (n / bound) * plotW);
  const yAt = (units) => pad.t + plotH - (units / yMax) * plotH;
  const ticks = 4;
  let axis = "";
  for (let i = 0; i <= ticks; i++) {
    const n = Math.round((bound * i) / ticks);
    const units = (yMax * i) / ticks;
    axis += `<line x1="${xAt(n)}" y1="${pad.t}" x2="${xAt(n)}" y2="${pad.t + plotH}" stroke="#20323d"/>`;
    axis += `<text x="${xAt(n)}" y="${height - 8}" fill="#9aadb8" font-size="11" text-anchor="middle">${n}</text>`;
    axis += `<text x="${pad.l - 8}" y="${yAt(expected.units > 0 ? units : 0)}" fill="#9aadb8" font-size="11" text-anchor="end">${formatExact(expected.units > 0 ? units : (i === 0 ? 0 : 1))}</text>`;
  }
  const line = `<line x1="${xAt(0)}" y1="${yAt(0)}" x2="${xAt(bound)}" y2="${yAt(expected.units)}" stroke="#56c7da" stroke-width="2"/>`;
  svg.innerHTML = `
    <text x="${pad.l}" y="12" fill="#9aadb8" font-size="11">Expected units</text>
    <text x="${width - 8}" y="${height - 8}" fill="#9aadb8" font-size="11" text-anchor="end">Eligible kills</text>
    ${axis}${line}
    <line id="dropCrossX" stroke="#e6b75e" stroke-dasharray="3 3" visibility="hidden"/>
    <line id="dropCrossY" stroke="#e6b75e" stroke-dasharray="3 3" visibility="hidden"/>
    <circle id="dropMarker" r="4" fill="#56c7da" visibility="hidden"/>`;
  svg.onmousemove = (event) => {
    const rect = svg.getBoundingClientRect();
    const local = ((event.clientX - rect.left) / rect.width) * width;
    const ratio = Math.min(1, Math.max(0, (local - pad.l) / plotW));
    const n = Math.round(ratio * bound);
    const point = entry
      ? dropExpectation(entry.chance_bps, entry.quantity_min, entry.quantity_max, n)
      : { kills: n, successes: 0, units: 0, chance: 0 };
    const marker = document.getElementById("dropMarker");
    const crossX = document.getElementById("dropCrossX");
    const crossY = document.getElementById("dropCrossY");
    const x = xAt(n);
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
    const hover = document.getElementById("dropHover");
    hover.hidden = false;
    hover.style.left = `${event.offsetX + 12}px`;
    hover.style.top = `${event.offsetY + 12}px`;
    hover.textContent = [
      `N ${point.kills}`,
      monster,
      name,
      `chance ${formatExact(point.chance * 100)}%`,
      `expected successes ${formatExact(point.successes)}`,
      `expected units ${formatExact(point.units)}`,
    ].join("\n");
  };
  svg.onmouseleave = () => {
    document.getElementById("dropHover").hidden = true;
    ["dropMarker", "dropCrossX", "dropCrossY"].forEach((id) => {
      document.getElementById(id)?.setAttribute("visibility", "hidden");
    });
  };
}

function renderMonsterDrops() {
  const host = document.getElementById("dropRows");
  if (!host || !window.mobLabState) return;
  const rows = dropRows();
  document.getElementById("dropEmpty").hidden = rows.length > 0;
  host.innerHTML = "";
  rows.forEach((entry, index) => {
    const row = document.createElement("div");
    row.className = "drop-row";
    const picker = document.createElement("select");
    for (const item of dropUi.items) {
      const option = document.createElement("option");
      option.value = String(item.content_id);
      option.textContent = `${item.display_name || item.label} (${item.content_id})`;
      picker.append(option);
    }
    picker.value = String(entry.item);
    picker.addEventListener("change", () => {
      entry.item = Number(picker.value);
      dropUi.selected = index;
      commitDrops(rows);
    });
    const chance = document.createElement("input");
    chance.type = "number"; chance.min = "0"; chance.max = "100"; chance.step = "0.01";
    chance.value = String(Number(entry.chance_bps) / 100);
    chance.title = "Chance percent. 10000 basis points = 100%.";
    chance.addEventListener("input", () => {
      entry.chance_bps = Math.round(Number(chance.value) * 100);
      dropUi.selected = index;
      window.mobNoteDrops();
      renderChart();
    });
    const min = document.createElement("input");
    const max = document.createElement("input");
    min.type = max.type = "number"; min.min = max.min = "1"; min.step = max.step = "1";
    min.value = entry.quantity_min; max.value = entry.quantity_max;
    min.addEventListener("input", () => { entry.quantity_min = Number(min.value); dropUi.selected = index; window.mobNoteDrops(); renderChart(); });
    max.addEventListener("input", () => { entry.quantity_max = Number(max.value); dropUi.selected = index; window.mobNoteDrops(); renderChart(); });
    const focus = document.createElement("button");
    focus.type = "button"; focus.textContent = "GRAPH";
    focus.addEventListener("click", () => { dropUi.selected = index; renderChart(); });
    const open = document.createElement("button");
    open.type = "button"; open.textContent = "OPEN IN ITEM LAB";
    open.addEventListener("click", async () => {
      const response = await fetch("/api/open-item-lab", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ content_id: Number(entry.item) }),
      });
      const payload = await response.json();
      if (!response.ok) { alert(payload.error || "Item Lab did not open."); return; }
      window.open(payload.url, "purgatory-item-lab");
    });
    const remove = document.createElement("button");
    remove.type = "button"; remove.textContent = "REMOVE";
    remove.addEventListener("click", () => {
      rows.splice(index, 1);
      dropUi.selected = Math.max(0, index - 1);
      commitDrops(rows);
    });
    row.append(picker, chance, min, max, focus, open, remove);
    if (index === dropUi.selected) row.style.outline = "1px solid #56c7da";
    host.append(row);
  });
  renderChart();
}
window.renderMonsterDrops = renderMonsterDrops;

document.getElementById("addDropButton")?.addEventListener("click", () => {
  const first = dropUi.items[0];
  if (!first || !window.mobLabState?.doc) return;
  const rows = dropRows().slice();
  rows.push({ item: first.content_id, chance_bps: 10000, quantity_min: 1, quantity_max: 1 });
  dropUi.selected = rows.length - 1;
  commitDrops(rows);
});

document.querySelectorAll("[data-kills]").forEach((button) => {
  button.addEventListener("click", () => {
    dropUi.kills = Number(button.dataset.kills);
    document.getElementById("dropKills").value = String(dropUi.kills);
    renderChart();
  });
});
document.getElementById("dropKills")?.addEventListener("input", () => {
  const value = Math.round(Number(document.getElementById("dropKills").value));
  if (value < 0 || value > DROP_N_MAX) return;
  dropUi.kills = value;
  renderChart();
});

fetch("/api/items").then((response) => response.json()).then((payload) => {
  dropUi.items = payload.items || [];
  renderMonsterDrops();
  if (window.mobLabState?.doc) window.mobNoteDrops();
}).catch(() => {});
