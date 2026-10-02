const DROP_BPS = window.DROP_BPS;
const DROP_N_MAX = window.DROP_N_MAX;
const dropExpectation = window.dropExpectation;
const formatExact = window.formatExact;
const dropUi = { items: [], selected: 0, kills: 1000 };

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
  const rows = dropRows();
  const entry = rows[dropUi.selected] || null;
  const item = entry ? dropUi.items.find((row) => row.content_id === Number(entry.item)) : null;
  document.getElementById("dropChartDraft").textContent = window.mobLabState?.dirty ? "Unsaved draft" : "";
  const chart = window.renderExpectationChart(document.getElementById("dropChart"), {
    monster: window.mobLabState?.doc?.id || "no source",
    itemName: item ? (item.display_name || item.label) : "no item",
    chanceBps: entry ? entry.chance_bps : 0,
    quantityMin: entry ? entry.quantity_min : 0,
    quantityMax: entry ? entry.quantity_max : 0,
    kills: dropUi.kills,
    entry: Boolean(entry),
    title: document.getElementById("dropChartTitle"),
    hover: document.getElementById("dropHover"),
    prefix: "drop",
  });
  dropUi.inspect = chart ? chart.inspect : null;
  const readout = document.getElementById("dropInspect");
  if (readout && chart) {
    const point = chart.inspect(dropUi.kills);
    readout.textContent = entry
      ? `At N=${point.kills}: expected successes ${formatExact(point.successes)}, expected units ${formatExact(point.units)}.`
      : "No drop row is selected.";
  }
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
