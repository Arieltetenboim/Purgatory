(() => {
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/presentation_cues.css";
  document.head.appendChild(css);

  const workspaceBody = document.querySelector(".workspace-body");
  const tab = [...document.querySelectorAll(".future-tab")].find(
    (candidate) => candidate.textContent.trim() === "PRESENTATION"
  );
  if (!workspaceBody || !tab) return;

  tab.disabled = false;

  const surface = document.createElement("section");
  surface.id = "presentationSurface";
  surface.className = "surface";
  surface.innerHTML = `
    <div class="presentation-cues">
      <aside class="presentation-beats pane-internal">
        <div class="section-kicker">N9a</div>
        <h2>Presentation Cues</h2>
        <p class="presentation-help">Animation is optional. No animation leaves the NPC in its normal presentation state.</p>
        <div id="presentationCatalogStatus" class="presentation-catalog-status">Animation catalog not loaded.</div>
        <div id="presentationBeatList" class="presentation-beat-list"></div>
      </aside>
      <section class="presentation-lines pane-internal">
        <div class="presentation-heading">
          <div>
            <div class="section-kicker">SELECTED BEAT</div>
            <h2 id="presentationBeatTitle">Select a beat</h2>
            <div id="presentationBeatId" class="presentation-beat-id"></div>
          </div>
          <div class="presentation-rule">Optional cue · presentation-only</div>
        </div>
        <div id="presentationLineList" class="presentation-line-list"></div>
      </section>
    </div>`;
  workspaceBody.appendChild(surface);

  const byId = (id) => document.getElementById(id);
  const ui = {
    catalogStatus: byId("presentationCatalogStatus"),
    beatList: byId("presentationBeatList"),
    beatTitle: byId("presentationBeatTitle"),
    beatId: byId("presentationBeatId"),
    lineList: byId("presentationLineList"),
  };

  let animationCatalog = [];
  let catalogLoaded = false;
  let catalogError = null;

  function normalizedAnimation(line) {
    return typeof line?.animation === "string" && line.animation.trim()
      ? line.animation.trim()
      : null;
  }

  async function loadCatalog() {
    if (catalogLoaded || catalogError) return;
    try {
      const response = await fetch("/api/catalog?kind=animation");
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error || "Animation catalog failed.");
      animationCatalog = Array.isArray(payload.items) ? payload.items : [];
      catalogLoaded = true;
      ui.catalogStatus.textContent = `${animationCatalog.length} authored animation${animationCatalog.length === 1 ? "" : "s"} available.`;
      if (state.activeSurface === "presentation") renderPresentation();
    } catch (error) {
      console.error(error);
      catalogError = error;
      ui.catalogStatus.textContent = `Animation catalog unavailable: ${error.message}`;
      if (state.activeSurface === "presentation") renderPresentation();
    }
  }

  function beatCueCount(beat) {
    const lines = Array.isArray(beat?.lines) ? beat.lines : [];
    return lines.filter((line) => normalizedAnimation(line)).length;
  }

  function renderBeatList() {
    ui.beatList.replaceChildren();
    const beats = beatsOf();
    if (!beats.length) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "No conversation beats authored.";
      ui.beatList.appendChild(empty);
      return;
    }

    beats.forEach((beat, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "presentation-beat";
      if (index === state.selectedBeatIndex) button.classList.add("selected");

      const title = document.createElement("div");
      title.className = "presentation-beat-title";
      title.textContent = beat?.title || beat?.id || `Beat ${index + 1}`;

      const meta = document.createElement("div");
      meta.className = "presentation-beat-meta";
      const lineCount = Array.isArray(beat?.lines) ? beat.lines.length : 0;
      const cueCount = beatCueCount(beat);
      meta.textContent = `${beat?.id || "(missing id)"} · ${lineCount} line${lineCount === 1 ? "" : "s"} · ${cueCount} cue${cueCount === 1 ? "" : "s"}`;

      button.append(title, meta);
      button.addEventListener("click", () => {
        state.selectedBeatIndex = index;
        renderDialogueAndState();
        renderPresentation();
      });
      ui.beatList.appendChild(button);
    });
  }

  function animationSelect(line) {
    const select = document.createElement("select");
    select.className = "input presentation-animation-select";

    const none = document.createElement("option");
    none.value = "";
    none.textContent = "No animation";
    select.appendChild(none);

    const ids = new Set();
    for (const item of animationCatalog) {
      if (!item || typeof item.id !== "string" || !item.id) continue;
      ids.add(item.id);
      const option = document.createElement("option");
      option.value = item.id;
      option.textContent = item.id;
      if (typeof item.path === "string" && item.path) option.title = item.path;
      select.appendChild(option);
    }

    const current = normalizedAnimation(line);
    if (current && !ids.has(current)) {
      const unavailable = document.createElement("option");
      unavailable.value = current;
      unavailable.textContent = `${current} (not in current catalog)`;
      select.appendChild(unavailable);
    }
    select.value = current || "";

    select.addEventListener("change", () => {
      line.animation = select.value || null;
      changed();
      renderPresentation();
    });
    return select;
  }

  function renderLines() {
    ui.lineList.replaceChildren();
    const beat = selectedBeat();
    if (!beat) {
      ui.beatTitle.textContent = "Select a beat";
      ui.beatId.textContent = "";
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "Choose a conversation beat to author presentation cues.";
      ui.lineList.appendChild(empty);
      return;
    }

    ui.beatTitle.textContent = beat.title || beat.id || "Beat";
    ui.beatId.textContent = beat.id || "(missing id)";
    const lines = Array.isArray(beat.lines) ? beat.lines : [];
    if (!lines.length) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.textContent = "This beat has no dialogue lines.";
      ui.lineList.appendChild(empty);
      return;
    }

    lines.forEach((line, index) => {
      const card = document.createElement("div");
      card.className = "presentation-line-card";

      const header = document.createElement("div");
      header.className = "presentation-line-header";
      const label = document.createElement("div");
      label.className = "presentation-line-label";
      label.textContent = `Dialogue Line ${index + 1}`;
      const status = document.createElement("div");
      status.className = "presentation-line-status";
      const current = normalizedAnimation(line);
      status.textContent = current ? `ANIMATION · ${current}` : "NO ANIMATION";
      status.classList.toggle("active", Boolean(current));
      header.append(label, status);

      const text = document.createElement("div");
      text.className = "presentation-line-text";
      text.textContent = typeof line?.text === "string" && line.text ? line.text : "(No authored NPC text)";

      const field = document.createElement("label");
      field.className = "field presentation-animation-field";
      const title = document.createElement("span");
      title.textContent = "Animation cue";
      field.append(title, animationSelect(line));

      const note = document.createElement("div");
      note.className = "presentation-line-note";
      note.textContent = "Optional. Stored as the authored animation id; runtime playback is deferred to N10.";

      card.append(header, text, field, note);
      ui.lineList.appendChild(card);
    });
  }

  function renderPresentation() {
    renderBeatList();
    renderLines();
  }

  function openPresentation() {
    if (state.activeSurface === "shell") {
      const parsed = parseRaw();
      if (!parsed.ok) {
        window.alert("Raw JSON is invalid. Fix it before leaving Shell.");
        return;
      }
      state.documentValue = parsed.value;
      fillIdentity();
      renderDialogueAndState();
    } else if (state.activeSurface === "identity") {
      applyIdentity();
    }

    for (const candidate of document.querySelectorAll(".surface")) candidate.classList.remove("active");
    surface.classList.add("active");
    for (const candidate of document.querySelectorAll(".future-tab")) candidate.classList.remove("active");
    tab.classList.add("active");
    state.activeSurface = "presentation";
    renderPresentation();
    loadCatalog();
  }

  tab.addEventListener("click", openPresentation);
  for (const other of document.querySelectorAll(".future-tab")) {
    if (other === tab) continue;
    other.addEventListener("click", () => {
      surface.classList.remove("active");
      tab.classList.remove("active");
    });
  }

  const baseValidationErrors = validationErrors;
  validationErrors = function n9aValidationErrors(doc) {
    const errors = baseValidationErrors(doc);
    beatsOf(doc).forEach((beat, beatIndex) => {
      const lines = Array.isArray(beat?.lines) ? beat.lines : [];
      lines.forEach((line, lineIndex) => {
        if (!line || typeof line !== "object" || Array.isArray(line)) return;
        if (!Object.prototype.hasOwnProperty.call(line, "animation") || line.animation === null) return;
        if (typeof line.animation !== "string" || !line.animation.trim()) {
          const beatLabel = beat?.id || `Beat ${beatIndex + 1}`;
          errors.push(`${beatLabel} Dialogue Line ${lineIndex + 1}: animation must be null or a non-empty authored animation id.`);
        }
      });
    });
    return errors;
  };

  const baseRenderDialogueAndState = renderDialogueAndState;
  renderDialogueAndState = function n9aRenderDialogueAndState() {
    baseRenderDialogueAndState();
    if (state.activeSurface === "presentation") renderPresentation();
  };

  const baseOpenNpc = openNpc;
  openNpc = async function n9aOpenNpc(path) {
    await baseOpenNpc(path);
    if (state.activeSurface === "presentation") renderPresentation();
  };
})();
