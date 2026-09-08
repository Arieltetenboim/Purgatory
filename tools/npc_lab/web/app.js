const state = {
  items: [],
  selectedPath: null,
  documentValue: null,
  originalCanonical: "",
  dirty: false,
  activeSurface: "identity",
};

const $ = (id) => document.getElementById(id);

const els = {
  npcList: $("npcList"),
  filterInput: $("filterInput"),
  newButton: $("newButton"),
  reloadButton: $("reloadButton"),
  saveButton: $("saveButton"),
  jsonEditor: $("jsonEditor"),
  documentTitle: $("documentTitle"),
  documentPath: $("documentPath"),
  dirtyBadge: $("dirtyBadge"),
  statusText: $("statusText"),
  validationStatus: $("validationStatus"),
  validationErrors: $("validationErrors"),
  summaryId: $("summaryId"),
  summarySchema: $("summarySchema"),
  summaryName: $("summaryName"),
  summaryArea: $("summaryArea"),
  summaryRole: $("summaryRole"),
  identitySurface: $("identitySurface"),
  shellSurface: $("shellSurface"),
  authoredId: $("authoredId"),
  schemaVersion: $("schemaVersion"),
  workingName: $("workingName"),
  displayName: $("displayName"),
  role: $("role"),
  area: $("area"),
  tags: $("tags"),
  background: $("background"),
  personality: $("personality"),
  speechStyle: $("speechStyle"),
  gameplayPurposes: $("gameplayPurposes"),
  narrativePurposes: $("narrativePurposes"),
  relationshipList: $("relationshipList"),
  addRelationshipButton: $("addRelationshipButton"),
  designNotes: $("designNotes"),
};

const identityInputs = [
  els.workingName, els.displayName, els.role, els.area, els.tags,
  els.background, els.personality, els.speechStyle,
  els.gameplayPurposes, els.narrativePurposes, els.designNotes,
];

function setStatus(message) {
  els.statusText.textContent = message;
}

function canonical(value) {
  return JSON.stringify(value);
}

function cloneValue(value) {
  return JSON.parse(JSON.stringify(value));
}

function lineArray(value) {
  return String(value || "").split(/\r?\n/).map((x) => x.trim()).filter(Boolean);
}

function arrayText(value) {
  return Array.isArray(value) ? value.join("\n") : "";
}

function commaArray(value) {
  const seen = new Set();
  const result = [];
  for (const raw of String(value || "").split(",")) {
    const item = raw.trim();
    if (item && !seen.has(item)) {
      seen.add(item);
      result.push(item);
    }
  }
  return result;
}

function containsHebrew(value) {
  return /[\u0590-\u05FF]/u.test(JSON.stringify(value));
}

function clientValidation(doc) {
  const errors = [];
  if (!doc || typeof doc !== "object" || Array.isArray(doc)) {
    return ["NPC document must be a JSON object."];
  }
  if (!Number.isInteger(doc.schema_version) || doc.schema_version < 1) {
    errors.push("schema_version must be an integer >= 1.");
  }
  if (typeof doc.id !== "string" || !doc.id.startsWith("npc.")) {
    errors.push("id must use the npc.* namespace.");
  }
  if (!doc.design || typeof doc.design !== "object" || Array.isArray(doc.design)) {
    errors.push("design must be an object.");
  }
  if (containsHebrew(doc)) {
    errors.push("NPC authored content is English-only; Hebrew characters are not allowed.");
  }
  return errors;
}

function markDirtyFromDocument() {
  const dirty = Boolean(state.selectedPath) &&
    Boolean(state.documentValue) &&
    canonical(state.documentValue) !== state.originalCanonical;
  state.dirty = dirty;
  els.saveButton.disabled = !state.selectedPath || !dirty;
  els.dirtyBadge.textContent = dirty ? "DIRTY" : "CLEAN";
  els.dirtyBadge.classList.toggle("dirty", dirty);
  els.dirtyBadge.classList.toggle("clean", !dirty);
}

function confirmDiscardIfNeeded() {
  return !state.dirty || window.confirm("Discard unsaved NPC Lab changes?");
}

function parseRawEditor() {
  try {
    return { ok: true, documentValue: JSON.parse(els.jsonEditor.value), error: null };
  } catch (error) {
    return { ok: false, documentValue: null, error: String(error.message || error) };
  }
}

function syncRawFromDocument() {
  els.jsonEditor.value = state.documentValue
    ? `${JSON.stringify(state.documentValue, null, 2)}\n`
    : "";
}

