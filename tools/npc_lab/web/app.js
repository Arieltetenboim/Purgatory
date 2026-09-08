const state = {
  items: [],
  selectedPath: null,
  originalText: "",
  dirty: false,
};

const els = {
  npcList: document.getElementById("npcList"),
  filterInput: document.getElementById("filterInput"),
  newButton: document.getElementById("newButton"),
  reloadButton: document.getElementById("reloadButton"),
  saveButton: document.getElementById("saveButton"),
  jsonEditor: document.getElementById("jsonEditor"),
  documentTitle: document.getElementById("documentTitle"),
  documentPath: document.getElementById("documentPath"),
  dirtyBadge: document.getElementById("dirtyBadge"),
  statusText: document.getElementById("statusText"),
  validationStatus: document.getElementById("validationStatus"),
  validationErrors: document.getElementById("validationErrors"),
  summaryId: document.getElementById("summaryId"),
  summarySchema: document.getElementById("summarySchema"),
  summaryName: document.getElementById("summaryName"),
  summaryArea: document.getElementById("summaryArea"),
  summaryRole: document.getElementById("summaryRole"),
};

function setStatus(message) {
  els.statusText.textContent = message;
}

function setDirty(isDirty) {
  state.dirty = isDirty;
  els.saveButton.disabled = !state.selectedPath || !isDirty;
  els.dirtyBadge.textContent = isDirty ? "DIRTY" : "CLEAN";
  els.dirtyBadge.classList.toggle("dirty", isDirty);
  els.dirtyBadge.classList.toggle("clean", !isDirty);
}

function confirmDiscardIfNeeded() {
  if (!state.dirty) {
    return true;
  }
  return window.confirm("Discard unsaved NPC Lab changes?");
}

function parseEditor() {
  const text = els.jsonEditor.value;
  try {
    const documentValue = JSON.parse(text);
    return { ok: true, documentValue, error: null };
  } catch (error) {
    return { ok: false, documentValue: null, error: String(error.message || error) };
  }
}

function updateInspector() {
  if (!state.selectedPath) {
    els.validationStatus.textContent = "No document";
    els.validationStatus.className = "value muted";
    els.validationErrors.textContent = "—";
    els.summaryId.textContent = "—";
    els.summarySchema.textContent = "—";
    els.summaryName.textContent = "—";
    els.summaryArea.textContent = "—";
    els.summaryRole.textContent = "—";
    return;
  }

  const parsed = parseEditor();
  if (!parsed.ok) {
    els.validationStatus.textContent = "Invalid JSON";
    els.validationStatus.className = "value bad";
    els.validationErrors.textContent = parsed.error;
    return;
  }

  const doc = parsed.documentValue || {};
  const design = doc.design && typeof doc.design === "object" ? doc.design : {};
  const errors = [];
  if (!Number.isInteger(doc.schema_version) || doc.schema_version < 1) {
    errors.push("schema_version must be an integer >= 1.");
  }
  if (typeof doc.id !== "string" || !doc.id.startsWith("npc.")) {
    errors.push("id must use the npc.* namespace.");
  }

  els.summaryId.textContent = doc.id ?? "—";
  els.summarySchema.textContent = doc.schema_version ?? "—";
  els.summaryName.textContent = design.working_name || "—";
  els.summaryArea.textContent = design.area || "—";
  els.summaryRole.textContent = design.role || "—";

  if (errors.length) {
    els.validationStatus.textContent = "Needs attention";
    els.validationStatus.className = "value bad";
    els.validationErrors.textContent = errors.join("\n");
  } else {
    els.validationStatus.textContent = "JSON + N1 contract OK";
    els.validationStatus.className = "value good";
    els.validationErrors.textContent = "No N1 shell errors.";
  }
}

function renderList() {
  const needle = els.filterInput.value.trim().toLowerCase();
  els.npcList.replaceChildren();

  const filtered = state.items.filter((item) => {
    const haystack = [
      item.id,
      item.working_name,
      item.area,
      item.path,
    ]
      .filter(Boolean)
      .join(" ")
      .toLowerCase();
    return !needle || haystack.includes(needle);
  });

  for (const item of filtered) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "npc-item";
    if (item.path === state.selectedPath) {
      button.classList.add("selected");
    }
    if (!item.valid) {
      button.classList.add("invalid");
    }

    const title = document.createElement("div");
    title.className = "npc-item-title";
    title.textContent = item.working_name || item.id || "(invalid NPC JSON)";
    button.appendChild(title);

    const path = document.createElement("div");
    path.className = "npc-item-path";
    path.textContent = item.path;
    button.appendChild(path);

    button.addEventListener("click", () => openNpc(item.path));
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
  setStatus("Loading NPC documents...");
  const response = await fetch("/api/npcs");
  if (!response.ok) {
    throw new Error(`NPC list failed: HTTP ${response.status}`);
  }
  const payload = await response.json();
  state.items = payload.items || [];
  renderList();
  setStatus(`Loaded ${state.items.length} NPC document(s).`);
}

