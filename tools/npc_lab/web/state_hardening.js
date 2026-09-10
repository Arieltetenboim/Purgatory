(() => {
  const CONDITION_KEYS = ["fact", "npc_met", "dialogue_heard", "item_owned", "item_equipped"];

  function supportedConditionKeys(condition) {
    if (!condition || typeof condition !== "object" || Array.isArray(condition)) return [];
    return CONDITION_KEYS.filter((key) => Object.prototype.hasOwnProperty.call(condition, key));
  }

  function conditionReferenceText(condition, kind) {
    if (kind === "fact") return `FACT ${condition.fact || "?"}`;
    if (kind === "npc_met") return `NPC MET ${condition.npc_met || "?"}`;
    if (kind === "item_owned") return `ITEM OWNED ${condition.item_owned || "?"}`;
    if (kind === "item_equipped") return `ITEM EQUIPPED ${condition.item_equipped || "?"}`;
    if (kind === "dialogue_heard") {
      return `DIALOGUE HEARD ${condition.dialogue_heard?.npc || "?"} / ${condition.dialogue_heard?.beat || "?"}`;
    }
    return "INVALID CONDITION";
  }

  const baseConditionErrors = conditionErrors;
  conditionErrors = function n7ConditionErrors(condition, label) {
    if (!condition || typeof condition !== "object" || Array.isArray(condition)) {
      return [`${label}: condition must be an object.`];
    }
    const keys = supportedConditionKeys(condition);
    if (keys.length !== 1) {
      return [`${label}: choose exactly one supported check; found ${keys.length}.`];
    }
    return baseConditionErrors(condition, label);
  };

  conditionSummary = function n7ConditionSummary(condition) {
    const keys = supportedConditionKeys(condition);
    if (keys.length !== 1) return `INVALID CONDITION · ${keys.length} supported checks`;
    const reference = conditionReferenceText(condition, keys[0]);
    if (typeof condition.equals !== "boolean") return `${reference} = INVALID EXPECTED VALUE`;
    return `${reference} = ${condition.equals ? "true" : "false"}`;
  };

  expectedSelect = function n7ExpectedSelect(condition) {
    const select = document.createElement("select");
    select.className = "input";

    const unset = document.createElement("option");
    unset.value = "";
    unset.textContent = "SELECT TRUE / FALSE";
    unset.disabled = true;
    select.appendChild(unset);

    for (const [value, label] of [["true", "TRUE"], ["false", "FALSE"]]) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }

    select.value = typeof condition.equals === "boolean" ? String(condition.equals) : "";
    select.addEventListener("change", () => {
      condition.equals = select.value === "true";
      renderConditionPreview(selectedBeat());
      renderConditionEditor();
      changed();
    });
    return select;
  };

  conditionTypeSelect = function n7ConditionTypeSelect(condition, index) {
    const select = document.createElement("select");
    select.className = "input";
    const options = [
      ["fact", "Fact"],
      ["npc_met", "NPC Met"],
      ["dialogue_heard", "Dialogue Heard"],
      ["item_owned", "Item Owned"],
      ["item_equipped", "Item Equipped"],
    ];

    const keys = supportedConditionKeys(condition);
    if (keys.length !== 1) {
      const repair = document.createElement("option");
      repair.value = "";
      repair.textContent = "SELECT CHECK TO REPAIR";
      repair.disabled = true;
      select.appendChild(repair);
    }

    for (const [value, label] of options) {
      const option = document.createElement("option");
      option.value = value;
      option.textContent = label;
      select.appendChild(option);
    }

    select.value = keys.length === 1 ? keys[0] : "";
    select.addEventListener("change", () => {
      if (!select.value) return;
      const expected = typeof condition.equals === "boolean" ? condition.equals : null;
      const replacements = {
        fact: { fact: "", equals: expected },
        npc_met: { npc_met: "npc.", equals: expected },
        dialogue_heard: { dialogue_heard: { npc: "npc.", beat: "" }, equals: expected },
        item_owned: { item_owned: "item.", equals: expected },
        item_equipped: { item_equipped: "item.", equals: expected },
      };
      selectedBeat().conditions[index] = replacements[select.value];
      renderConditionEditor();
      renderConditionPreview(selectedBeat());
      changed();
    });
    return select;
  };

  const baseConditionReferenceFields = conditionReferenceFields;
  conditionReferenceFields = function n7ConditionReferenceFields(condition) {
    const keys = supportedConditionKeys(condition);
    if (keys.length === 1) return baseConditionReferenceFields(condition);

    const wrapper = document.createElement("div");
    wrapper.className = "condition-reference";
    const note = document.createElement("div");
    note.className = "reserved-ref";
    note.textContent = "This condition cannot be represented safely. Choose one Check above to replace it.";
    const raw = document.createElement("pre");
    raw.className = "condition-raw";
    raw.textContent = JSON.stringify(condition, null, 2);
    wrapper.append(note, raw);
    return wrapper;
  };

  const baseRenderConditionEditor = renderConditionEditor;
  renderConditionEditor = function n7RenderConditionEditor() {
    baseRenderConditionEditor();
    const beat = selectedBeat();
    if (!beat || !Array.isArray(beat.conditions)) return;

    const cards = [...els.conditionEditorList.querySelectorAll(".condition-editor-card")];
    beat.conditions.forEach((condition, index) => {
      const card = cards[index];
      if (!card) return;
      const errors = conditionErrors(condition, `Condition ${index + 1}`);
      const status = document.createElement("div");
      status.className = errors.length ? "condition-raw" : "reserved-ref";
      status.textContent = errors.length ? errors.join(" ") : "Valid condition.";
      card.appendChild(status);
    });
  };

  const sliceBadge = document.querySelector(".tool-name .slice");
  if (sliceBadge) sliceBadge.textContent = "N7";

  if (selectedBeat()) {
    renderConditionEditor();
    renderConditionPreview(selectedBeat());
    updateInspector();
  }
})();