function updateHeader() {
  const doc = state.documentValue || {};
  const design = doc.design && typeof doc.design === "object" ? doc.design : {};
  els.documentTitle.textContent =
    design.working_name || doc.id || state.selectedPath || "No NPC selected";
}

function updateInspector(rawError = null) {
  if (!state.selectedPath) {
    els.validationStatus.textContent = "No document";
    els.validationStatus.className = "value muted";
    els.validationErrors.textContent = "-";
    els.summaryId.textContent = "-";
    els.summarySchema.textContent = "-";
    els.summaryName.textContent = "-";
    els.summaryArea.textContent = "-";
    els.summaryRole.textContent = "-";
    return;
  }

  if (rawError) {
    els.validationStatus.textContent = "Invalid JSON";
    els.validationStatus.className = "value bad";
    els.validationErrors.textContent = rawError;
    return;
  }

  const doc = state.documentValue || {};
  const design = doc.design && typeof doc.design === "object" ? doc.design : {};
  const errors = clientValidation(doc);

  els.summaryId.textContent = doc.id ?? "-";
  els.summarySchema.textContent = doc.schema_version ?? "-";
  els.summaryName.textContent = design.working_name || "-";
  els.summaryArea.textContent = design.area || "-";
  els.summaryRole.textContent = design.role || "-";

  if (errors.length) {
    els.validationStatus.textContent = "Needs attention";
    els.validationStatus.className = "value bad";
    els.validationErrors.textContent = errors.join("\n");
  } else {
    els.validationStatus.textContent = "N2 authoring contract OK";
    els.validationStatus.className = "value good";
    els.validationErrors.textContent = "Identity/design structure + English-only check passed.";
  }
}

function relationshipField(labelText, key, value, placeholder) {
  const label = document.createElement("label");
  label.className = "field";

  const span = document.createElement("span");
  span.textContent = labelText;

  const input = document.createElement("input");
  input.className = "input relationship-input";
  input.type = "text";
  input.dataset.relationshipKey = key;
  input.placeholder = placeholder;
  input.value = value;
  input.addEventListener("input", applyIdentityForm);

  label.append(span, input);
  return label;
}

function populateRelationships(relationships) {
  els.relationshipList.replaceChildren();
  const values = Array.isArray(relationships) ? relationships : [];

  if (!values.length) {
    const empty = document.createElement("div");
    empty.className = "empty-state";
    empty.textContent = "No authored relationships yet.";
    els.relationshipList.appendChild(empty);
    return;
  }

  values.forEach((relationship, index) => {
    const rel = relationship && typeof relationship === "object" ? relationship : {};
    const card = document.createElement("div");
    card.className = "relationship-card";
    card.dataset.relationshipIndex = String(index);

    const header = document.createElement("div");
    header.className = "relationship-card-header";

    const title = document.createElement("div");
    title.className = "relationship-title";
    title.textContent = rel.working_label || `Relationship ${index + 1}`;

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "icon-button";
    remove.textContent = "REMOVE";
    remove.addEventListener("click", () => {
      const relationshipsNow = Array.isArray(state.documentValue.relationships)
        ? [...state.documentValue.relationships]
        : [];
      relationshipsNow.splice(index, 1);
      state.documentValue.relationships = relationshipsNow;
      populateRelationships(relationshipsNow);
      syncRawFromDocument();
      updateInspector();
      markDirtyFromDocument();
    });

    header.append(title, remove);
    card.appendChild(header);

    const grid = document.createElement("div");
    grid.className = "form-grid two";
    grid.appendChild(relationshipField("Working Label", "working_label", rel.working_label || "", "Workshop craftsperson"));
    grid.appendChild(relationshipField("Target ID", "target_id", rel.target_id || "", "npc.welcome.someone (optional)"));
    card.appendChild(grid);

    const notesLabel = document.createElement("label");
    notesLabel.className = "field";
    const notesTitle = document.createElement("span");
    notesTitle.textContent = "Notes";
    const notes = document.createElement("textarea");
    notes.className = "form-textarea short relationship-input";
    notes.dataset.relationshipKey = "notes";
    notes.value = rel.notes || "";
    notes.addEventListener("input", applyIdentityForm);
    notesLabel.append(notesTitle, notes);
    card.appendChild(notesLabel);

    els.relationshipList.appendChild(card);
  });
}

function readRelationshipsFromForm() {
  const existing = Array.isArray(state.documentValue?.relationships)
    ? state.documentValue.relationships
    : [];

  return [...els.relationshipList.querySelectorAll(".relationship-card")].map((card) => {
    const index = Number(card.dataset.relationshipIndex);
    const base =
      Number.isInteger(index) && existing[index] && typeof existing[index] === "object"
        ? cloneValue(existing[index])
        : {};

    for (const input of card.querySelectorAll(".relationship-input")) {
      const key = input.dataset.relationshipKey;
      base[key] = key === "target_id" ? (input.value.trim() || null) : input.value.trim();
    }
    return base;
  });
}