async function openNpc(path) {
  if (path === state.selectedPath && !state.dirty) {
    return;
  }
  if (!confirmDiscardIfNeeded()) {
    return;
  }

  setStatus(`Opening ${path}...`);
  const response = await fetch(`/api/npc?path=${encodeURIComponent(path)}`);
  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload.error || `Open failed: HTTP ${response.status}`);
  }

  const pretty = `${JSON.stringify(payload.document, null, 2)}\n`;
  state.selectedPath = payload.path;
  state.originalText = pretty;
  els.jsonEditor.disabled = false;
  els.jsonEditor.value = pretty;
  els.documentPath.textContent = payload.path;
  els.documentTitle.textContent =
    payload.document?.design?.working_name ||
    payload.document?.id ||
    payload.path;

  setDirty(false);
  updateInspector();
  renderList();
  setStatus(`Opened ${payload.path}.`);
}

async function saveCurrent() {
  if (!state.selectedPath) {
    return;
  }

  const parsed = parseEditor();
  if (!parsed.ok) {
    updateInspector();
    setStatus("Cannot save invalid JSON.");
    return;
  }

  setStatus(`Saving ${state.selectedPath}...`);
  const response = await fetch(
    `/api/npc?path=${encodeURIComponent(state.selectedPath)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json; charset=utf-8" },
      body: JSON.stringify(parsed.documentValue),
    },
  );
  const payload = await response.json();
  if (!response.ok) {
    const details = (payload.validation_errors || []).join(" | ");
    throw new Error(payload.error + (details ? ` ${details}` : ""));
  }

  const pretty = `${JSON.stringify(parsed.documentValue, null, 2)}\n`;
  els.jsonEditor.value = pretty;
  state.originalText = pretty;
  setDirty(false);
  updateInspector();
  await refreshList();
  setStatus(`Saved ${state.selectedPath}.`);
}

async function newNpc() {
  if (!confirmDiscardIfNeeded()) {
    return;
  }

  const authoredId = window.prompt(
    "Authored NPC ID:",
    "npc.welcome.",
  );
  if (!authoredId) {
    return;
  }

  const suggestedArea =
    authoredId.startsWith("npc.") && authoredId.split(".").length >= 3
      ? authoredId.split(".")[1]
      : "unassigned";
  const area = window.prompt("Area / folder:", suggestedArea);
  if (area === null) {
    return;
  }

  setStatus(`Creating ${authoredId}...`);
  const response = await fetch("/api/new", {
    method: "POST",
    headers: { "Content-Type": "application/json; charset=utf-8" },
    body: JSON.stringify({ id: authoredId, area }),
  });
  const payload = await response.json();
  if (!response.ok) {
    throw new Error(payload.error || `Create failed: HTTP ${response.status}`);
  }

  await refreshList();
  await openNpc(payload.path);
}

els.jsonEditor.addEventListener("input", () => {
  setDirty(els.jsonEditor.value !== state.originalText);
  updateInspector();
});

els.filterInput.addEventListener("input", renderList);

els.newButton.addEventListener("click", () => {
  newNpc().catch((error) => {
    console.error(error);
    setStatus(`ERROR: ${error.message}`);
    window.alert(error.message);
  });
});

els.reloadButton.addEventListener("click", () => {
  if (!confirmDiscardIfNeeded()) {
    return;
  }
  const selected = state.selectedPath;
  refreshList()
    .then(() => (selected ? openNpc(selected) : null))
    .catch((error) => {
      console.error(error);
      setStatus(`ERROR: ${error.message}`);
    });
});

els.saveButton.addEventListener("click", () => {
  saveCurrent().catch((error) => {
    console.error(error);
    setStatus(`ERROR: ${error.message}`);
    window.alert(error.message);
  });
});

window.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveCurrent().catch((error) => {
      console.error(error);
      setStatus(`ERROR: ${error.message}`);
      window.alert(error.message);
    });
  }
});

window.addEventListener("beforeunload", (event) => {
  if (state.dirty) {
    event.preventDefault();
    event.returnValue = "";
  }
});

refreshList()
  .then(() => {
    if (state.items.length) {
      return openNpc(state.items[0].path);
    }
    return null;
  })
  .catch((error) => {
    console.error(error);
    setStatus(`ERROR: ${error.message}`);
  });
