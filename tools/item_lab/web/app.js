const $ = (id) => document.getElementById(id);
const state = { items: [], selected: null, draft: null, creating: false, saved: null };

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
    throw new Error(message);
  }
  return payload;
}

function iconUrl(item) {
  return item && item.icon_file ? "/api/icon?key=" + encodeURIComponent(item.icon) : "";
}

function renderList() {
  const query = $("searchInput").value.trim().toLowerCase();
  const category = $("categoryFilter").value;
  const rows = state.items.filter((item) => {
    if (category && item.category !== category) return false;
    const hay = `${item.display_name} ${item.label} ${item.content_id}`.toLowerCase();
    return !query || hay.includes(query);
  });
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
  $("iconPreview").src = src;
  $("groundPreview").src = src;
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
    const lines = [];
    for (const monster of used.monsters) lines.push(`Monster ${monster.id}`);
    for (const path of used.dialogue) lines.push(`Dialogue ${path}`);
    for (const path of used.equipment) lines.push(`Equipment ${path}`);
    $("whereUsed").textContent = lines.length ? lines.join("\n") : "No monster drops, dialogue references, or equipment file.";
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
  banner("New item. Save allocates a permanent Content ID.", "");
  renderList();
});

$("duplicateButton").addEventListener("click", () => {
  if (!state.saved) return;
  state.creating = true;
  state.selected = null;
  state.draft = { ...state.saved, label: state.saved.label + "_copy", content_id: null };
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
    state.draft = null;
    fill(blank("material"));
    banner("Create cancelled.", "ready");
    return;
  }
  if (state.saved) fill(state.saved);
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
    renderList();
  });
});

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
    banner(state.creating
      ? `Created ${item.content_id}. Rebuild the client and server before the game can load it.`
      : "Saved. Restart the client and server to load the JSON.", "ready");
    await boot(item.content_id);
  } catch (error) {
    banner(error.message, "error");
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
