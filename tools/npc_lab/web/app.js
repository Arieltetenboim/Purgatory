const state = {
  items: [],
  selectedPath: null,
  documentValue: null,
  originalCanonical: "",
  dirty: false,
  activeSurface: "dialogue",
  selectedBeatIndex: null,
};

const $ = (id) => document.getElementById(id);
const els = {};
[
  "npcList","filterInput","newButton","reloadButton","saveButton","jsonEditor","documentTitle","documentPath",
  "dirtyBadge","statusText","validationStatus","validationErrors","summaryId","summaryName","summaryBeatCount","summaryBeat",
  "identitySurface","dialogueSurface","stateSurface","shellSurface",
  "authoredId","schemaVersion","workingName","displayName","role","area","tags","background","personality","speechStyle",
  "gameplayPurposes","narrativePurposes","relationshipList","addRelationshipButton","designNotes",
  "beatList","addBeatButton","beatEmpty","beatForm","beatEditorHeading","removeBeatButton","beatId","beatTitle","beatPriority","beatEntryMode",
  "beatPool","beatConditionCount","conditionPreview","editConditionsButton","beatNotes","lineList","addLineButton","choiceList","addChoiceButton",
  "stateBeatList","stateEmpty","stateForm","stateBeatHeading","stateBeatId","stateBeatPriority","addConditionButton","conditionEditorList"
].forEach((id) => { els[id] = $(id); });

const identityInputs = [
  els.workingName, els.displayName, els.role, els.area, els.tags, els.background,
  els.personality, els.speechStyle, els.gameplayPurposes, els.narrativePurposes, els.designNotes,
];

function canonical(v) { return JSON.stringify(v); }
function clone(v) { return JSON.parse(JSON.stringify(v)); }
function setStatus(v) { els.statusText.textContent = v; }
function lineArray(v) { return String(v || "").split(/\r?\n/).map(x => x.trim()).filter(Boolean); }
function arrayText(v) { return Array.isArray(v) ? v.join("\n") : ""; }
function commaArray(v) {
  const out = [], seen = new Set();
  for (const raw of String(v || "").split(",")) {
    const item = raw.trim();
    if (item && !seen.has(item)) { seen.add(item); out.push(item); }
  }
  return out;
}
function hasHebrew(v) { return /[\u0590-\u05FF]/u.test(JSON.stringify(v)); }
function beatsOf(doc = state.documentValue) { return Array.isArray(doc?.interaction?.beats) ? doc.interaction.beats : []; }
function selectedBeat() {
  const beats = beatsOf();
  return Number.isInteger(state.selectedBeatIndex) ? (beats[state.selectedBeatIndex] || null) : null;
}

function conditionKind(c) {
  if (!c || typeof c !== "object" || Array.isArray(c)) return null;
  if (Object.prototype.hasOwnProperty.call(c, "fact")) return "fact";
  if (Object.prototype.hasOwnProperty.call(c, "npc_met")) return "npc_met";
  if (Object.prototype.hasOwnProperty.call(c, "dialogue_heard")) return "dialogue_heard";
  if (Object.prototype.hasOwnProperty.call(c, "item_owned")) return "item_owned";
  if (Object.prototype.hasOwnProperty.call(c, "item_equipped")) return "item_equipped";
  return null;
}

function conditionErrors(c, label) {
  const errors = [];
  const kind = conditionKind(c);
  if (!kind) return [`${label}: unsupported condition shape.`];
  if (typeof c.equals !== "boolean") errors.push(`${label}: equals must be true or false.`);

  if (kind === "fact" && (typeof c.fact !== "string" || !c.fact.trim())) {
    errors.push(`${label}: Fact requires a non-empty fact id.`);
  } else if (kind === "fact" && /^npc\..+\.met$/.test(c.fact)) {
    errors.push(`${label}: use the typed NPC Met condition instead of an npc.*.met fact.`);
  }
  if (kind === "npc_met" && (typeof c.npc_met !== "string" || !c.npc_met.startsWith("npc."))) {
    errors.push(`${label}: NPC Met requires an npc.* ContentId.`);
  }
  if (kind === "item_owned" && (typeof c.item_owned !== "string" || !c.item_owned.startsWith("item."))) {
    errors.push(`${label}: Item Owned requires an item.* ContentId.`);
  }
  if (kind === "item_equipped" && (typeof c.item_equipped !== "string" || !c.item_equipped.startsWith("item."))) {
    errors.push(`${label}: Item Equipped requires an item.* ContentId.`);
  }
  if (kind === "dialogue_heard") {
    const ref = c.dialogue_heard;
    if (!ref || typeof ref !== "object" || Array.isArray(ref)) {
      errors.push(`${label}: Dialogue Heard requires npc + beat.`);
    } else {
      if (typeof ref.npc !== "string" || !ref.npc.startsWith("npc.")) errors.push(`${label}: Dialogue Heard npc must use npc.*.`);
      if (typeof ref.beat !== "string" || !ref.beat.trim()) errors.push(`${label}: Dialogue Heard requires a beat id.`);
    }
  }
  return errors;
}