function populateIdentityForm(doc) {
  const design = doc?.design && typeof doc.design === "object" ? doc.design : {};
  els.authoredId.value = doc?.id || "";
  els.schemaVersion.value = doc?.schema_version ?? "";
  els.workingName.value = design.working_name || "";
  els.displayName.value = design.display_name || "";
  els.role.value = design.role || "";
  els.area.value = design.area || "";
  els.tags.value = Array.isArray(design.tags) ? design.tags.join(", ") : "";
  els.background.value = design.background || "";
  els.personality.value = arrayText(design.personality);
  els.speechStyle.value = arrayText(design.speech_style);
  els.gameplayPurposes.value = arrayText(design.gameplay_purposes);
  els.narrativePurposes.value = arrayText(design.narrative_purposes);
  els.designNotes.value = arrayText(doc?.notes);
  populateRelationships(doc?.relationships);
}

function applyIdentityForm() {
  if (!state.documentValue) return;

  const doc = state.documentValue;
  const design = doc.design && typeof doc.design === "object" ? doc.design : {};

  design.working_name = els.workingName.value.trim();
  design.display_name = els.displayName.value.trim() || null;
  design.role = els.role.value.trim();
  design.area = els.area.value.trim();
  design.tags = commaArray(els.tags.value);
  design.background = els.background.value.trim();
  design.personality = lineArray(els.personality.value);
  design.speech_style = lineArray(els.speechStyle.value);
  design.gameplay_purposes = lineArray(els.gameplayPurposes.value);
  design.narrative_purposes = lineArray(els.narrativePurposes.value);

  doc.design = design;
  doc.relationships = readRelationshipsFromForm();
  doc.notes = lineArray(els.designNotes.value);

  syncRawFromDocument();
  updateHeader();
  updateInspector();
  markDirtyFromDocument();
}

function setSurface(surface) {
  if (surface === state.activeSurface) return;

  if (surface === "identity") {
    const parsed = parseRawEditor();
    if (!parsed.ok) {
      window.alert("Raw JSON is invalid. Fix it before returning to Identity.");
      setStatus(`ERROR: ${parsed.error}`);
      return;
    }
    state.documentValue = parsed.documentValue;
    populateIdentityForm(state.documentValue);
    updateHeader();
    updateInspector();
    markDirtyFromDocument();
  } else {
    applyIdentityForm();
    syncRawFromDocument();
  }

  state.activeSurface = surface;
  els.identitySurface.classList.toggle("active", surface === "identity");
  els.shellSurface.classList.toggle("active", surface === "shell");

  for (const tab of document.querySelectorAll(".future-tab[data-surface]")) {
    tab.classList.toggle("active", tab.dataset.surface === surface);
  }
}

function renderList() {
  const needle = els.filterInput.value.trim().toLowerCase();
  els.npcList.replaceChildren();

  const filtered = state.items.filter((item) => {
    const haystack = [item.id, item.working_name, item.area, item.path]
      .filter(Boolean).join(" ").toLowerCase();
    return !needle || haystack.includes(needle);
  });

  for (const item of filtered) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "npc-item";
    if (item.path === state.selectedPath) button.classList.add("selected");
    if (!item.valid) button.classList.add("invalid");

    const title = document.createElement("div");
    title.className = "npc-item-title";
    title.textContent = item.working_name || item.id || "(invalid NPC JSON)";

    const path = document.createElement("div");
    path.className = "npc-item-path";
    path.textContent = item.path;

    button.append(title, path);
    button.addEventListener("click", () => openNpc(item.path).catch(handleError));
    els.npcList.appendChild(button);
  }

  if (!filtered.length) {
    const empty = document.createElement("div");
    empty.className = "value muted";
    empty.textContent = "No NPC documents match.";
    els.npcList.appendChild(empty);
  }
}

async function refreshList() {
  const response = await fetch("/api/npcs");
  if (!response.ok) throw new Error(`NPC list failed: HTTP ${response.status}`);
  const payload = await response.json();
  state.items = payload.items || [];
  renderList();
}

