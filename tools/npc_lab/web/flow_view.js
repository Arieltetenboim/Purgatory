(() => {
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/flow_view.css";
  document.head.appendChild(css);

  const workspaceBody = document.querySelector(".workspace-body");
  const tabs = document.querySelector(".future-tabs");
  if (!workspaceBody || !tabs) return;

  const stateTab = [...tabs.querySelectorAll(".future-tab")].find(
    (candidate) => candidate.dataset.surface === "state"
  );
  const tab = document.createElement("button");
  tab.className = "future-tab";
  tab.type = "button";
  tab.textContent = "FLOW";
  if (stateTab) stateTab.insertAdjacentElement("afterend", tab);
  else tabs.appendChild(tab);

  const surface = document.createElement("section");
  surface.id = "flowSurface";
  surface.className = "surface";
  surface.innerHTML = `
    <div class="flow-view">
      <div class="flow-toolbar">
        <div class="flow-toolbar-main">
          <div>
            <div class="section-kicker">N8</div>
            <div class="flow-title">Dialogue Flow</div>
          </div>
          <div id="flowSummary" class="flow-summary">Select an NPC.</div>
        </div>
        <div class="flow-toolbar-actions">
          <div class="flow-legend">
            <span><i class="flow-dot"></i> ENTRY</span>
            <span><i class="flow-dot continuation"></i> CONTINUATION</span>
            <span><i class="flow-loop-glyph">↺</i> LOOP</span>
          </div>
          <div id="flowSelected" class="flow-selected"></div>
        </div>
      </div>
      <div class="flow-scroll">
        <div id="flowCanvas" class="flow-canvas">
          <svg id="flowEdges" class="flow-edges" aria-hidden="true"></svg>
          <div class="flow-columns">
            <section class="flow-lane flow-entry-column">
              <div class="flow-lane-header"><span class="flow-lane-title">ENTRY</span><span id="flowEntryCount" class="flow-lane-count">0</span></div>
              <div id="flowEntryLane" class="flow-lane"></div>
            </section>
            <section class="flow-lane flow-continuation-column">
              <div class="flow-lane-header"><span class="flow-lane-title">CONTINUATION</span><span id="flowContinuationCount" class="flow-lane-count">0</span></div>
              <div id="flowContinuationLane" class="flow-lane"></div>
            </section>
          </div>
        </div>
      </div>
    </div>`;
  workspaceBody.appendChild(surface);

  const byId = (id) => document.getElementById(id);
  const ui = {
    canvas: byId("flowCanvas"),
    edges: byId("flowEdges"),
    entryLane: byId("flowEntryLane"),
    continuationLane: byId("flowContinuationLane"),
    entryCount: byId("flowEntryCount"),
    continuationCount: byId("flowContinuationCount"),
    summary: byId("flowSummary"),
    selected: byId("flowSelected"),
  };

  function linksOf(beat) {
    const choices = Array.isArray(beat?.choices) ? beat.choices : [];
    return choices.map((choice) => ({
      choice: choice?.text || choice?.id || "choice",
      target: typeof choice?.next === "string" && choice.next ? choice.next : null,
    }));
  }

  function edgeKey(source, target) {
    return `${source}\u0000${target}`;
  }

  function cycleEdges(beats) {
    const knownIds = new Set(beats.map((beat) => beat?.id).filter(Boolean));
    const adjacency = new Map();
    for (const beat of beats) {
      if (!beat?.id) continue;
      adjacency.set(
        beat.id,
        linksOf(beat)
          .map((link) => link.target)
          .filter((target) => target && knownIds.has(target))
      );
    }

    function reaches(start, goal) {
      const pending = [start];
      const seen = new Set();
      while (pending.length) {
        const current = pending.pop();
        if (current === goal) return true;
        if (seen.has(current)) continue;
        seen.add(current);
        for (const next of adjacency.get(current) || []) {
          if (!seen.has(next)) pending.push(next);
        }
      }
      return false;
    }

    const result = new Set();
    for (const [source, targets] of adjacency) {
      for (const target of targets) {
        if (target === source || reaches(target, source)) {
          result.add(edgeKey(source, target));
        }
      }
    }
    return result;
  }

  function makeNode(beat, index, knownIds, loopEdges) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `flow-node ${beat?.entry === true ? "entry" : "continuation"}`;
    button.dataset.beatId = beat?.id || "";
    if (index === state.selectedBeatIndex) button.classList.add("selected");
    if (!beat?.id) button.classList.add("invalid");

    const top = document.createElement("div");
    top.className = "flow-node-top";
    const title = document.createElement("div");
    title.className = "flow-node-title";
    title.textContent = beat?.title || beat?.id || `Beat ${index + 1}`;
    const priority = document.createElement("div");
    priority.className = "flow-node-priority";
    priority.textContent = Number.isInteger(beat?.priority) ? `P${beat.priority}` : "P?";
    top.append(title, priority);

    const id = document.createElement("div");
    id.className = "flow-node-id";
    id.textContent = beat?.id || "(missing id)";

    const meta = document.createElement("div");
    meta.className = "flow-node-meta";
    for (const label of [
      beat?.pool || "mandatory",
      `${Array.isArray(beat?.conditions) ? beat.conditions.length : 0} cond`,
      `${Array.isArray(beat?.choices) ? beat.choices.length : 0} choices`,
    ]) {
      const pill = document.createElement("span");
      pill.className = "flow-pill";
      pill.textContent = label;
      meta.appendChild(pill);
    }

    button.append(top, id, meta);

    const links = linksOf(beat);
    if (links.length) {
      const list = document.createElement("div");
      list.className = "flow-links";
      for (const link of links) {
        const row = document.createElement("div");
        row.className = "flow-link";
        const choice = document.createElement("span");
        choice.className = "flow-link-choice";
        choice.textContent = link.choice;
        const target = document.createElement("span");
        target.className = "flow-link-target";
        if (!link.target) {
          row.classList.add("end");
          target.textContent = "END";
        } else if (!knownIds.has(link.target)) {
          row.classList.add("missing");
          target.textContent = `${link.target} !`;
        } else if (beat?.id && loopEdges.has(edgeKey(beat.id, link.target))) {
          row.classList.add("loop");
          target.textContent = `↺ LOOP → ${link.target}`;
        } else {
          target.textContent = `→ ${link.target}`;
        }
        row.append(choice, target);
        list.appendChild(row);
      }
      button.appendChild(list);
    }

    button.addEventListener("click", () => {
      state.selectedBeatIndex = index;
      renderDialogueAndState();
      renderFlow();
    });
    return button;
  }

  function appendArrowMarker(id, className) {
    const marker = document.createElementNS("http://www.w3.org/2000/svg", "marker");
    marker.setAttribute("id", id);
    marker.setAttribute("viewBox", "0 0 10 10");
    marker.setAttribute("refX", "9");
    marker.setAttribute("refY", "5");
    marker.setAttribute("markerWidth", "6");
    marker.setAttribute("markerHeight", "6");
    marker.setAttribute("orient", "auto-start-reverse");
    const arrow = document.createElementNS("http://www.w3.org/2000/svg", "path");
    arrow.setAttribute("d", "M 0 0 L 10 5 L 0 10 z");
    arrow.setAttribute("class", className);
    marker.appendChild(arrow);
    return marker;
  }

  function drawEdges(loopEdges = cycleEdges(beatsOf())) {
    ui.edges.replaceChildren();
    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    defs.append(
      appendArrowMarker("flowArrow", "flow-arrow"),
      appendArrowMarker("flowArrowSelected", "flow-arrow-selected"),
      appendArrowMarker("flowArrowLoop", "flow-arrow-loop")
    );
    ui.edges.appendChild(defs);

    const canvasRect = ui.canvas.getBoundingClientRect();
    const nodes = new Map(
      [...ui.canvas.querySelectorAll(".flow-node[data-beat-id]")]
        .filter((node) => node.dataset.beatId)
        .map((node) => [node.dataset.beatId, node])
    );
    const height = Math.max(ui.canvas.scrollHeight, 1);
    const width = Math.max(ui.canvas.scrollWidth, 1);
    ui.edges.setAttribute("viewBox", `0 0 ${width} ${height}`);

    for (const beat of beatsOf()) {
      if (!beat?.id) continue;
      const source = nodes.get(beat.id);
      if (!source) continue;
      const sourceRect = source.getBoundingClientRect();
      for (const link of linksOf(beat)) {
        if (!link.target) continue;
        const target = nodes.get(link.target);
        if (!target) continue;
        const targetRect = target.getBoundingClientRect();
        const isLoop = loopEdges.has(edgeKey(beat.id, link.target));
        const isSelected = beatsOf()[state.selectedBeatIndex]?.id === beat.id;
        const sameLane = source.parentElement === target.parentElement;
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");

        if (isLoop && sameLane) {
          const entryLane = source.parentElement === ui.entryLane;
          const sx = (entryLane ? sourceRect.left : sourceRect.right) - canvasRect.left + ui.canvas.scrollLeft;
          const sy = sourceRect.top - canvasRect.top + ui.canvas.scrollTop + sourceRect.height / 2;
          const tx = (entryLane ? targetRect.left : targetRect.right) - canvasRect.left + ui.canvas.scrollLeft;
          const ty = targetRect.top - canvasRect.top + ui.canvas.scrollTop + targetRect.height / 2;
          const outsideX = entryLane
            ? Math.min(sx, tx) - 46
            : Math.max(sx, tx) + 46;
          path.setAttribute(
            "d",
            `M ${sx} ${sy} C ${outsideX} ${sy}, ${outsideX} ${ty}, ${tx} ${ty}`
          );
        } else {
          const sx = sourceRect.right - canvasRect.left + ui.canvas.scrollLeft;
          const sy = sourceRect.top - canvasRect.top + ui.canvas.scrollTop + sourceRect.height / 2;
          const tx = targetRect.left - canvasRect.left + ui.canvas.scrollLeft;
          const ty = targetRect.top - canvasRect.top + ui.canvas.scrollTop + targetRect.height / 2;
          const bend = Math.max(40, Math.abs(tx - sx) * 0.45);
          path.setAttribute(
            "d",
            `M ${sx} ${sy} C ${sx + bend} ${sy}, ${tx - bend} ${ty}, ${tx} ${ty}`
          );
        }

        const classes = ["flow-edge"];
        if (isLoop) classes.push("loop");
        if (isSelected) classes.push("selected");
        path.setAttribute("class", classes.join(" "));
        path.setAttribute(
          "marker-end",
          `url(#${isLoop ? "flowArrowLoop" : isSelected ? "flowArrowSelected" : "flowArrow"})`
        );
        ui.edges.appendChild(path);
      }
    }
  }

  function renderFlow() {
    ui.entryLane.replaceChildren();
    ui.continuationLane.replaceChildren();
    const beats = beatsOf();
    const knownIds = new Set(beats.map((beat) => beat?.id).filter(Boolean));
    const loopEdges = cycleEdges(beats);
    let entries = 0;
    let continuations = 0;
    let missingTargets = 0;

    beats.forEach((beat, index) => {
      const node = makeNode(beat, index, knownIds, loopEdges);
      if (beat?.entry === true) {
        entries += 1;
        ui.entryLane.appendChild(node);
      } else {
        continuations += 1;
        ui.continuationLane.appendChild(node);
      }
      for (const link of linksOf(beat)) {
        if (link.target && !knownIds.has(link.target)) missingTargets += 1;
      }
    });

    if (!entries) {
      const empty = document.createElement("div");
      empty.className = "flow-empty";
      empty.textContent = "No ENTRY beats.";
      ui.entryLane.appendChild(empty);
    }
    if (!continuations) {
      const empty = document.createElement("div");
      empty.className = "flow-empty";
      empty.textContent = "No CONTINUATION beats.";
      ui.continuationLane.appendChild(empty);
    }

    ui.entryCount.textContent = String(entries);
    ui.continuationCount.textContent = String(continuations);
    ui.summary.textContent = state.documentValue
      ? `${beats.length} beats · ${entries} ENTRY · ${continuations} CONTINUATION${loopEdges.size ? ` · ${loopEdges.size} loop link${loopEdges.size === 1 ? "" : "s"}` : ""}${missingTargets ? ` · ${missingTargets} missing target${missingTargets === 1 ? "" : "s"}` : ""}`
      : "Select an NPC.";
    const selected = selectedBeat();
    ui.selected.textContent = selected ? `Selected: ${selected.id || "(missing id)"}` : "";
    requestAnimationFrame(() => drawEdges(loopEdges));
  }

  function openFlow() {
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

    for (const s of document.querySelectorAll(".surface")) s.classList.remove("active");
    surface.classList.add("active");
    for (const t of document.querySelectorAll(".future-tab")) t.classList.remove("active");
    tab.classList.add("active");
    state.activeSurface = "flow";
    renderFlow();
  }

  tab.addEventListener("click", openFlow);
  for (const other of document.querySelectorAll(".future-tab[data-surface]")) {
    other.addEventListener("click", () => {
      surface.classList.remove("active");
      tab.classList.remove("active");
    });
  }
  const testTab = [...tabs.querySelectorAll(".future-tab")].find(
    (candidate) => candidate.textContent.trim() === "TEST"
  );
  if (testTab) {
    testTab.addEventListener("click", () => {
      surface.classList.remove("active");
      tab.classList.remove("active");
    });
  }

  window.addEventListener("resize", () => {
    if (state.activeSurface === "flow") drawEdges();
  });

  const baseRenderDialogueAndState = renderDialogueAndState;
  renderDialogueAndState = function n8RenderDialogueAndState() {
    baseRenderDialogueAndState();
    if (state.activeSurface === "flow") renderFlow();
  };

  const baseOpenNpc = openNpc;
  openNpc = async function n8OpenNpc(path) {
    await baseOpenNpc(path);
    if (state.activeSurface === "flow") renderFlow();
  };
})();