function dialogueErrors(doc) {
  const errors = [], ids = new Set();
  beatsOf(doc).forEach((beat, i) => {
    const name = beat?.id || `Beat ${i + 1}`;
    if (!beat || typeof beat !== "object") { errors.push(`Beat ${i + 1} must be an object.`); return; }
    if (!beat.id || typeof beat.id !== "string") errors.push(`Beat ${i + 1} requires an id.`);
    else if (ids.has(beat.id)) errors.push(`Duplicate beat id: ${beat.id}.`);
    else ids.add(beat.id);
    if (!beat.title || typeof beat.title !== "string") errors.push(`${name} requires a title.`);
    if (!Number.isInteger(beat.priority)) errors.push(`${name} priority must be an integer.`);
    if (typeof beat.entry !== "boolean") errors.push(`${name} requires an explicit Selection Role: ENTRY or CONTINUATION.`);

    const conditions = Array.isArray(beat.conditions) ? beat.conditions : [];
    if (!Array.isArray(beat.conditions)) errors.push(`${name} conditions must be an array.`);
    conditions.forEach((c, n) => errors.push(...conditionErrors(c, `${name} condition ${n + 1}`)));

    if (!Array.isArray(beat.lines) || !beat.lines.length) errors.push(`${name} requires at least one Dialogue Text.`);
    else beat.lines.forEach((line, n) => {
      if (!line?.text || typeof line.text !== "string") errors.push(`${name} Dialogue Text ${n + 1} requires text.`);
    });

    if (!Array.isArray(beat.choices)) errors.push(`${name} choices must be an array.`);
    else {
      const choiceIds = new Set();
      beat.choices.forEach((choice, n) => {
        if (!choice?.id || typeof choice.id !== "string") errors.push(`${name} choice ${n + 1} requires an id.`);
        else if (choiceIds.has(choice.id)) errors.push(`${name} has duplicate choice id: ${choice.id}.`);
        else choiceIds.add(choice.id);
        if (!choice?.text || typeof choice.text !== "string") errors.push(`${name} choice ${n + 1} requires player text.`);
        if (choice?.actions !== undefined && !Array.isArray(choice.actions)) {
          errors.push(`${name} choice ${n + 1} actions must be an array.`);
        } else {
          (choice.actions || []).forEach((action, actionIndex) => {
            const actionLabel = `${name} choice ${n + 1} action ${actionIndex + 1}`;
            const kind = actionKind(action);
            if (!kind) {
              errors.push(`${actionLabel}: unsupported action shape.`);
            } else if (kind === "set_fact") {
              const data = action.set_fact;
              if (!data || typeof data.fact !== "string" || !data.fact.trim()) errors.push(`${actionLabel}: Set Fact requires a fact id.`);
              else if (/^npc\..+\.met$/.test(data.fact)) errors.push(`${actionLabel}: use Mark NPC Met instead of Set Fact for npc.*.met.`);
              if (!data || typeof data.value !== "boolean") errors.push(`${actionLabel}: Set Fact value must be true or false.`);
            } else if (kind === "mark_npc_met") {
              if (typeof action.mark_npc_met !== "string" || !action.mark_npc_met.startsWith("npc.")) {
                errors.push(`${actionLabel}: Mark NPC Met requires an npc.* ContentId.`);
              }
            } else {
              const data = action[kind];
              if (!data || typeof data.item !== "string" || !data.item.startsWith("item.")) errors.push(`${actionLabel}: item must use item.*.`);
              if (!data || !Number.isInteger(data.quantity) || data.quantity < 1) errors.push(`${actionLabel}: quantity must be an integer >= 1.`);
            }
          });
        }
      });
    }
  });

  for (const beat of beatsOf(doc)) {
    for (const choice of Array.isArray(beat?.choices) ? beat.choices : []) {
      if (choice?.next && !ids.has(choice.next)) errors.push(`${beat.id || "Beat"} choice ${choice.id || "?"} targets missing beat: ${choice.next}.`);
    }
  }
  return errors;
}

function validationErrors(doc) {
  const errors = [];
  if (!doc || typeof doc !== "object") return ["NPC document must be an object."];
  if (!Number.isInteger(doc.schema_version) || doc.schema_version < 1) errors.push("schema_version must be >= 1.");
  if (typeof doc.id !== "string" || !doc.id.startsWith("npc.")) errors.push("id must use npc.*.");
  if (!doc.design || typeof doc.design !== "object") errors.push("design must be an object.");
  if (hasHebrew(doc)) errors.push("NPC authored content is English-only.");
  errors.push(...dialogueErrors(doc));
  return errors;
}

function markDirty() {
  state.dirty = Boolean(state.selectedPath) && canonical(state.documentValue) !== state.originalCanonical;
  els.saveButton.disabled = !state.selectedPath || !state.dirty;
  els.dirtyBadge.textContent = state.dirty ? "DIRTY" : "CLEAN";
  els.dirtyBadge.classList.toggle("dirty", state.dirty);
  els.dirtyBadge.classList.toggle("clean", !state.dirty);
}
function syncRaw() { els.jsonEditor.value = state.documentValue ? `${JSON.stringify(state.documentValue, null, 2)}\n` : ""; }
function parseRaw() {
  try { return { ok: true, value: JSON.parse(els.jsonEditor.value) }; }
  catch (error) { return { ok: false, error: String(error.message || error) }; }
}
function updateHeader() {
  els.documentTitle.textContent = state.documentValue?.design?.working_name || state.documentValue?.id || state.selectedPath || "No NPC selected";
}
function updateInspector(rawError = null) {
  if (!state.selectedPath) return;
  const beat = selectedBeat();
  const errors = rawError ? [rawError] : validationErrors(state.documentValue);
  els.summaryId.textContent = state.documentValue?.id || "-";
  els.summaryName.textContent = state.documentValue?.design?.working_name || "-";
  els.summaryBeatCount.textContent = String(beatsOf().length);
  els.summaryBeat.textContent = beat ? (beat.title || beat.id || "-") : "-";
  els.validationStatus.textContent = errors.length ? (rawError ? "Invalid JSON" : "Needs attention") : "N4 authoring contract OK";
  els.validationStatus.className = errors.length ? "value bad" : "value good";
  els.validationErrors.textContent = errors.length ? errors.join("\n") : "Dialogue + condition vocabulary + transitions + English-only check passed.";
}
function changed() { syncRaw(); updateHeader(); updateInspector(); markDirty(); }

