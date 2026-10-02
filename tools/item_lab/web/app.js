const $ = (id) => document.getElementById(id);
const state = { items: [], selected: null, draft: null, creating: false, saved: null, sources: [], kills: 1000 };

function banner(text, kind) {
  const node = $("banner");
  node.textContent = text;
  node.className = "banner" + (kind ? " " + kind : "");
}

function snapshot(item) {
  return JSON.stringify({
    label: item.label,
    category: item.category,
    stack_limit: Number(item.stack_limit),
    drop_requires_confirmation: Boolean(item.drop_requires_confirmation),
    display_name: item.display_name || "",
    description: item.description || "",
    icon: item.icon || "",
    equipment_slot: item.equipment_slot || "",
    notes: item.notes || "",
    tags: item.tags || [],
  });
}

function readForm() {
  return {
    label: $("label").value.trim(),
    category: $("category").value,
    stack_limit: Number($("stackLimit").value),
    drop_requires_confirmation: $("confirmDrop").checked,
    display_name: $("displayName").value,
    description: $("description").value,
    icon: $("icon").value.trim(),
    equipment_slot: $("category").value === "equipment" ? $("equipmentSlot").value : null,
    notes: $("notes").value,
    tags: $("tags").value.split(",").map((tag) => tag.trim()).filter(Boolean),
    revision: state.saved ? state.saved.revision : null,
  };
}

function dirty() {
  if (!state.draft) return false;
  return snapshot(readForm()) !== snapshot(state.draft);
}

function showErrors(errors) {
  document.querySelectorAll(".field-error").forEach((node) => { node.textContent = ""; });
  const rest = [];
  for (const error of errors || []) {
    const lower = error.toLowerCase();
    const field = lower.includes("label") ? "label"
      : lower.includes("category") ? "category"
      : lower.includes("stack") ? "stack"
      : lower.includes("slot") ? "slot"
      : lower.includes("icon") ? "icon" : "";
    const node = field ? document.querySelector(`[data-for="${field}"]`) : null;
    if (node && !node.textContent) node.textContent = error;
    else rest.push(error);
  }
  $("errors").textContent = rest.join(" ");
}

async function api(path, options) {
  const response = await fetch(path, options);
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    const message = payload.error || (payload.errors || []).join(" ") || response.statusText;
    const error = new Error(message);
    error.status = response.status;
    error.conflict = Boolean(payload.conflict);
    throw error;
  }
  return payload;
}

function iconUrl(item) {
  return item && item.icon_file ? "/api/icon?key=" + encodeURIComponent(item.icon) : "";
}

function renderList() {
  const rows = window.filterItems(state.items, $("searchInput").value, $("categoryFilter").value);
  $("itemList").innerHTML = "";
  for (const item of rows) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "row";
    if (state.selected === item.content_id) button.classList.add("selected");
    if (state.selected === item.content_id && dirty()) button.classList.add("dirty");
    const img = document.createElement("img");
    img.alt = "";
    if (iconUrl(item)) img.src = iconUrl(item);
    const text = document.createElement("span");
    text.textContent = `${item.display_name || item.label}  ${item.content_id}  ${item.category}`;
    button.append(img, text);
    button.addEventListener("click", () => selectItem(item.content_id));
    $("itemList").append(button);
  }
}

function fill(item) {
  $("contentId").value = item.content_id || "unallocated";
  $("label").value = item.label || "";
  $("label").readOnly = !state.creating;
  $("category").value = item.category || "material";
  $("category").disabled = !state.creating;
  $("stackLimit").value = item.stack_limit || 1;
  $("confirmDrop").checked = Boolean(item.drop_requires_confirmation);
  $("displayName").value = item.display_name || "";
  $("description").value = item.description || "";
  $("icon").value = item.icon || "";
  $("equipmentSlot").value = item.equipment_slot || "weapon";
  $("equipmentSlot").disabled = !state.creating;
  $("equipmentBlock").hidden = $("category").value !== "equipment";
  $("notes").value = item.notes || "";
  $("tags").value = (item.tags || []).join(", ");
  const src = iconUrl(item);
  const missing = Boolean(item.icon) && !item.icon_file;
  $("iconMissing").hidden = !missing;
  for (const node of [$("iconPreview"), $("groundPreview")]) {
    node.classList.toggle("missing", missing || !src);
    if (src) node.src = src;
    else node.removeAttribute("src");
  }
  const title = item.display_name || item.label || "Untitled";
  $("infoTitle").textContent = title;
  $("infoDetail").textContent = `${item.category || ""} | Qty 1 | Stack ${item.stack_limit || 1}`;
  $("infoBody").textContent = item.description || "";
  $("duplicateButton").disabled = state.creating || !item.content_id;
  $("saveButton").disabled = !dirty() && !state.creating;
  $("revertButton").disabled = !dirty() && !state.creating;
}