async function openNpc(path) {
  if (path === state.selectedPath && !state.dirty) return;
  if (!confirmDiscardIfNeeded()) return;

  const response = await fetch(`/api/npc?path=${encodeURIComponent(path)}`);
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || `Open failed: HTTP ${response.status}`);

  state.selectedPath = payload.path;
  state.documentValue = payload.document;
  state.originalCanonical = canonical(payload.document);

  els.jsonEditor.disabled = false;
  els.documentPath.textContent = payload.path;

  populateIdentityForm(state.documentValue);
  syncRawFromDocument();
  updateHeader();
  updateInspector();
  markDirtyFromDocument();
  renderList();
  setStatus(`Opened ${payload.path}.`);
}

async function saveCurrent() {
  if (!state.selectedPath) return;

  if (state.activeSurface === "shell") {
    const parsed = parseRawEditor();
    if (!parsed.ok) {
      updateInspector(parsed.error);
      setStatus("Cannot save invalid JSON.");
      return;
    }
    state.documentValue = parsed.documentValue;
  } else {
    applyIdentityForm();
  }

  const errors = clientValidation(state.documentValue);
  if (errors.length) {
    updateInspector();
    window.alert(`Cannot save:\n${errors.join("\n")}`);
    return;
  }

  const response = await fetch(`/api/npc?path=${encodeURIComponent(state.selectedPath)}`, {
    method: "POST",
    headers: { "Content-Type": "application/json; charset=utf-8" },
    body: JSON.stringify(state.documentValue),
  });

  const payload = await response.json();
  if (!response.ok) {
    const details = (payload.validation_errors || []).join(" | ");
    throw new Error(payload.error + (details ? ` ${details}` : ""));
  }

  state.originalCanonical = canonical(state.documentValue);
  syncRawFromDocument();
  markDirtyFromDocument();
  updateInspector();
  await refreshList();
  setStatus(`Saved ${state.selectedPath}.`);
}

async function newNpc() {
  if (!confirmDiscardIfNeeded()) return;

  const authoredId = window.prompt("Authored NPC ID:", "npc.welcome.");
  if (!authoredId) return;

  const parts = authoredId.split(".");
  const suggestedArea = authoredId.startsWith("npc.") && parts.length >= 3 ? parts[1] : "unassigned";
  const area = window.prompt("Area / folder:", suggestedArea);
  if (area === null) return;

  const response = await fetch("/api/new", {
    method: "POST",
    headers: { "Content-Type": "application/json; charset=utf-8" },
    body: JSON.stringify({ id: authoredId, area }),
  });
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || `Create failed: HTTP ${response.status}`);

  await refreshList();
  await openNpc(payload.path);
  if (state.activeSurface !== "identity") setSurface("identity");
}

function handleError(error) {
  console.error(error);
  setStatus(`ERROR: ${error.message}`);
  window.alert(error.message);
}

for (const input of identityInputs) {
  input.addEventListener("input", applyIdentityForm);
}

els.addRelationshipButton.addEventListener("click", () => {
  if (!state.documentValue) return;
  const relationships = Array.isArray(state.documentValue.relationships)
    ? [...state.documentValue.relationships]
    : [];
  relationships.push({ working_label: "", target_id: null, notes: "" });
  state.documentValue.relationships = relationships;
  populateRelationships(relationships);
  syncRawFromDocument();
  updateInspector();
  markDirtyFromDocument();
});

els.jsonEditor.addEventListener("input", () => {
  const parsed = parseRawEditor();
  if (!parsed.ok) {
    state.dirty = true;
    els.saveButton.disabled = true;
    els.dirtyBadge.textContent = "DIRTY";
    els.dirtyBadge.classList.add("dirty");
    els.dirtyBadge.classList.remove("clean");
    updateInspector(parsed.error);
    return;
  }
  state.documentValue = parsed.documentValue;
  updateHeader();
  updateInspector();
  markDirtyFromDocument();
});

els.filterInput.addEventListener("input", renderList);
els.newButton.addEventListener("click", () => newNpc().catch(handleError));

els.reloadButton.addEventListener("click", () => {
  if (!confirmDiscardIfNeeded()) return;
  const selected = state.selectedPath;
  state.dirty = false;
  state.selectedPath = null;
  refreshList()
    .then(() => selected ? openNpc(selected) : null)
    .catch(handleError);
});

els.saveButton.addEventListener("click", () => saveCurrent().catch(handleError));

for (const tab of document.querySelectorAll(".future-tab[data-surface]")) {
  tab.addEventListener("click", () => setSurface(tab.dataset.surface));
}

window.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveCurrent().catch(handleError);
  }
});

window.addEventListener("beforeunload", (event) => {
  if (state.dirty) {
    event.preventDefault();
    event.returnValue = "";
  }
});

refreshList()
  .then(() => state.items.length ? openNpc(state.items[0].path) : null)
  .catch(handleError);