function relField(labelText, key, value) {
  const label = document.createElement("label"); label.className = "field";
  const title = document.createElement("span"); title.textContent = labelText;
  const input = document.createElement("input"); input.className = "input relationship-input"; input.dataset.relationshipKey = key; input.value = value || "";
  input.addEventListener("input", applyIdentity); label.append(title, input); return label;
}
function renderRelationships(items) {
  els.relationshipList.replaceChildren();
  const values = Array.isArray(items) ? items : [];
  if (!values.length) {
    const empty = document.createElement("div"); empty.className = "empty-state"; empty.textContent = "No authored relationships yet."; els.relationshipList.appendChild(empty); return;
  }
  values.forEach((rel, index) => {
    const card = document.createElement("div"); card.className = "relationship-card"; card.dataset.index = String(index);
    const header = document.createElement("div"); header.className = "relationship-card-header";
    const title = document.createElement("div"); title.className = "relationship-title"; title.textContent = rel?.working_label || `Relationship ${index + 1}`;
    const remove = document.createElement("button"); remove.className = "icon-button"; remove.type = "button"; remove.textContent = "REMOVE";
    remove.addEventListener("click", () => { state.documentValue.relationships.splice(index, 1); renderRelationships(state.documentValue.relationships); changed(); });
    header.append(title, remove);
    const grid = document.createElement("div"); grid.className = "form-grid two";
    grid.append(relField("Working Label", "working_label", rel?.working_label)); grid.append(relField("Target ID", "target_id", rel?.target_id));
    const notesLabel = document.createElement("label"); notesLabel.className = "field";
    const notesTitle = document.createElement("span"); notesTitle.textContent = "Notes";
    const notes = document.createElement("textarea"); notes.className = "form-textarea short relationship-input"; notes.dataset.relationshipKey = "notes"; notes.value = rel?.notes || "";
    notes.addEventListener("input", applyIdentity); notesLabel.append(notesTitle, notes); card.append(header, grid, notesLabel); els.relationshipList.appendChild(card);
  });
}
function readRelationships() {
  const old = Array.isArray(state.documentValue?.relationships) ? state.documentValue.relationships : [];
  return [...els.relationshipList.querySelectorAll(".relationship-card")].map(card => {
    const i = Number(card.dataset.index); const base = old[i] && typeof old[i] === "object" ? clone(old[i]) : {};
    for (const input of card.querySelectorAll(".relationship-input")) {
      const key = input.dataset.relationshipKey; base[key] = key === "target_id" ? (input.value.trim() || null) : input.value.trim();
    } return base;
  });
}
function fillIdentity() {
  const doc = state.documentValue || {}, d = doc.design || {};
  els.authoredId.value = doc.id || ""; els.schemaVersion.value = doc.schema_version ?? "";
  els.workingName.value = d.working_name || ""; els.displayName.value = d.display_name || ""; els.role.value = d.role || "";
  els.area.value = d.area || ""; els.tags.value = Array.isArray(d.tags) ? d.tags.join(", ") : ""; els.background.value = d.background || "";
  els.personality.value = arrayText(d.personality); els.speechStyle.value = arrayText(d.speech_style);
  els.gameplayPurposes.value = arrayText(d.gameplay_purposes); els.narrativePurposes.value = arrayText(d.narrative_purposes);
  els.designNotes.value = arrayText(doc.notes); renderRelationships(doc.relationships);
}
function applyIdentity() {
  if (!state.documentValue) return;
  const d = state.documentValue.design || {};
  d.working_name = els.workingName.value.trim(); d.display_name = els.displayName.value.trim() || null; d.role = els.role.value.trim();
  d.area = els.area.value.trim(); d.tags = commaArray(els.tags.value); d.background = els.background.value.trim();
  d.personality = lineArray(els.personality.value); d.speech_style = lineArray(els.speechStyle.value);
  d.gameplay_purposes = lineArray(els.gameplayPurposes.value); d.narrative_purposes = lineArray(els.narrativePurposes.value);
  state.documentValue.design = d; state.documentValue.relationships = readRelationships(); state.documentValue.notes = lineArray(els.designNotes.value); changed();
}

function conditionSummary(c) {
  const kind = conditionKind(c);
  const expected = c?.equals === false ? "false" : "true";
  if (kind === "fact") return `FACT ${c.fact || "?"} = ${expected}`;
  if (kind === "npc_met") return `NPC MET ${c.npc_met || "?"} = ${expected}`;
  if (kind === "item_owned") return `ITEM OWNED ${c.item_owned || "?"} = ${expected}`;
  if (kind === "item_equipped") return `ITEM EQUIPPED ${c.item_equipped || "?"} = ${expected}`;
  if (kind === "dialogue_heard") return `DIALOGUE HEARD ${c.dialogue_heard?.npc || "?"} / ${c.dialogue_heard?.beat || "?"} = ${expected}`;
  return `UNSUPPORTED ${JSON.stringify(c)}`;
}
function renderConditionPreview(beat) {
  els.conditionPreview.replaceChildren();
  const conditions = Array.isArray(beat?.conditions) ? beat.conditions : [];
  els.beatConditionCount.textContent = String(conditions.length);
  if (!conditions.length) {
    const empty = document.createElement("div"); empty.className = "condition-empty"; empty.textContent = "No conditions."; els.conditionPreview.appendChild(empty); return;
  }
  for (const c of conditions) {
    const chip = document.createElement("div"); chip.className = "condition-chip"; chip.textContent = conditionSummary(c); els.conditionPreview.appendChild(chip);
  }
}

