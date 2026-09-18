const state={items:[],selectedPath:null,doc:null,contentId:null,original:"",dirty:false};
const $=id=>document.getElementById(id);
const els={};
["monsterList","filterInput","newButton","reloadButton","saveButton","documentTitle","documentPath","dirtyBadge","emptyState","editor","idInput","contentIdInput","schemaInput","debugNameInput","healthInput","halfXInput","halfYInput","speedInput","leashInput","kindInput","aggroInput","jsonEditor","applyRawButton","validationStatus","validationErrors","summaryId","statusText"].forEach(id=>els[id]=$(id));
const canonical=v=>JSON.stringify(v);
const clone=v=>JSON.parse(JSON.stringify(v));
const number=v=>Number(v);
function setStatus(v){els.statusText.textContent=v}
function localErrors(doc){
  const e=[];
  if(!doc||typeof doc!=="object"||Array.isArray(doc))return["Monster document must be an object."];
  if(doc.schema_version!==2)e.push("schema_version must be 2.");
  if(typeof doc.id!=="string"||!/^monster\.[a-z0-9][a-z0-9._-]*$/.test(doc.id))e.push("id must use monster.*.");
  if(typeof doc.debug_name!=="string"||!doc.debug_name.trim())e.push("debug_name is required.");
  for(const k of ["health_max","movement_speed"])if(!(Number.isFinite(Number(doc[k]))&&Number(doc[k])>0))e.push(k+" must be > 0.");
  if(!Array.isArray(doc.half_extents)||doc.half_extents.length!==2||doc.half_extents.some(v=>!(Number.isFinite(Number(v))&&Number(v)>0)))e.push("half_extents must contain two positive numbers.");
  if(doc.behavior?.kind!=="chase_contact")e.push("behavior.kind must be chase_contact.");
  if(doc.behavior?.aggro!=="when_attacked")e.push("behavior.aggro must be when_attacked.");
  if(!(Number.isFinite(Number(doc.behavior?.home_leash_radius))&&Number(doc.behavior.home_leash_radius)>0))e.push("home_leash_radius must be > 0.");
  return e;
}
function updateDirty(){state.dirty=Boolean(state.selectedPath)&&canonical(state.doc)!==state.original;els.saveButton.disabled=!state.dirty;els.dirtyBadge.textContent=state.dirty?"DIRTY":"CLEAN";els.dirtyBadge.classList.toggle("dirty",state.dirty)}
function updateInspector(){const e=localErrors(state.doc);els.summaryId.textContent=state.doc?.id||"-";els.validationStatus.textContent=e.length?"Needs attention":"Schema v2 OK";els.validationStatus.className=e.length?"bad":"good";els.validationErrors.textContent=e.length?e.join("\n"):"Local shape valid. Save also runs the Rust runtime content validator."}
function syncRaw(){els.jsonEditor.value=state.doc?JSON.stringify(state.doc,null,2)+"\n":""}
function renderForm(){if(!state.doc)return;els.idInput.value=state.doc.id||"";els.contentIdInput.value=state.contentId??"UNALLOCATED";els.schemaInput.value=state.doc.schema_version??"";els.debugNameInput.value=state.doc.debug_name||"";els.healthInput.value=state.doc.health_max??"";els.halfXInput.value=state.doc.half_extents?.[0]??"";els.halfYInput.value=state.doc.half_extents?.[1]??"";els.speedInput.value=state.doc.movement_speed??"";els.leashInput.value=state.doc.behavior?.home_leash_radius??"";els.kindInput.value=state.doc.behavior?.kind||"chase_contact";els.aggroInput.value=state.doc.behavior?.aggro||"when_attacked";syncRaw();updateInspector();updateDirty()}
function applyForm(){if(!state.doc)return;state.doc.debug_name=els.debugNameInput.value;state.doc.health_max=number(els.healthInput.value);state.doc.half_extents=[number(els.halfXInput.value),number(els.halfYInput.value)];state.doc.movement_speed=number(els.speedInput.value);state.doc.behavior={kind:els.kindInput.value,aggro:els.aggroInput.value,home_leash_radius:number(els.leashInput.value)};syncRaw();updateInspector();updateDirty()}
["debugNameInput","healthInput","halfXInput","halfYInput","speedInput","leashInput","kindInput","aggroInput"].forEach(id=>els[id].addEventListener("input",applyForm));
function renderList(){const q=els.filterInput.value.trim().toLowerCase();els.monsterList.replaceChildren();state.items.filter(x=>!q||String(x.id||"").toLowerCase().includes(q)||String(x.debug_name||"").toLowerCase().includes(q)).forEach(item=>{const b=document.createElement("button");b.className="item"+(item.path===state.selectedPath?" selected":"")+(item.valid?"":" invalid");b.innerHTML="<strong>"+(item.debug_name||item.id||item.path)+"</strong><small>"+item.path+"</small>";b.onclick=()=>openMonster(item.path);els.monsterList.appendChild(b)})}
async function api(url,options){const r=await fetch(url,options);const data=await r.json();if(!r.ok)throw new Error(data.error||data.validation_errors?.join("\n")||"Request failed");return data}
async function loadList(){const data=await api("/api/monsters");state.items=data.items;renderList()}
async function openMonster(path){if(state.dirty&&!confirm("Discard unsaved changes?"))return;const data=await api("/api/monster?path="+encodeURIComponent(path));state.selectedPath=data.path;state.doc=data.document;state.contentId=data.content_id??null;state.original=canonical(state.doc);els.emptyState.classList.add("hidden");els.editor.classList.remove("hidden");els.documentTitle.textContent=state.doc.debug_name||state.doc.id;els.documentPath.textContent="content/definitions/monsters/"+data.path;renderForm();renderList();setStatus("Loaded "+data.path)}
els.filterInput.addEventListener("input",renderList);
els.reloadButton.onclick=async()=>{await loadList();if(state.selectedPath)await openMonster(state.selectedPath)};
els.applyRawButton.onclick=()=>{try{state.doc=JSON.parse(els.jsonEditor.value);renderForm();setStatus("Raw JSON applied in memory.")}catch(e){setStatus("Invalid JSON: "+e.message)}};
els.saveButton.onclick=async()=>{const errors=localErrors(state.doc);if(errors.length){updateInspector();setStatus("Fix validation errors before save.");return}setStatus("Running runtime content validation...");try{const data=await api("/api/monster?path="+encodeURIComponent(state.selectedPath),{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(state.doc)});state.original=canonical(state.doc);updateDirty();await loadList();setStatus("Saved and runtime-validated: "+data.path)}catch(e){setStatus(String(e.message||e))}};
els.newButton.onclick=async()=>{const id=prompt("Monster authored label (must already have a stable numeric ContentId allocation):","monster.");if(!id)return;const name=prompt("Debug name:","New Monster");if(!name)return;setStatus("Creating and validating...");try{const data=await api("/api/new",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({id,debug_name:name})});await loadList();await openMonster(data.path)}catch(e){setStatus(String(e.message||e))}};
loadList().catch(e=>setStatus(String(e.message||e)));