async function selectItem(contentId) {
  if (dirty() && !confirm("Discard unsaved edits?")) return;
  banner("Loading item…", "");
  try {
    const item = state.items.find((row) => row.content_id === contentId);
    const loaded = await api("/api/item?label=" + encodeURIComponent(item.label));
    state.creating = false;
    state.selected = contentId;
    state.saved = loaded;
    state.draft = loaded;
    fill(loaded);
    const used = await api("/api/where-used?id=" + contentId + "&label=" + encodeURIComponent(loaded.label));
    state.sources = used.monsters || [];
    const lines = [];
    for (const monster of state.sources) lines.push(`Monster ${monster.id}`);
    for (const path of used.dialogue) lines.push(`Dialogue ${path}`);
    for (const path of used.equipment) lines.push(`Equipment ${path}`);
    $("whereUsed").textContent = lines.length ? lines.join("\n") : "No monster drops, dialogue references, or equipment file.";
    const picker = $("sourceMonster");
    picker.innerHTML = "";
    for (const monster of state.sources) {
      const option = document.createElement("option");
      option.value = monster.id;
      option.textContent = monster.id;
      picker.append(option);
    }
    renderItemChart();
    banner("Ready", "ready");
    renderList();
  } catch (error) {
    banner(error.message, "error");
  }
}

function blank(category) {
  return {
    label: "item.",
    category,
    stack_limit: category === "equipment" ? 1 : 20,
    drop_requires_confirmation: category === "equipment",
    display_name: "",
    description: "",
    icon: "item.placeholder",
    equipment_slot: category === "equipment" ? "weapon" : null,
    notes: "",
    tags: [],
    icon_file: false,
  };
}

$("newButton").addEventListener("click", () => {
  if (dirty() && !confirm("Discard unsaved edits?")) return;
  state.creating = true;
  state.selected = null;
  state.saved = null;
  state.draft = blank($("categoryFilter").value || "material");
  fill(state.draft);
  $("whereUsed").textContent = "Save the item before references can exist.";
  state.sources = [];
  $("sourceMonster").innerHTML = "";
  renderItemChart();
  banner("New item. Save allocates a permanent Content ID.", "");
  renderList();
});

$("duplicateButton").addEventListener("click", () => {
  if (!state.saved || state.creating) return;
  if (dirty() && !confirm("Discard unsaved edits?")) return;
  const source = state.saved;
  state.creating = true;
  state.selected = null;
  state.saved = null;
  state.draft = { ...source, label: source.label + "_copy", content_id: null, revision: null };
  fill(state.draft);
  banner("Duplicate will allocate a new Content ID. The original stays unchanged.", "");
});

$("reloadButton").addEventListener("click", async () => {
  if (dirty() && !confirm("Reload from disk and discard unsaved edits?")) return;
  await boot(state.selected);
});

$("revertButton").addEventListener("click", () => {
  if (state.creating) {
    state.creating = false;
    state.selected = null;
    state.saved = null;
    state.draft = null;
    state.sources = [];
    fill(blank("material"));
    $("saveButton").disabled = true;
    $("revertButton").disabled = true;
    $("duplicateButton").disabled = true;
    $("whereUsed").textContent = "Select an item.";
    $("sourceMonster").innerHTML = "";
    renderItemChart();
    banner("Create cancelled.", "ready");
    renderList();
    return;
  }
  if (state.saved) {
    state.draft = state.saved;
    fill(state.saved);
  }
});

["label","category","stackLimit","confirmDrop","displayName","description","icon","equipmentSlot","notes","tags"].forEach((id) => {
  $(id).addEventListener("input", () => {
    if ($("category").value === "equipment") $("stackLimit").value = "1";
    $("equipmentBlock").hidden = $("category").value !== "equipment";
    const form = readForm();
    $("infoTitle").textContent = form.display_name || form.label || "Untitled";
    $("infoDetail").textContent = `${form.category} | Qty 1 | Stack ${form.stack_limit || 1}`;
    $("infoBody").textContent = form.description;
    $("saveButton").disabled = !dirty() && !state.creating;
    $("revertButton").disabled = !dirty() && !state.creating;
    const known = state.items.some((row) => row.icon === form.icon && row.icon_file);
    const preview = known ? "/api/icon?key=" + encodeURIComponent(form.icon) : "";
    $("iconMissing").hidden = !form.icon || known;
    for (const node of [$("iconPreview"), $("groundPreview")]) {
      node.classList.toggle("missing", !preview);
      if (preview) node.src = preview;
    }
    renderList();
  });
});