function makeBeatButton(beat, index, selectHandler) {
  const button = document.createElement("button"); button.className = "beat-item"; button.type = "button";
  if (index === state.selectedBeatIndex) button.classList.add("selected");
  const top = document.createElement("div"); top.className = "beat-item-top";
  const title = document.createElement("span"); title.className = "beat-item-title"; title.textContent = beat?.title || beat?.id || `Beat ${index + 1}`;
  const priority = document.createElement("span"); priority.className = "beat-priority"; priority.textContent = Number.isInteger(beat?.priority) ? `P${beat.priority}` : "P?";
  top.append(title, priority);
  const id = document.createElement("div"); id.className = "beat-item-id"; id.textContent = beat?.id || "(missing id)";
  const meta = document.createElement("div"); meta.className = "beat-item-meta";
  const role = beat?.entry === true ? "ENTRY" : (beat?.entry === false ? "CONT" : "UNSET");
  meta.textContent = `${role} · ${Array.isArray(beat?.conditions) ? beat.conditions.length : 0} conditions · ${Array.isArray(beat?.choices) ? beat.choices.length : 0} choices`;
  button.append(top, id, meta); button.addEventListener("click", () => selectHandler(index)); return button;
}
function renderBeatLists() {
  els.beatList.replaceChildren(); els.stateBeatList.replaceChildren();
  beatsOf().forEach((beat, index) => {
    els.beatList.appendChild(makeBeatButton(beat, index, selectBeat));
    els.stateBeatList.appendChild(makeBeatButton(beat, index, selectStateBeat));
  });
  if (!beatsOf().length) {
    for (const target of [els.beatList, els.stateBeatList]) {
      const empty = document.createElement("div"); empty.className = "empty-state"; empty.textContent = "No conversation beats authored."; target.appendChild(empty);
    }
  }
}
function transitionSelect(choice) {
  const select = document.createElement("select"); select.className = "input";
  const end = document.createElement("option"); end.value = ""; end.textContent = "END CONVERSATION"; select.appendChild(end);
  const ids = new Set();
  for (const beat of beatsOf()) {
    if (!beat?.id) continue; ids.add(beat.id);
    const option = document.createElement("option"); option.value = beat.id; option.textContent = beat.id; select.appendChild(option);
  }
  if (choice?.next && !ids.has(choice.next)) {
    const missing = document.createElement("option"); missing.value = choice.next; missing.textContent = `${choice.next} (MISSING)`; select.appendChild(missing);
  }
  select.value = choice?.next || ""; select.addEventListener("change", () => { choice.next = select.value || null; changed(); }); return select;
}
function renderLines(beat) {
  els.lineList.replaceChildren(); const items = Array.isArray(beat.lines) ? beat.lines : [];
  items.forEach((line, index) => {
    const card = document.createElement("div"); card.className = "dialogue-card";
    const header = document.createElement("div"); header.className = "dialogue-card-header";
    const title = document.createElement("div"); title.className = "dialogue-card-title"; title.textContent = `Dialogue Text ${index + 1}`;
    const remove = document.createElement("button"); remove.className = "icon-button"; remove.type = "button"; remove.textContent = "REMOVE";
    remove.addEventListener("click", () => { beat.lines.splice(index, 1); renderSelectedBeat(); changed(); }); header.append(title, remove);
    const text = document.createElement("textarea"); text.className = "form-textarea dialogue-text"; text.value = line?.text || "";
    text.addEventListener("input", () => { line.text = text.value; changed(); }); card.append(header, text);
    if (line?.voice || line?.animation) {
      const refs = document.createElement("div"); refs.className = "reserved-ref"; refs.textContent = `Preserved refs: voice=${line.voice || "-"} · animation=${line.animation || "-"}`; card.appendChild(refs);
    }
    els.lineList.appendChild(card);
  });
  if (!items.length) { const empty = document.createElement("div"); empty.className = "empty-state"; empty.textContent = "No Dialogue Text."; els.lineList.appendChild(empty); }
}
function nextChoiceId(beat) {
  const used = new Set((beat.choices || []).map(c => c?.id).filter(Boolean)); let i = 1; while (used.has(`choice_${i}`)) i += 1; return `choice_${i}`;
}
function actionKind(action) {
  if (!action || typeof action !== "object" || Array.isArray(action)) return null;
  if (Object.prototype.hasOwnProperty.call(action, "set_fact")) return "set_fact";
  if (Object.prototype.hasOwnProperty.call(action, "mark_npc_met")) return "mark_npc_met";
  if (Object.prototype.hasOwnProperty.call(action, "give_item")) return "give_item";
  if (Object.prototype.hasOwnProperty.call(action, "remove_item")) return "remove_item";
  return null;
}

