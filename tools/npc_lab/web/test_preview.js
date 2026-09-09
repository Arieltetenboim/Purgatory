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
        <div class="section-kicker">N6A</div><h2>Synthetic State</h2>
        <label class="field"><span>Facts</span><textarea id="testFacts" class="form-textarea list" placeholder="welcome.workshop.package_needed=true"></textarea><small>One fact=value per line.</small></label>
        <label class="field"><span>NPC Met</span><textarea id="testNpcMet" class="form-textarea list" placeholder="npc.welcome.traveler_stayed"></textarea></label>
        <label class="field"><span>Dialogue Heard</span><textarea id="testHeard" class="form-textarea list" placeholder="npc.welcome.traveler_stayed|intro"></textarea><small>npc|beat per line.</small></label>
        <label class="field"><span>Item Owned</span><textarea id="testOwned" class="form-textarea list" placeholder="item.package"></textarea></label>
        <label class="field"><span>Item Equipped</span><textarea id="testEquipped" class="form-textarea list"></textarea></label>
        <div class="test-actions"><button id="testEvaluate" class="button primary">EVALUATE</button><button id="testReset" class="button secondary">RESET</button></div>
      </aside>
      <section class="test-preview pane-internal">
        <div class="section-kicker">CONVERSATION PREVIEW</div>
        <div id="testNpcName" class="test-npc-name">Select an NPC</div>
        <div id="testBeatMeta" class="test-meta"></div>
        <div id="testSpeech" class="test-speech">Set synthetic state, then Evaluate.</div>
        <div id="testChoices" class="test-choices"></div>
      </section>
    </div>`;
  workspaceBody.appendChild(surface);

  const $t = id => document.getElementById(id);
  const els = {
    facts:$t("testFacts"), npcMet:$t("testNpcMet"), heard:$t("testHeard"), owned:$t("testOwned"), equipped:$t("testEquipped"),
    evaluate:$t("testEvaluate"), reset:$t("testReset"), npcName:$t("testNpcName"), meta:$t("testBeatMeta"), speech:$t("testSpeech"), choices:$t("testChoices")
  };
  let synthetic = {facts:{},npc_met:[],dialogue_heard:[],item_owned:[],item_equipped:[]};
  let currentBeat = null;

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
    writeState(payload.state||synthetic); currentBeat=payload.beat||null; render(payload.ended===true); return payload;
  }
  function render(ended=false) {
    els.choices.replaceChildren();
    const d=state.documentValue?.design||{}; els.npcName.textContent=d.display_name||d.working_name||state.documentValue?.id||"NPC";
    if(!currentBeat){ els.meta.textContent=""; els.speech.textContent=ended?"Conversation ended.":"No eligible ENTRY beat for this state."; return; }
    els.meta.textContent=`${currentBeat.id} · P${currentBeat.priority} · ${currentBeat.pool||"mandatory"}`;
    els.speech.textContent=(currentBeat.lines||[]).map(v=>v.text).filter(Boolean).join("\n\n") || "(No dialogue text)";
    const choices=Array.isArray(currentBeat.choices)?currentBeat.choices:[];
    if(!choices.length){ const b=document.createElement("button"); b.className="test-choice"; b.textContent="Continue / End"; b.onclick=()=>call("complete",{beat_id:currentBeat.id}).catch(showError); els.choices.appendChild(b); return; }
    for(const choice of choices){ const b=document.createElement("button"); b.className="test-choice"; b.textContent=choice.text||choice.id; b.onclick=()=>call("advance",{beat_id:currentBeat.id,choice_id:choice.id}).catch(showError); els.choices.appendChild(b); }
  }
  function showError(error){ console.error(error); els.speech.textContent=`ERROR: ${error.message}`; }
  function openTest(){
    for(const s of document.querySelectorAll(".surface")) s.classList.remove("active"); surface.classList.add("active");
    for(const t of document.querySelectorAll(".future-tab")) t.classList.remove("active"); tab.classList.add("active");
    state.activeSurface="test"; currentBeat=null; render(false);
  }
  tab.addEventListener("click", openTest);
  for(const other of document.querySelectorAll(".future-tab[data-surface]")) other.addEventListener("click",()=>surface.classList.remove("active"));
  els.evaluate.addEventListener("click",()=>call("evaluate").catch(showError));
  els.reset.addEventListener("click",()=>{ writeState({facts:{},npc_met:[],dialogue_heard:[],item_owned:[],item_equipped:[]}); currentBeat=null; render(false); });
})();
