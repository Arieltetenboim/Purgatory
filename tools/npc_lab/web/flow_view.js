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
          <div class="flow-search-group">
            <input id="flowSearch" class="flow-search" type="search" placeholder="Jump to beat">
            <button id="flowJump" class="flow-tool-button" type="button">JUMP</button>
          </div>
          <div class="flow-zoom-group">
            <button id="flowZoomOut" class="flow-tool-button compact" type="button" title="Zoom out">−</button>
            <button id="flowZoomReset" class="flow-tool-button zoom-value" type="button" title="Reset zoom">100%</button>
            <button id="flowZoomIn" class="flow-tool-button compact" type="button" title="Zoom in">+</button>
            <button id="flowFit" class="flow-tool-button" type="button">FIT</button>
            <button id="flowCenter" class="flow-tool-button" type="button">CENTER</button>
            <button id="flowFocus" class="flow-tool-button" type="button">FOCUS</button>
          </div>
          <div class="flow-legend">
            <span><i class="flow-dot"></i> ENTRY</span>
            <span><i class="flow-dot continuation"></i> CONTINUATION</span>
            <span><i class="flow-loop-glyph">↺</i> LOOP</span>
            <span><i class="flow-warning-glyph">!</i> STRUCTURE</span>
          </div>
          <div id="flowSelected" class="flow-selected"></div>
        </div>
      </div>
      <div id="flowViewport" class="flow-viewport">
        <div id="flowZoomSpace" class="flow-zoom-space">
          <div id="flowStage" class="flow-stage">
            <svg id="flowEdges" class="flow-edges" aria-hidden="true"></svg>
            <div id="flowColumns" class="flow-columns"></div>
          </div>
        </div>
      </div>
    </div>`;
  workspaceBody.appendChild(surface);

  const byId = (id) => document.getElementById(id);
  const ui = {
    viewport: byId("flowViewport"),
    zoomSpace: byId("flowZoomSpace"),
    stage: byId("flowStage"),
    edges: byId("flowEdges"),
    columns: byId("flowColumns"),
    summary: byId("flowSummary"),
    selected: byId("flowSelected"),
    search: byId("flowSearch"),
    jump: byId("flowJump"),
    zoomOut: byId("flowZoomOut"),
    zoomReset: byId("flowZoomReset"),
    zoomIn: byId("flowZoomIn"),
    fit: byId("flowFit"),
    center: byId("flowCenter"),
    focus: byId("flowFocus"),
  };

  const view = {
    zoom: 1,
    baseWidth: 900,
    baseHeight: 600,
    focusSelected: false,
    dragging: false,
    dragX: 0,
    dragY: 0,
    startScrollLeft: 0,
    startScrollTop: 0,
    pendingCenterId: null,
  };

  function linksOf(beat) {
    const choices = Array.isArray(beat?.choices) ? beat.choices : [];
    return choices.map((choice, index) => ({
      index,
      choice: choice?.text || choice?.id || `choice ${index + 1}`,
      target: typeof choice?.next === "string" && choice.next ? choice.next : null,
    }));
  }

  function edgeKey(source, target) {
    return `${source}\u0000${target}`;
  }

  function graphAnalysis(beats) {
    const idToIndex = new Map();
    const duplicateIds = new Set();
    beats.forEach((beat, index) => {
      if (!beat?.id) return;
      if (idToIndex.has(beat.id)) duplicateIds.add(beat.id);
      else idToIndex.set(beat.id, index);
    });
    const knownIds = new Set(idToIndex.keys());
    const adjacency = new Map([...knownIds].map((id) => [id, []]));
    const incoming = new Map([...knownIds].map((id) => [id, []]));
    let missingTargets = 0;

    for (const beat of beats) {
      if (!beat?.id || !knownIds.has(beat.id)) continue;
      for (const link of linksOf(beat)) {
        if (!link.target) continue;
        if (!knownIds.has(link.target)) {
          missingTargets += 1;
          continue;
        }
        adjacency.get(beat.id).push(link.target);
        incoming.get(link.target).push(beat.id);
      }
    }

    const entryIds = beats
      .filter((beat) => beat?.entry === true && beat?.id && knownIds.has(beat.id))
      .map((beat) => beat.id);
    const depth = new Map();
    const queue = [];
    for (const id of entryIds) {
      if (!depth.has(id)) {
        depth.set(id, 0);
        queue.push(id);
      }
    }
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const current = queue[cursor];
      const nextDepth = depth.get(current) + 1;
      for (const target of adjacency.get(current) || []) {
        if (!depth.has(target) || nextDepth < depth.get(target)) {
          depth.set(target, nextDepth);
          queue.push(target);
        }
      }
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

    const loopEdges = new Set();
    for (const [source, targets] of adjacency) {
      for (const target of targets) {
        const sourceDepth = depth.get(source);
        const targetDepth = depth.get(target);
        const closesCycle = target === source || reaches(target, source);
        if (
          closesCycle &&
          (sourceDepth === undefined || targetDepth === undefined || targetDepth <= sourceDepth)
        ) {
          loopEdges.add(edgeKey(source, target));
        }
      }
    }

    const unreachableIds = new Set(
      [...knownIds].filter((id) => !depth.has(id) && !entryIds.includes(id))
    );
    const invalidBeatIndexes = new Set(
      beats.map((beat, index) => (!beat?.id ? index : null)).filter((index) => index !== null)
    );
    const warningCount =
      missingTargets + unreachableIds.size + duplicateIds.size + invalidBeatIndexes.size + (entryIds.length ? 0 : 1);

    return {
      knownIds,
      idToIndex,
      duplicateIds,
      adjacency,
      incoming,
      entryIds,
      depth,
      loopEdges,
      unreachableIds,
      missingTargets,
      invalidBeatIndexes,
      warningCount,
    };
  }

  function makeMetaPill(label, className = "") {
    const pill = document.createElement("span");
    pill.className = `flow-pill${className ? ` ${className}` : ""}`;
    pill.textContent = label;
    return pill;
  }

  function selectBeatIndex(index, center = false) {
    if (!Number.isInteger(index) || index < 0 || index >= beatsOf().length) return;
    state.selectedBeatIndex = index;
    if (center) view.pendingCenterId = beatsOf()[index]?.id || null;
    renderDialogueAndState();
  }

  function makeNode(beat, index, analysis) {
    const node = document.createElement("div");
    node.className = `flow-node ${beat?.entry === true ? "entry" : "continuation"}`;
    node.dataset.beatId = beat?.id || "";
    node.dataset.beatIndex = String(index);
    node.tabIndex = 0;
    node.setAttribute("role", "button");
    if (index === state.selectedBeatIndex) node.classList.add("selected");
    if (!beat?.id || analysis.duplicateIds.has(beat.id)) node.classList.add("invalid");
    if (beat?.id && analysis.unreachableIds.has(beat.id)) node.classList.add("unreachable");

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
    meta.append(
      makeMetaPill(beat?.entry === true ? "ENTRY" : "CONTINUATION", beat?.entry === true ? "entry" : "continuation"),
      makeMetaPill(beat?.pool || "mandatory"),
      makeMetaPill(`${Array.isArray(beat?.conditions) ? beat.conditions.length : 0} cond`),
      makeMetaPill(`${Array.isArray(beat?.choices) ? beat.choices.length : 0} choices`)
    );
    if (!beat?.id) meta.append(makeMetaPill("MISSING ID", "warning"));
    if (beat?.id && analysis.duplicateIds.has(beat.id)) meta.append(makeMetaPill("DUPLICATE ID", "warning"));
    if (beat?.id && analysis.unreachableIds.has(beat.id)) meta.append(makeMetaPill("NO ENTRY PATH", "warning"));

    node.append(top, id, meta);

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
        choice.title = link.choice;
        const target = document.createElement("span");
        target.className = "flow-link-target";
        if (!link.target) {
          row.classList.add("end");
          target.textContent = "END";
        } else if (!analysis.knownIds.has(link.target)) {
          row.classList.add("missing");
          target.textContent = `${link.target} !`;
        } else if (beat?.id && analysis.loopEdges.has(edgeKey(beat.id, link.target))) {
          row.classList.add("loop");
          target.textContent = `↺ LOOP → ${link.target}`;
        } else {
          target.textContent = `→ ${link.target}`;
        }
        row.append(choice, target);
        if (link.target && analysis.idToIndex.has(link.target)) {
          row.classList.add("jumpable");
          row.title = `Jump to ${link.target}`;
          row.addEventListener("click", (event) => {
            event.stopPropagation();
            selectBeatIndex(analysis.idToIndex.get(link.target), true);
          });
        }
        list.appendChild(row);
      }
      node.appendChild(list);
    } else {
      const terminal = document.createElement("div");
      terminal.className = "flow-node-terminal";
      terminal.textContent = "END";
      node.appendChild(terminal);
    }

    const choose = () => selectBeatIndex(index, false);
    node.addEventListener("click", choose);
    node.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        choose();
      }
    });
    node.addEventListener("dblclick", () => {
      view.pendingCenterId = beat?.id || null;
      centerSelected();
    });
    return node;
  }

  function layerForBeat(beat, analysis) {
    if (!beat?.id || !analysis.depth.has(beat.id)) return "unreachable";
    return analysis.depth.get(beat.id);
  }

  function renderLayers(beats, analysis) {
    ui.columns.replaceChildren();
    const numericDepths = [...analysis.depth.values()];
    const maxDepth = numericDepths.length ? Math.max(...numericDepths) : 0;
    const layers = new Map();
    for (let depth = 0; depth <= maxDepth; depth += 1) layers.set(depth, []);
    const unreachable = [];

    beats.forEach((beat, index) => {
      const layer = layerForBeat(beat, analysis);
      if (layer === "unreachable") unreachable.push([beat, index]);
      else layers.get(layer).push([beat, index]);
    });

    function addLayer(key, items) {
      if (!items.length && key !== 0) return;
      const column = document.createElement("section");
      column.className = `flow-layer${key === "unreachable" ? " unreachable" : ""}`;
      column.dataset.depth = String(key);
      const header = document.createElement("div");
      header.className = "flow-layer-header";
      const title = document.createElement("span");
      title.className = "flow-layer-title";
      title.textContent = key === "unreachable" ? "UNREACHABLE" : key === 0 ? "START" : `STEP ${key}`;
      const count = document.createElement("span");
      count.className = "flow-layer-count";
      count.textContent = String(items.length);
      header.append(title, count);
      const nodes = document.createElement("div");
      nodes.className = "flow-layer-nodes";
      for (const [beat, index] of items) nodes.appendChild(makeNode(beat, index, analysis));
      if (!items.length) {
        const empty = document.createElement("div");
        empty.className = "flow-empty";
        empty.textContent = "No beats.";
        nodes.appendChild(empty);
      }
      column.append(header, nodes);
      ui.columns.appendChild(column);
    }

    for (let depth = 0; depth <= maxDepth; depth += 1) addLayer(depth, layers.get(depth));
    if (unreachable.length) addLayer("unreachable", unreachable);
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

  function nodePoint(node, side) {
    const stageRect = ui.stage.getBoundingClientRect();
    const rect = node.getBoundingClientRect();
    const scale = view.zoom || 1;
    if (side === "top") {
      return {
        x: (rect.left + rect.width / 2 - stageRect.left) / scale,
        y: (rect.top - stageRect.top) / scale,
      };
    }
    if (side === "bottom") {
      return {
        x: (rect.left + rect.width / 2 - stageRect.left) / scale,
        y: (rect.bottom - stageRect.top) / scale,
      };
    }
    return {
      x: ((side === "left" ? rect.left : rect.right) - stageRect.left) / scale,
      y: (rect.top + rect.height / 2 - stageRect.top) / scale,
    };
  }

  function makeEdgeLabel(text, x, y, classes = "") {
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.setAttribute("class", `flow-edge-label-group${classes ? ` ${classes}` : ""}`);
    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("x", String(x));
    label.setAttribute("y", String(y));
    label.setAttribute("text-anchor", "middle");
    label.setAttribute("class", "flow-edge-label");
    const compact = text.length > 28 ? `${text.slice(0, 27)}…` : text;
    label.textContent = compact;
    group.appendChild(label);
    return group;
  }

  function relatedIds(analysis) {
    const selected = selectedBeat()?.id;
    if (!selected || !analysis.knownIds.has(selected)) return new Set();
    const related = new Set([selected]);
    for (const target of analysis.adjacency.get(selected) || []) related.add(target);
    for (const source of analysis.incoming.get(selected) || []) related.add(source);
    return related;
  }

  function drawEdges(analysis) {
    ui.edges.replaceChildren();
    const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
    defs.append(
      appendArrowMarker("flowArrow", "flow-arrow"),
      appendArrowMarker("flowArrowSelected", "flow-arrow-selected"),
      appendArrowMarker("flowArrowLoop", "flow-arrow-loop")
    );
    ui.edges.appendChild(defs);
    ui.edges.setAttribute("viewBox", `0 0 ${view.baseWidth} ${view.baseHeight}`);

    const nodes = new Map(
      [...ui.stage.querySelectorAll(".flow-node[data-beat-id]")]
        .filter((node) => node.dataset.beatId)
        .map((node) => [node.dataset.beatId, node])
    );
    const selectedId = selectedBeat()?.id || null;
    const related = relatedIds(analysis);
    let loopRailIndex = 0;

    for (const beat of beatsOf()) {
      if (!beat?.id) continue;
      const source = nodes.get(beat.id);
      if (!source) continue;
      for (const link of linksOf(beat)) {
        if (!link.target) continue;
        const target = nodes.get(link.target);
        if (!target) continue;
        const isLoop = analysis.loopEdges.has(edgeKey(beat.id, link.target));
        const touchesSelected = selectedId && (beat.id === selectedId || link.target === selectedId);
        const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
        let labelX = 0;
        let labelY = 0;

        if (isLoop) {
          const sourcePoint = nodePoint(source, "top");
          const targetPoint = nodePoint(target, "top");
          const railY = 18 + (loopRailIndex % 4) * 15;
          loopRailIndex += 1;
          const shoulder = 28;
          path.setAttribute(
            "d",
            `M ${sourcePoint.x} ${sourcePoint.y} C ${sourcePoint.x} ${sourcePoint.y - shoulder}, ${sourcePoint.x} ${railY}, ${sourcePoint.x} ${railY} L ${targetPoint.x} ${railY} C ${targetPoint.x} ${railY}, ${targetPoint.x} ${targetPoint.y - shoulder}, ${targetPoint.x} ${targetPoint.y}`
          );
          labelX = (sourcePoint.x + targetPoint.x) / 2;
          labelY = railY - 4;
        } else {
          const sourcePoint = nodePoint(source, "right");
          const targetPoint = nodePoint(target, "left");
          const gap = Math.max(48, Math.abs(targetPoint.x - sourcePoint.x) * 0.45);
          path.setAttribute(
            "d",
            `M ${sourcePoint.x} ${sourcePoint.y} C ${sourcePoint.x + gap} ${sourcePoint.y}, ${targetPoint.x - gap} ${targetPoint.y}, ${targetPoint.x} ${targetPoint.y}`
          );
          labelX = (sourcePoint.x + targetPoint.x) / 2;
          labelY = (sourcePoint.y + targetPoint.y) / 2 - 5 + link.index * 10;
        }

        const classes = ["flow-edge"];
        if (isLoop) classes.push("loop");
        if (touchesSelected) classes.push("selected");
        if (view.focusSelected && selectedId && !touchesSelected) classes.push("dimmed");
        path.setAttribute("class", classes.join(" "));
        path.setAttribute(
          "marker-end",
          `url(#${isLoop ? "flowArrowLoop" : touchesSelected ? "flowArrowSelected" : "flowArrow"})`
        );
        ui.edges.appendChild(path);
        const labelClasses = [isLoop ? "loop" : ""];
        if (touchesSelected) labelClasses.push("selected");
        if (view.focusSelected && selectedId && !touchesSelected) labelClasses.push("dimmed");
        ui.edges.appendChild(makeEdgeLabel(link.choice, labelX, labelY, labelClasses.filter(Boolean).join(" ")));
      }
    }

    for (const node of nodes.values()) {
      if (!view.focusSelected || !selectedId) {
        node.classList.remove("dimmed", "related");
      } else if (related.has(node.dataset.beatId)) {
        node.classList.remove("dimmed");
        if (node.dataset.beatId !== selectedId) node.classList.add("related");
      } else {
        node.classList.remove("related");
        node.classList.add("dimmed");
      }
    }
  }

  function measureStage() {
    view.baseWidth = Math.max(ui.columns.scrollWidth, 760);
    view.baseHeight = Math.max(ui.columns.scrollHeight, 420);
    ui.stage.style.width = `${view.baseWidth}px`;
    ui.stage.style.height = `${view.baseHeight}px`;
    applyZoom(false);
  }

  function applyZoom(preserveCenter = true) {
    const oldZoom = Number(ui.stage.dataset.zoom || "1") || 1;
    let centerX = 0;
    let centerY = 0;
    if (preserveCenter) {
      centerX = (ui.viewport.scrollLeft + ui.viewport.clientWidth / 2) / oldZoom;
      centerY = (ui.viewport.scrollTop + ui.viewport.clientHeight / 2) / oldZoom;
    }
    ui.stage.style.transform = `scale(${view.zoom})`;
    ui.stage.dataset.zoom = String(view.zoom);
    ui.zoomSpace.style.width = `${Math.ceil(view.baseWidth * view.zoom)}px`;
    ui.zoomSpace.style.height = `${Math.ceil(view.baseHeight * view.zoom)}px`;
    ui.zoomReset.textContent = `${Math.round(view.zoom * 100)}%`;
    if (preserveCenter) {
      ui.viewport.scrollLeft = Math.max(0, centerX * view.zoom - ui.viewport.clientWidth / 2);
      ui.viewport.scrollTop = Math.max(0, centerY * view.zoom - ui.viewport.clientHeight / 2);
    }
  }

  function setZoom(value, preserveCenter = true) {
    view.zoom = Math.min(1.8, Math.max(0.3, value));
    applyZoom(preserveCenter);
  }

  function fitGraph() {
    if (!view.baseWidth || !view.baseHeight) return;
    const x = (ui.viewport.clientWidth - 24) / view.baseWidth;
    const y = (ui.viewport.clientHeight - 24) / view.baseHeight;
    setZoom(Math.min(x, y, 1.25), false);
    ui.viewport.scrollLeft = 0;
    ui.viewport.scrollTop = 0;
  }

  function centerSelected() {
    const selected = selectedBeat()?.id;
    if (!selected) return;
    const node = [...ui.stage.querySelectorAll(".flow-node[data-beat-id]")].find(
      (candidate) => candidate.dataset.beatId === selected
    );
    if (!node) return;
    const viewportRect = ui.viewport.getBoundingClientRect();
    const nodeRect = node.getBoundingClientRect();
    ui.viewport.scrollLeft += nodeRect.left + nodeRect.width / 2 - (viewportRect.left + viewportRect.width / 2);
    ui.viewport.scrollTop += nodeRect.top + nodeRect.height / 2 - (viewportRect.top + viewportRect.height / 2);
  }

  function renderFlow() {
    const beats = beatsOf();
    const analysis = graphAnalysis(beats);
    renderLayers(beats, analysis);

    const entries = beats.filter((beat) => beat?.entry === true).length;
    const continuations = beats.length - entries;
    const summary = [
      `${beats.length} beats`,
      `${entries} ENTRY`,
      `${continuations} CONTINUATION`,
    ];
    if (analysis.loopEdges.size) summary.push(`${analysis.loopEdges.size} loop${analysis.loopEdges.size === 1 ? "" : "s"}`);
    if (analysis.warningCount) summary.push(`! ${analysis.warningCount} structural`);
    ui.summary.textContent = state.documentValue ? summary.join(" · ") : "Select an NPC.";
    const selected = selectedBeat();
    ui.selected.textContent = selected ? `Selected: ${selected.id || "(missing id)"}` : "";
    ui.focus.classList.toggle("active", view.focusSelected);

    requestAnimationFrame(() => {
      measureStage();
      drawEdges(analysis);
      if (view.pendingCenterId) {
        const wanted = view.pendingCenterId;
        view.pendingCenterId = null;
        const index = analysis.idToIndex.get(wanted);
        if (Number.isInteger(index)) {
          state.selectedBeatIndex = index;
          centerSelected();
        }
      }
    });
  }

  function jumpToSearch() {
    const query = ui.search.value.trim().toLowerCase();
    if (!query) return;
    const beats = beatsOf();
    const index = beats.findIndex((beat) =>
      [beat?.id, beat?.title]
        .filter((value) => typeof value === "string")
        .some((value) => value.toLowerCase().includes(query))
    );
    if (index < 0) {
      setStatus(`FLOW: no beat matching '${ui.search.value.trim()}'.`);
      return;
    }
    selectBeatIndex(index, true);
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

    for (const candidate of document.querySelectorAll(".surface")) candidate.classList.remove("active");
    surface.classList.add("active");
    for (const candidate of document.querySelectorAll(".future-tab")) candidate.classList.remove("active");
    tab.classList.add("active");
    state.activeSurface = "flow";
    renderFlow();
  }

  tab.addEventListener("click", openFlow);
  for (const other of document.querySelectorAll(".future-tab")) {
    if (other === tab) continue;
    other.addEventListener("click", () => {
      surface.classList.remove("active");
      tab.classList.remove("active");
    });
  }

  ui.jump.addEventListener("click", jumpToSearch);
  ui.search.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      jumpToSearch();
    }
  });
  ui.zoomOut.addEventListener("click", () => setZoom(view.zoom - 0.1));
  ui.zoomIn.addEventListener("click", () => setZoom(view.zoom + 0.1));
  ui.zoomReset.addEventListener("click", () => setZoom(1));
  ui.fit.addEventListener("click", fitGraph);
  ui.center.addEventListener("click", centerSelected);
  ui.focus.addEventListener("click", () => {
    view.focusSelected = !view.focusSelected;
    renderFlow();
  });

  ui.viewport.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || event.target.closest(".flow-node, button, input")) return;
    view.dragging = true;
    view.dragX = event.clientX;
    view.dragY = event.clientY;
    view.startScrollLeft = ui.viewport.scrollLeft;
    view.startScrollTop = ui.viewport.scrollTop;
    ui.viewport.classList.add("dragging");
  });
  window.addEventListener("mousemove", (event) => {
    if (!view.dragging) return;
    ui.viewport.scrollLeft = view.startScrollLeft - (event.clientX - view.dragX);
    ui.viewport.scrollTop = view.startScrollTop - (event.clientY - view.dragY);
  });
  window.addEventListener("mouseup", () => {
    view.dragging = false;
    ui.viewport.classList.remove("dragging");
  });
  ui.viewport.addEventListener(
    "wheel",
    (event) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      setZoom(view.zoom + (event.deltaY < 0 ? 0.1 : -0.1));
    },
    { passive: false }
  );

  window.addEventListener("resize", () => {
    if (state.activeSurface === "flow") {
      const analysis = graphAnalysis(beatsOf());
      requestAnimationFrame(() => {
        measureStage();
        drawEdges(analysis);
      });
    }
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