function actionTypeSelect(action, choice, actionIndex, beat) {
  const select = document.createElement("select");
  select.className = "input";
  for (const [value, label] of [
    ["set_fact", "Set Fact"],
    ["mark_npc_met", "Mark NPC Met"],
    ["give_item", "Give Item"],
    ["remove_item", "Remove Item"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.appendChild(option);
  }

  const kind = actionKind(action);
  if (!kind) {
    const unsupported = document.createElement("option");
    unsupported.value = "__unsupported";
    unsupported.textContent = "Unsupported action";
    select.appendChild(unsupported);
    select.value = "__unsupported";
    select.disabled = true;
  } else {
    select.value = kind;
  }

  select.addEventListener("change", () => {
    const replacements = {
      set_fact: { set_fact: { fact: "", value: true } },
      mark_npc_met: { mark_npc_met: "npc." },
      give_item: { give_item: { item: "item.", quantity: 1 } },
      remove_item: { remove_item: { item: "item.", quantity: 1 } },
    };
    choice.actions[actionIndex] = replacements[select.value];
    renderChoices(beat);
    changed();
  });
  return select;
}

function renderChoiceActions(choice, beat) {
  const wrapper = document.createElement("div");
  wrapper.className = "choice-action-list";
  const actions = Array.isArray(choice.actions) ? choice.actions : [];

  actions.forEach((action, actionIndex) => {
    const card = document.createElement("div");
    card.className = "choice-action-card";

    const header = document.createElement("div");
    header.className = "choice-action-card-header";
    const label = document.createElement("div");
    label.className = "action-index";
    label.textContent = `Action ${actionIndex + 1}`;

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "icon-button";
    remove.textContent = "REMOVE";
    remove.addEventListener("click", () => {
      choice.actions.splice(actionIndex, 1);
      if (!choice.actions.length) delete choice.actions;
      renderChoices(beat);
      changed();
    });
    header.append(label, remove);

    const typeLabel = document.createElement("label");
    typeLabel.className = "field";
    const typeTitle = document.createElement("span");
    typeTitle.textContent = "Action Type";
    typeLabel.append(typeTitle, actionTypeSelect(action, choice, actionIndex, beat));

    card.append(header, typeLabel);

    const kind = actionKind(action);
    if (kind === "set_fact") {
      const data = action.set_fact;
      const grid = document.createElement("div");
      grid.className = "choice-grid";

      const factLabel = document.createElement("label");
      factLabel.className = "field";
      const factTitle = document.createElement("span");
      factTitle.textContent = "Fact ID";
      const factInput = document.createElement("input");
      factInput.className = "input monospace-input";
      factInput.placeholder = "welcome.workshop.package_taken";
      factInput.value = data?.fact || "";
      factInput.addEventListener("input", () => {
        data.fact = factInput.value;
        changed();
      });
      factLabel.append(factTitle, factInput);

      const valueLabel = document.createElement("label");
      valueLabel.className = "field";
      const valueTitle = document.createElement("span");
      valueTitle.textContent = "Value";
      const valueSelect = document.createElement("select");
      valueSelect.className = "input";
      for (const [value, text] of [["true", "TRUE"], ["false", "FALSE"]]) {
        const option = document.createElement("option");
        option.value = value;
        option.textContent = text;
        valueSelect.appendChild(option);
      }
      valueSelect.value = data?.value === false ? "false" : "true";
      valueSelect.addEventListener("change", () => {
        data.value = valueSelect.value === "true";
        changed();
      });
      valueLabel.append(valueTitle, valueSelect);
      grid.append(factLabel, valueLabel);
      card.appendChild(grid);
    } else if (kind === "mark_npc_met") {
      const npcLabel = document.createElement("label");
      npcLabel.className = "field";
      const npcTitle = document.createElement("span");
      npcTitle.textContent = "NPC ContentId";
      const npcInput = document.createElement("input");
      npcInput.className = "input monospace-input";
      npcInput.placeholder = "npc.welcome.workshop_craftsperson";
      npcInput.value = action.mark_npc_met || "";
      npcInput.addEventListener("input", () => {
        action.mark_npc_met = npcInput.value;
        changed();
      });
      npcLabel.append(npcTitle, npcInput);
      card.appendChild(npcLabel);
    } else if (kind === "give_item" || kind === "remove_item") {
      const data = action[kind];
      const grid = document.createElement("div");
      grid.className = "choice-grid";

      const itemLabel = document.createElement("label");
      itemLabel.className = "field";
      const itemTitle = document.createElement("span");
      itemTitle.textContent = "Item ContentId";
      const itemInput = document.createElement("input");
      itemInput.className = "input monospace-input";
      itemInput.placeholder = "item.workshop.package";
      itemInput.value = data?.item || "";
      itemInput.addEventListener("input", () => {
        data.item = itemInput.value;
        changed();
      });
      itemLabel.append(itemTitle, itemInput);

      const quantityLabel = document.createElement("label");
      quantityLabel.className = "field";
      const quantityTitle = document.createElement("span");
      quantityTitle.textContent = "Quantity";
      const quantityInput = document.createElement("input");
      quantityInput.className = "input";
      quantityInput.type = "number";
      quantityInput.min = "1";
      quantityInput.step = "1";
      quantityInput.value = Number.isInteger(data?.quantity) ? data.quantity : 1;
      quantityInput.addEventListener("input", () => {
        const value = Number(quantityInput.value);
        data.quantity = Number.isInteger(value) && value > 0 ? value : null;
        changed();
      });
      quantityLabel.append(quantityTitle, quantityInput);
      grid.append(itemLabel, quantityLabel);
      card.appendChild(grid);
    } else {
      const raw = document.createElement("pre");
      raw.className = "condition-raw";
      raw.textContent = JSON.stringify(action, null, 2);
      card.appendChild(raw);
    }

    wrapper.appendChild(card);
  });

  if (!actions.length) {
    const empty = document.createElement("div");
    empty.className = "choice-actions-empty";
    empty.textContent = "No actions. This choice only changes conversation flow.";
    wrapper.appendChild(empty);
  }

  return wrapper;
}

function renderChoices(beat) {
  els.choiceList.replaceChildren(); const choices = Array.isArray(beat.choices) ? beat.choices : [];
  choices.forEach((choice, index) => {
    const card = document.createElement("div"); card.className = "dialogue-card";
    const header = document.createElement("div"); header.className = "dialogue-card-header";
    const title = document.createElement("div"); title.className = "dialogue-card-title"; title.textContent = `Player Choice ${index + 1}`;
    const remove = document.createElement("button"); remove.className = "icon-button"; remove.type = "button"; remove.textContent = "REMOVE";
    remove.addEventListener("click", () => { beat.choices.splice(index, 1); renderSelectedBeat(); renderBeatLists(); changed(); }); header.append(title, remove);
    const grid = document.createElement("div"); grid.className = "choice-grid";
    const idLabel = document.createElement("label"); idLabel.className = "field";
    const idTitle = document.createElement("span"); idTitle.textContent = "Choice ID";
    const idInput = document.createElement("input"); idInput.className = "input monospace-input"; idInput.value = choice?.id || "";
    idInput.addEventListener("input", () => { choice.id = idInput.value; changed(); }); idLabel.append(idTitle, idInput);
    const nextLabel = document.createElement("label"); nextLabel.className = "field";
    const nextTitle = document.createElement("span"); nextTitle.textContent = "Next Beat / End"; nextLabel.append(nextTitle, transitionSelect(choice)); grid.append(idLabel, nextLabel);
    const textLabel = document.createElement("label"); textLabel.className = "field";
    const textTitle = document.createElement("span"); textTitle.textContent = "Player Text";
    const textInput = document.createElement("textarea"); textInput.className = "form-textarea short"; textInput.value = choice?.text || "";
    textInput.addEventListener("input", () => { choice.text = textInput.value; changed(); }); textLabel.append(textTitle, textInput);

    const actionsSection = document.createElement("div");
    actionsSection.className = "choice-actions";
    const actionsHeader = document.createElement("div");
    actionsHeader.className = "choice-actions-header";
    const actionsTitle = document.createElement("div");
    actionsTitle.className = "dialogue-card-title";
    const actionCount = Array.isArray(choice.actions) ? choice.actions.length : 0;
    actionsTitle.textContent = `Actions (${actionCount})`;
    const addAction = document.createElement("button");
    addAction.className = "button compact";
    addAction.type = "button";
    addAction.textContent = "+ ACTION";
    addAction.addEventListener("click", () => {
      if (!Array.isArray(choice.actions)) choice.actions = [];
      choice.actions.push({ set_fact: { fact: "", value: true } });
      renderChoices(beat);
      changed();
    });
    actionsHeader.append(actionsTitle, addAction);
    actionsSection.appendChild(actionsHeader);
    actionsSection.appendChild(renderChoiceActions(choice, beat));

    card.append(header, grid, textLabel, actionsSection); els.choiceList.appendChild(card);
  });
  if (!choices.length) { const empty = document.createElement("div"); empty.className = "empty-state"; empty.textContent = "No choices. Conversation ends after Dialogue Text."; els.choiceList.appendChild(empty); }
}
function renderSelectedBeat() {
  const beat = selectedBeat(); els.beatEmpty.classList.toggle("hidden", Boolean(beat)); els.beatForm.classList.toggle("hidden", !beat);
  if (!beat) { updateInspector(); return; }
  els.beatEditorHeading.textContent = beat.title || beat.id || "Conversation Beat"; els.beatId.value = beat.id || "";
  els.beatTitle.value = beat.title || ""; els.beatPriority.value = Number.isInteger(beat.priority) ? beat.priority : "";
  els.beatEntryMode.value = beat.entry === true ? "entry" : (beat.entry === false ? "continuation" : "");
  els.beatPool.textContent = beat.pool || "(not set)"; els.beatNotes.value = typeof beat.notes === "string" ? beat.notes : "";
  renderConditionPreview(beat); renderLines(beat); renderChoices(beat); updateInspector();
}

function conditionTypeSelect(condition, index) {
  const select = document.createElement("select"); select.className = "input";
  const options = [
    ["fact","Fact"],["npc_met","NPC Met"],["dialogue_heard","Dialogue Heard"],["item_owned","Item Owned"],["item_equipped","Item Equipped"]
  ];
  for (const [value, label] of options) {
    const option = document.createElement("option"); option.value = value; option.textContent = label; select.appendChild(option);
  }
  const kind = conditionKind(condition);
  if (!kind) {
    const unsupported = document.createElement("option"); unsupported.value = "__unsupported"; unsupported.textContent = "Unsupported condition"; select.appendChild(unsupported);
    select.value = "__unsupported"; select.disabled = true;
  } else select.value = kind;

  select.addEventListener("change", () => {
    const expected = typeof condition.equals === "boolean" ? condition.equals : true;
    const kindValue = select.value;
    const replacements = {
      fact: { fact: "", equals: expected },
      npc_met: { npc_met: "npc.", equals: expected },
      dialogue_heard: { dialogue_heard: { npc: "npc.", beat: "" }, equals: expected },
      item_owned: { item_owned: "item.", equals: expected },
      item_equipped: { item_equipped: "item.", equals: expected },
    };
    selectedBeat().conditions[index] = replacements[kindValue];
    renderConditionEditor(); renderConditionPreview(selectedBeat()); changed();
  });
  return select;
}
function expectedSelect(condition) {
  const select = document.createElement("select"); select.className = "input";
  for (const [value, label] of [["true","TRUE"],["false","FALSE"]]) {
    const option = document.createElement("option"); option.value = value; option.textContent = label; select.appendChild(option);
  }
  select.value = condition.equals === false ? "false" : "true";
  select.addEventListener("change", () => { condition.equals = select.value === "true"; renderConditionPreview(selectedBeat()); changed(); });
  return select;
}
function conditionReferenceFields(condition) {
  const kind = conditionKind(condition);
  const wrapper = document.createElement("div"); wrapper.className = "condition-reference";
  if (!kind) {
    const raw = document.createElement("pre"); raw.className = "condition-raw"; raw.textContent = JSON.stringify(condition, null, 2); wrapper.appendChild(raw); return wrapper;
  }

  function field(labelText, value, onInput, placeholder = "") {
    const label = document.createElement("label"); label.className = "field";
    const title = document.createElement("span"); title.textContent = labelText;
    const input = document.createElement("input"); input.className = "input monospace-input"; input.value = value || ""; input.placeholder = placeholder;
    input.addEventListener("input", () => { onInput(input.value); renderConditionPreview(selectedBeat()); changed(); });
    label.append(title, input); return label;
  }

  if (kind === "fact") wrapper.appendChild(field("Fact ID", condition.fact, v => condition.fact = v, "welcome.workshop.package_needed"));
  if (kind === "npc_met") wrapper.appendChild(field("NPC ContentId", condition.npc_met, v => condition.npc_met = v, "npc.welcome.workshop_craftsperson"));
  if (kind === "item_owned") wrapper.appendChild(field("Item ContentId", condition.item_owned, v => condition.item_owned = v, "item.workshop.package"));
  if (kind === "item_equipped") wrapper.appendChild(field("Item ContentId", condition.item_equipped, v => condition.item_equipped = v, "item.workshop.boots"));
  if (kind === "dialogue_heard") {
    const grid = document.createElement("div"); grid.className = "form-grid two";
    grid.appendChild(field("NPC ContentId", condition.dialogue_heard?.npc, v => condition.dialogue_heard.npc = v, "npc.welcome.workshop_craftsperson"));
    grid.appendChild(field("Beat ID", condition.dialogue_heard?.beat, v => condition.dialogue_heard.beat = v, "intro"));
    wrapper.appendChild(grid);
  }
  return wrapper;
}
function renderConditionEditor() {
  els.conditionEditorList.replaceChildren(); const beat = selectedBeat();
  if (!beat) return;
  if (!Array.isArray(beat.conditions)) beat.conditions = [];

  beat.conditions.forEach((condition, index) => {
    const card = document.createElement("div"); card.className = "condition-editor-card";
    const header = document.createElement("div"); header.className = "condition-editor-header";
    const title = document.createElement("div"); title.className = "dialogue-card-title"; title.textContent = `Condition ${index + 1}`;
    const remove = document.createElement("button"); remove.className = "icon-button"; remove.type = "button"; remove.textContent = "REMOVE";
    remove.addEventListener("click", () => { beat.conditions.splice(index, 1); renderConditionEditor(); renderConditionPreview(beat); renderBeatLists(); changed(); });
    header.append(title, remove);

    const controls = document.createElement("div"); controls.className = "condition-controls";
    const typeLabel = document.createElement("label"); typeLabel.className = "field";
    const typeTitle = document.createElement("span"); typeTitle.textContent = "Check";
    typeLabel.append(typeTitle, conditionTypeSelect(condition, index));
    const expectedLabel = document.createElement("label"); expectedLabel.className = "field";
    const expectedTitle = document.createElement("span"); expectedTitle.textContent = "Expected";
    expectedLabel.append(expectedTitle, expectedSelect(condition));
    controls.append(typeLabel, expectedLabel);

    card.append(header, controls, conditionReferenceFields(condition));
    els.conditionEditorList.appendChild(card);
  });

  if (!beat.conditions.length) {
    const empty = document.createElement("div"); empty.className = "empty-state"; empty.textContent = "No conditions. This beat has no N4 state gate."; els.conditionEditorList.appendChild(empty);
  }
}
function renderStateSelectedBeat() {
  const beat = selectedBeat(); els.stateEmpty.classList.toggle("hidden", Boolean(beat)); els.stateForm.classList.toggle("hidden", !beat);
  if (!beat) return;
  els.stateBeatHeading.textContent = beat.title || beat.id || "Beat Conditions"; els.stateBeatId.textContent = beat.id || "-";
  els.stateBeatPriority.textContent = Number.isInteger(beat.priority) ? `P${beat.priority}` : "P?";
  renderConditionEditor(); updateInspector();
}

function selectBeat(index) { state.selectedBeatIndex = index; renderBeatLists(); renderSelectedBeat(); renderStateSelectedBeat(); }
function selectStateBeat(index) { state.selectedBeatIndex = index; renderBeatLists(); renderSelectedBeat(); renderStateSelectedBeat(); }
function ensureBeats() {
  if (!state.documentValue.interaction || typeof state.documentValue.interaction !== "object") state.documentValue.interaction = {};
  if (!Array.isArray(state.documentValue.interaction.beats)) state.documentValue.interaction.beats = [];
  return state.documentValue.interaction.beats;
}
function newBeatId() {
  const ids = new Set(beatsOf().map(b => b?.id).filter(Boolean)); let i = 1; while (ids.has(`beat_${i}`)) i += 1; return `beat_${i}`;
}
function renameBeat(oldId, newId) {
  if (!oldId || oldId === newId) return;
  for (const beat of beatsOf()) for (const choice of Array.isArray(beat?.choices) ? beat.choices : []) if (choice.next === oldId) choice.next = newId;
}
function renderDialogueAndState() {
  const beats = beatsOf();
  if (!beats.length) state.selectedBeatIndex = null;
  else if (!Number.isInteger(state.selectedBeatIndex) || state.selectedBeatIndex >= beats.length) state.selectedBeatIndex = 0;
  renderBeatLists(); renderSelectedBeat(); renderStateSelectedBeat();
}
function switchSurface(surface) {
  if (surface === state.activeSurface) return;
  if (state.activeSurface === "shell") {
    const parsed = parseRaw();
    if (!parsed.ok) { window.alert("Raw JSON is invalid. Fix it before leaving Shell."); return; }
    state.documentValue = parsed.value; fillIdentity(); renderDialogueAndState();
  } else if (state.activeSurface === "identity") applyIdentity();

  if (surface === "identity") fillIdentity();
  if (surface === "dialogue" || surface === "state") renderDialogueAndState();
  if (surface === "shell") syncRaw();

  state.activeSurface = surface;
  els.identitySurface.classList.toggle("active", surface === "identity");
  els.dialogueSurface.classList.toggle("active", surface === "dialogue");
  els.stateSurface.classList.toggle("active", surface === "state");
  els.shellSurface.classList.toggle("active", surface === "shell");
  for (const tab of document.querySelectorAll(".future-tab[data-surface]")) tab.classList.toggle("active", tab.dataset.surface === surface);
}
function renderList() {
  const needle = els.filterInput.value.trim().toLowerCase(); els.npcList.replaceChildren();
  const filtered = state.items.filter(item => !needle || [item.id,item.working_name,item.area,item.path].filter(Boolean).join(" ").toLowerCase().includes(needle));
  for (const item of filtered) {
    const button = document.createElement("button"); button.type = "button"; button.className = "npc-item";
    if (item.path === state.selectedPath) button.classList.add("selected"); if (!item.valid) button.classList.add("invalid");
    const title = document.createElement("div"); title.className = "npc-item-title"; title.textContent = item.working_name || item.id || "(invalid NPC JSON)";
    const path = document.createElement("div"); path.className = "npc-item-path"; path.textContent = item.path;
    button.append(title, path); button.addEventListener("click", () => openNpc(item.path).catch(handleError)); els.npcList.appendChild(button);
  }
}
async function refreshList() {
  const response = await fetch("/api/npcs"); if (!response.ok) throw new Error(`NPC list failed: HTTP ${response.status}`);
  state.items = (await response.json()).items || []; renderList();
}
async function openNpc(path) {
  if (path === state.selectedPath && !state.dirty) return;
  if (state.dirty && !window.confirm("Discard unsaved NPC Lab changes?")) return;
  const response = await fetch(`/api/npc?path=${encodeURIComponent(path)}`); const payload = await response.json();
  if (!response.ok) throw new Error(payload.error || "Open failed.");
  state.selectedPath = payload.path; state.documentValue = payload.document; state.originalCanonical = canonical(payload.document); state.selectedBeatIndex = null;
  els.jsonEditor.disabled = false; els.documentPath.textContent = payload.path;
  fillIdentity(); renderDialogueAndState(); syncRaw(); updateHeader(); updateInspector(); markDirty(); renderList();
}
async function saveCurrent() {
  if (!state.selectedPath) return;
  if (state.activeSurface === "shell") {
    const parsed = parseRaw(); if (!parsed.ok) { updateInspector(parsed.error); return; } state.documentValue = parsed.value;
  } else if (state.activeSurface === "identity") applyIdentity();

  const errors = validationErrors(state.documentValue);
  if (errors.length) { updateInspector(); window.alert(`Cannot save:\n${errors.join("\n")}`); return; }

  const response = await fetch(`/api/npc?path=${encodeURIComponent(state.selectedPath)}`, {
    method:"POST", headers:{"Content-Type":"application/json; charset=utf-8"}, body:JSON.stringify(state.documentValue)
  });
  const payload = await response.json();
  if (!response.ok) {
    const details = (payload.validation_errors || []).join(" | ");
    throw new Error((payload.error || "Save failed.") + (details ? ` ${details}` : ""));
  }
  state.originalCanonical = canonical(state.documentValue); syncRaw(); markDirty(); await refreshList(); setStatus(`Saved ${state.selectedPath}.`);
}
async function newNpc() {
  if (state.dirty && !window.confirm("Discard unsaved NPC Lab changes?")) return;
  const id = window.prompt("Authored NPC ID:", "npc.welcome."); if (!id) return;
  const area = window.prompt("Area / folder:", id.split(".")[1] || "unassigned"); if (area === null) return;
  const response = await fetch("/api/new", { method:"POST", headers:{"Content-Type":"application/json; charset=utf-8"}, body:JSON.stringify({id,area}) });
  const payload = await response.json(); if (!response.ok) throw new Error(payload.error || "Create failed.");
  await refreshList(); await openNpc(payload.path); switchSurface("dialogue");
}
function handleError(error) { console.error(error); setStatus(`ERROR: ${error.message}`); window.alert(error.message); }

for (const input of identityInputs) input.addEventListener("input", applyIdentity);
els.addRelationshipButton.addEventListener("click", () => {
  if (!state.documentValue) return; if (!Array.isArray(state.documentValue.relationships)) state.documentValue.relationships = [];
  state.documentValue.relationships.push({working_label:"",target_id:null,notes:""}); renderRelationships(state.documentValue.relationships); changed();
});
els.addBeatButton.addEventListener("click", () => {
  const beats = ensureBeats(); beats.push({id:newBeatId(),title:"New beat",priority:0,entry:true,pool:"once",conditions:[],lines:[{text:"",voice:null,animation:null}],choices:[],notes:""});
  state.selectedBeatIndex = beats.length - 1; renderDialogueAndState(); changed();
});
els.removeBeatButton.addEventListener("click", () => {
  const beat = selectedBeat(); if (!beat || !window.confirm(`Remove beat '${beat.id || "unnamed"}'?`)) return;
  beatsOf().splice(state.selectedBeatIndex,1); state.selectedBeatIndex = beatsOf().length ? Math.min(state.selectedBeatIndex,beatsOf().length-1) : null; renderDialogueAndState(); changed();
});
els.beatId.addEventListener("input", () => {
  const beat = selectedBeat(); if (!beat) return; const old = beat.id; beat.id = els.beatId.value; renameBeat(old,beat.id); renderDialogueAndState(); changed();
});
els.beatTitle.addEventListener("input", () => { const beat=selectedBeat(); if(!beat)return; beat.title=els.beatTitle.value; renderDialogueAndState(); changed(); });
els.beatPriority.addEventListener("input", () => { const beat=selectedBeat(); if(!beat)return; const n=Number(els.beatPriority.value); beat.priority=Number.isInteger(n)?n:null; renderDialogueAndState(); changed(); });
els.beatEntryMode.addEventListener("change", () => {
  const beat = selectedBeat();
  if (!beat) return;
  if (els.beatEntryMode.value === "entry") beat.entry = true;
  else if (els.beatEntryMode.value === "continuation") beat.entry = false;
  else delete beat.entry;
  renderDialogueAndState();
  changed();
});
els.beatNotes.addEventListener("input", () => { const beat=selectedBeat(); if(!beat)return; beat.notes=els.beatNotes.value; changed(); });
els.addLineButton.addEventListener("click", () => { const beat=selectedBeat(); if(!beat)return; if(!Array.isArray(beat.lines))beat.lines=[]; beat.lines.push({text:"",voice:null,animation:null}); renderLines(beat); changed(); });
els.addChoiceButton.addEventListener("click", () => { const beat=selectedBeat(); if(!beat)return; if(!Array.isArray(beat.choices))beat.choices=[]; beat.choices.push({id:nextChoiceId(beat),text:"",next:null}); renderChoices(beat); renderBeatLists(); changed(); });
els.editConditionsButton.addEventListener("click", () => switchSurface("state"));
els.addConditionButton.addEventListener("click", () => {
  const beat=selectedBeat(); if(!beat)return; if(!Array.isArray(beat.conditions))beat.conditions=[];
  beat.conditions.push({fact:"",equals:true}); renderConditionEditor(); renderConditionPreview(beat); renderBeatLists(); changed();
});
els.jsonEditor.addEventListener("input", () => {
  const parsed=parseRaw(); if(!parsed.ok){state.dirty=true;els.saveButton.disabled=true;updateInspector(parsed.error);return;}
  state.documentValue=parsed.value; updateHeader(); updateInspector(); markDirty();
});
els.filterInput.addEventListener("input",renderList);
els.newButton.addEventListener("click",()=>newNpc().catch(handleError));
els.saveButton.addEventListener("click",()=>saveCurrent().catch(handleError));
els.reloadButton.addEventListener("click",()=>{
  if(state.dirty&&!window.confirm("Discard unsaved NPC Lab changes?"))return;
  const selected=state.selectedPath; state.dirty=false; state.selectedPath=null;
  refreshList().then(()=>selected?openNpc(selected):null).catch(handleError);
});
for(const tab of document.querySelectorAll(".future-tab[data-surface]"))tab.addEventListener("click",()=>switchSurface(tab.dataset.surface));
window.addEventListener("keydown",event=>{if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==="s"){event.preventDefault();saveCurrent().catch(handleError);}});
window.addEventListener("beforeunload",event=>{if(state.dirty){event.preventDefault();event.returnValue="";}});
refreshList().then(()=>state.items.length?openNpc(state.items[0].path):null).catch(handleError);
