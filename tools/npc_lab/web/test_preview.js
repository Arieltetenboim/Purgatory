(() => {
  const css = document.createElement("link");
  css.rel = "stylesheet";
  css.href = "/test_preview.css";
  document.head.appendChild(css);

  const workspaceBody = document.querySelector(".workspace-body");
  const tab = [...document.querySelectorAll(".future-tab")].find(x => x.textContent.trim() === "TEST");
  if (!workspaceBody || !tab) return;

  tab.disabled = false;
  const surface = document.createElement("section");
  surface.id = "testSurface";
  surface.className = "surface";
  surface.innerHTML = `
    <div class="testbench">
      <aside class="test-state pane-internal">
        <div class="section-kicker">TEST STATE</div><h2>Synthetic State</h2>
        <label class="field"><span>Facts</span><textarea id="testFacts" class="form-textarea list" placeholder="welcome.workshop.package_needed=true"></textarea><small>One fact=value per line.</small></label>
        <label class="field"><span>NPC Met</span><textarea id="testNpcMet" class="form-textarea list" placeholder="npc.welcome.traveler_stayed"></textarea></label>
        <label class="field"><span>Dialogue Heard</span><textarea id="testHeard" class="form-textarea list" placeholder="npc.welcome.traveler_stayed|intro"></textarea><small>npc|beat per line.</small></label>
        <label class="field"><span>Item Owned</span><textarea id="testOwned" class="form-textarea list" placeholder="item.package"></textarea></label>
        <label class="field"><span>Item Equipped</span><textarea id="testEquipped" class="form-textarea list"></textarea></label>
        <div class="test-actions"><button id="testEvaluate" class="button primary">START / EVALUATE</button><button id="testReset" class="button secondary">RESET</button></div>
      </aside>
      <section class="test-preview pane-internal">
        <div class="preview-toolbar">
          <div>
            <div class="section-kicker">PLAYER-FACING CONTENT</div>
            <div id="testAuthorNpc" class="preview-author-meta">Select an NPC</div>
          </div>
          <div id="testBeatMeta" class="preview-author-meta"></div>
        </div>
        <div class="player-preview-shell">
          <div id="testNpcName" class="player-npc-name hidden"></div>
          <div id="testSpeech" class="player-dialogue">Set synthetic state, then Start / Evaluate.</div>
          <div id="testChoices" class="player-choices"></div>
        </div>
        <details id="testDiagnosticsPanel" class="test-diagnostics-panel">
          <summary><span>Why this dialogue?</span><span id="testDiagSummary" class="diag-summary-inline">Evaluate to inspect selection</span></summary>
          <div id="testDiagnostics" class="test-diagnostics muted"></div>
        </details>
      </section>
    </div>`;
  workspaceBody.appendChild(surface);

  const $t = id => document.getElementById(id);
  const els = {
    facts:$t("testFacts"), npcMet:$t("testNpcMet"), heard:$t("testHeard"), owned:$t("testOwned"), equipped:$t("testEquipped"),
    evaluate:$t("testEvaluate"), reset:$t("testReset"), authorNpc:$t("testAuthorNpc"), npcName:$t("testNpcName"), meta:$t("testBeatMeta"), speech:$t("testSpeech"), choices:$t("testChoices"), diagnostics:$t("testDiagnostics"), diagSummary:$t("testDiagSummary"), diagnosticsPanel:$t("testDiagnosticsPanel")
  };
  let synthetic = {facts:{},npc_met:[],dialogue_heard:[],item_owned:[],item_equipped:[]};
  let currentBeat = null;
  let diagnostics = null;

  const lines = value => String(value||"").split(/\r?\n/).map(v=>v.trim()).filter(Boolean);
  function parseState() {
    const facts = {};
    for (const line of lines(els.facts.value)) {
      const i = line.indexOf("="); if (i < 1) throw new Error(`Fact must use id=true/false: ${line}`);
      const key=line.slice(0,i).trim(), raw=line.slice(i+1).trim().toLowerCase();
      if (raw!=="true" && raw!=="false") throw new Error(`Fact value must be true/false: ${line}`);
      facts[key]=raw==="true";
    }
    const heard = lines(els.heard.value).map(line => { const i=line.indexOf("|"); if(i<1) throw new Error(`Dialogue Heard must use npc|beat: ${line}`); return {npc:line.slice(0,i).trim(),beat:line.slice(i+1).trim()}; });
    return {facts,npc_met:lines(els.npcMet.value),dialogue_heard:heard,item_owned:lines(els.owned.value),item_equipped:lines(els.equipped.value)};
  }
  function writeState(s) {
    synthetic=s;
    els.facts.value=Object.entries(s.facts||{}).map(([k,v])=>`${k}=${v}`).join("\n");
    els.npcMet.value=(s.npc_met||[]).join("\n");
    els.heard.value=(s.dialogue_heard||[]).map(v=>`${v.npc}|${v.beat}`).join("\n");
    els.owned.value=(s.item_owned||[]).join("\n");
    els.equipped.value=(s.item_equipped||[]).join("\n");
  }
  async function call(operation, extra={}) {
    if (!state.documentValue) throw new Error("Select an NPC first.");
    synthetic=parseState();
    const response=await fetch("/api/test-preview",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({operation,document:state.documentValue,state:synthetic,...extra})});
    const payload=await response.json(); if(!response.ok) throw new Error(payload.error || (payload.validation_errors||[]).join(" | ") || "Preview failed.");
    writeState(payload.state||synthetic); currentBeat=payload.beat||null; diagnostics=payload.diagnostics||null; render(payload.ended===true); return payload;
  }
  function referenceText(item) {
    if(item.kind==="dialogue_heard") return `${item.reference.npc}|${item.reference.beat}`;
    return String(item.reference);
  }
  function failedConditionText(condition) {
    return `${condition.kind}: ${referenceText(condition)} expected ${condition.expected}, actual ${condition.actual}`;
  }
  function makeDiagRow(beat, message, detailClass="") {
    const row=document.createElement("div"); row.className=`diag-row ${detailClass}`;
    const identity=document.createElement("div"); identity.className="diag-row-id"; identity.textContent=`${beat.id} · P${beat.priority}`;
    const reason=document.createElement("div"); reason.className="diag-row-reason"; reason.textContent=message;
    row.append(identity, reason); return row;
  }
  function renderDiagnostics() {
    els.diagnostics.replaceChildren();
    if(!diagnostics){
      els.diagSummary.textContent="Evaluate to inspect selection";
      els.diagnostics.textContent="Diagnostics apply only to top-level ENTRY evaluation. Explicit Next Beat transitions are authored conversation flow.";
      return;
    }
    const winner=(diagnostics.beats||[]).find(b=>b.status==="winner")||null;
    const eligible=(diagnostics.beats||[]).filter(b=>b.status==="eligible");
    const rejected=(diagnostics.beats||[]).filter(b=>b.status==="rejected");
    const continuations=(diagnostics.beats||[]).filter(b=>b.status==="continuation");

    if(winner){
      els.diagSummary.textContent=`${winner.id} selected · ${rejected.length} blocked`;
      const why=document.createElement("div"); why.className="diag-explanation";
      why.textContent=eligible.length
        ? `${winner.id} is eligible at P${winner.priority} and outranks ${eligible.length} other eligible ENTRY beat${eligible.length===1?"":"s"}.`
        : `${winner.id} is eligible at P${winner.priority}; every other ENTRY beat is currently blocked.`;
      els.diagnostics.appendChild(why);
    } else {
      els.diagSummary.textContent=`No eligible ENTRY · ${rejected.length} blocked`;
      const why=document.createElement("div"); why.className="diag-explanation"; why.textContent="No ENTRY beat currently passes both its state conditions and pool rule."; els.diagnostics.appendChild(why);
    }

    if(eligible.length){
      const h=document.createElement("div"); h.className="diag-section-label"; h.textContent="Eligible alternatives"; els.diagnostics.appendChild(h);
      for(const beat of eligible) els.diagnostics.appendChild(makeDiagRow(beat, beat.reason, "eligible"));
    }

    if(rejected.length){
      const h=document.createElement("div"); h.className="diag-section-label"; h.textContent="Blocked ENTRY beats"; els.diagnostics.appendChild(h);
      for(const beat of rejected){
        const failures=(beat.conditions||[]).filter(c=>!c.matched).map(failedConditionText);
        if(beat.pool_result && !beat.pool_result.allowed) failures.push(beat.pool_result.reason);
        els.diagnostics.appendChild(makeDiagRow(beat, failures.join(" · ") || beat.reason, "blocked"));
      }
    }

    if(continuations.length){
      const details=document.createElement("details"); details.className="diag-continuations";
      const summary=document.createElement("summary"); summary.textContent=`${continuations.length} continuation beat${continuations.length===1?"":"s"} — outside ENTRY selection`;
      details.appendChild(summary);
      for(const beat of continuations) details.appendChild(makeDiagRow(beat, "Reached only through an authored Next Beat transition.", "continuation"));
      els.diagnostics.appendChild(details);
    }
  }
  function renderPlayerDialogue(ended=false) {
    els.choices.replaceChildren();
    const doc=state.documentValue||{};
    const design=doc.design||{};
    els.authorNpc.textContent=design.working_name||doc.id||"NPC";
    els.meta.textContent=currentBeat?`${currentBeat.id} · P${currentBeat.priority} · ${currentBeat.pool||"mandatory"}`:"";

    const displayName=typeof design.display_name==="string" && design.display_name.trim()?design.display_name.trim():null;
    els.npcName.classList.toggle("hidden", !displayName);
    els.npcName.textContent=displayName||"";

    if(!currentBeat){
      els.speech.replaceChildren();
      const message=document.createElement("div"); message.className="player-empty";
      message.textContent=ended?"Conversation ended.":"No eligible conversation for this state.";
      els.speech.appendChild(message);
      return;
    }

    els.speech.replaceChildren();
    const authoredLines=Array.isArray(currentBeat.lines)?currentBeat.lines:[];
    const visibleLines=authoredLines.filter(line=>line && typeof line.text==="string" && line.text.length>0);
    if(!visibleLines.length){
      const empty=document.createElement("div"); empty.className="player-empty"; empty.textContent="(No authored NPC text)"; els.speech.appendChild(empty);
    } else {
      for(const line of visibleLines){
        const utterance=document.createElement("div"); utterance.className="player-line"; utterance.textContent=line.text; els.speech.appendChild(utterance);
      }
    }

    const choices=Array.isArray(currentBeat.choices)?currentBeat.choices:[];
    if(!choices.length){
      const controls=document.createElement("div"); controls.className="preview-chrome";
      const b=document.createElement("button"); b.className="preview-continue"; b.textContent="Continue"; b.title="Preview control; not authored player dialogue.";
      b.onclick=()=>call("complete",{beat_id:currentBeat.id}).catch(showError); controls.appendChild(b); els.choices.appendChild(controls); return;
    }
    for(const choice of choices){
      const b=document.createElement("button"); b.className="player-choice"; b.textContent=choice.text||"";
      b.onclick=()=>call("advance",{beat_id:currentBeat.id,choice_id:choice.id}).catch(showError); els.choices.appendChild(b);
    }
  }
  function render(ended=false) {
    renderPlayerDialogue(ended);
    renderDiagnostics();
  }
  function showError(error){ console.error(error); els.speech.replaceChildren(); const message=document.createElement("div"); message.className="player-empty error"; message.textContent=`Preview error: ${error.message}`; els.speech.appendChild(message); }
  function openTest(){
    for(const s of document.querySelectorAll(".surface")) s.classList.remove("active"); surface.classList.add("active");
    for(const t of document.querySelectorAll(".future-tab")) t.classList.remove("active"); tab.classList.add("active");
    state.activeSurface="test"; currentBeat=null; diagnostics=null; els.diagnosticsPanel.open=false; render(false);
  }
  tab.addEventListener("click", openTest);
  for(const other of document.querySelectorAll(".future-tab[data-surface]")) other.addEventListener("click",()=>surface.classList.remove("active"));
  els.evaluate.addEventListener("click",()=>call("evaluate").catch(showError));
  els.reset.addEventListener("click",()=>{ writeState({facts:{},npc_met:[],dialogue_heard:[],item_owned:[],item_equipped:[]}); currentBeat=null; diagnostics=null; els.diagnosticsPanel.open=false; render(false); });
})();