$("searchInput").addEventListener("input", renderList);
$("categoryFilter").addEventListener("change", renderList);
$("sourceMonster").addEventListener("change", renderItemChart);
document.querySelectorAll("[data-item-kills]").forEach((button) => {
  button.addEventListener("click", () => {
    state.kills = Number(button.dataset.itemKills);
    $("itemKills").value = String(state.kills);
    renderItemChart();
  });
});
$("itemKills").addEventListener("input", () => {
  const value = Math.round(Number($("itemKills").value));
  if (value < 0 || value > window.DROP_N_MAX) return;
  state.kills = value;
  renderItemChart();
});

function renderItemChart() {
  const monster = state.sources.find((row) => row.id === $("sourceMonster").value) || null;
  const drops = monster ? monster.drops || [] : [];
  const entry = drops[0] || null;
  const empty = $("chartEmpty");
  if (!state.selected || state.creating) {
    empty.hidden = false;
    empty.textContent = "Save and select an item before the chart has a source.";
  } else if (!monster) {
    empty.hidden = false;
    empty.textContent = "No monster drop references this item.";
  } else if (!entry) {
    empty.hidden = false;
    empty.textContent = "The selected monster has no drop row for this item.";
  } else {
    empty.hidden = true;
  }
  const chart = window.renderExpectationChart($("itemChart"), {
    monster: monster ? monster.id : "no source",
    itemName: state.saved ? (state.saved.display_name || state.saved.label) : "no item",
    chanceBps: entry ? entry.chance_bps : 0,
    quantityMin: entry ? entry.quantity_min : 0,
    quantityMax: entry ? entry.quantity_max : 0,
    kills: state.kills,
    entry: Boolean(entry),
    title: $("itemChartTitle"),
    hover: $("itemHover"),
    prefix: "item",
  });
  const readout = $("itemInspect");
  if (readout && chart) {
    const point = chart.inspect(state.kills);
    readout.textContent = entry
      ? `At N=${point.kills}: expected successes ${window.formatExact(point.successes)}, expected units ${window.formatExact(point.units)}.`
      : "";
  }
}

$("iconFile").addEventListener("change", async () => {
  const file = $("iconFile").files[0];
  if (!file) return;
  const key = $("icon").value.trim();
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  try {
    await api("/api/icons", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ icon: key, png_base64: btoa(binary) }),
    });
    if (state.saved) state.saved.icon_file = true;
    $("iconPreview").src = "/api/icon?key=" + encodeURIComponent(key);
    $("groundPreview").src = $("iconPreview").src;
    banner("Icon stored. It does not replace an existing file.", "ready");
  } catch (error) {
    banner(error.message, "error");
  }
});

$("saveButton").addEventListener("click", async () => {
  const payload = readForm();
  banner("Saving…", "");
  showErrors([]);
  try {
    const result = state.creating
      ? await api("/api/items", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(payload) })
      : await api("/api/items/save", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(payload) });
    const item = result.item;
    const created = state.creating;
    state.creating = false;
    state.saved = item;
    state.draft = item;
    state.selected = item.content_id;
    banner(created
      ? `Created ${item.content_id}. Rebuild the client and server before the game can load it.`
      : "Saved. Restart the client and server to load the JSON.", "ready");
    await boot(item.content_id);
  } catch (error) {
    const kept = error.conflict
      ? " The unsaved draft is still here. Reload to see the saved item, then reconcile."
      : "";
    banner(error.message + kept, "error");
    showErrors([error.message]);
  }
});

async function boot(selectId) {
  banner("Loading items…", "");
  try {
    const health = await api("/api/health");
    if (health.tool !== "item-lab" || health.build !== "item-lab-v1") {
      throw new Error("This page is not the current Item Lab.");
    }
    state.items = (await api("/api/items")).items;
    const icons = await api("/api/icons");
    $("iconKeys").innerHTML = "";
    for (const key of icons.icons || []) {
      const option = document.createElement("option");
      option.value = key;
      $("iconKeys").append(option);
    }
    const requested = selectId || Number(new URLSearchParams(location.search).get("item"));
    banner("Ready", "ready");
    if (requested && state.items.some((item) => item.content_id === requested)) {
      await selectItem(requested);
    } else {
      renderList();
    }
  } catch (error) {
    banner(error.message, "error");
  }
}

boot();
