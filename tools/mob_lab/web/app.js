const state={
  items:[],sprites:[],selectedPath:null,doc:null,contentId:null,original:"",dirty:false,
  presentation:null,previewImage:null,previewFrame:0,previewTimer:null
};
const $=id=>document.getElementById(id);
const els={};
[
  "monsterList","filterInput","newButton","reloadButton","saveButton","documentTitle","documentPath",
  "dirtyBadge","emptyState","editor","idInput","contentIdInput","schemaInput","debugNameInput",
  "spriteInput","healthInput","leftInput","rightInput","bottomInput","topInput","speedInput",
  "leashInput","kindInput","aggroInput","jsonEditor","applyRawButton","validationStatus",
  "validationErrors","summaryId","statusText","previewCanvas","previewUnavailable","spriteReadout",
  "hitboxReadout","anchorReadout"
].forEach(id=>els[id]=$(id));

const canonical=v=>JSON.stringify(v);
const number=v=>Number(v);
function setStatus(v){els.statusText.textContent=v}

function localErrors(doc){
  const e=[];
  if(!doc||typeof doc!=="object"||Array.isArray(doc))return["Monster document must be an object."];
  if(doc.schema_version!==4)e.push("schema_version must be 4.");
  if(typeof doc.id!=="string"||!/^monster\.[a-z0-9][a-z0-9._-]*$/.test(doc.id))e.push("id must use monster.*.");
  if(typeof doc.debug_name!=="string"||!doc.debug_name.trim())e.push("debug_name is required.");
  if(typeof doc.sprite!=="string"||!doc.sprite.trim())e.push("sprite is required.");
  if(state.sprites.length&&!state.sprites.some(sprite=>sprite.id===doc.sprite))e.push("sprite is not present in Graphic/creature.");
  for(const k of ["health_max","movement_speed"]){
    if(!(Number.isFinite(Number(doc[k]))&&Number(doc[k])>0))e.push(k+" must be > 0.");
  }
  const b=doc.collision_bounds;
  if(!b||typeof b!=="object")e.push("collision_bounds is required.");
  else{
    for(const k of ["left","right","bottom","top"]){
      if(!(Number.isFinite(Number(b[k]))&&Number(b[k])>=0))e.push("collision_bounds."+k+" must be >= 0.");
    }
    if(Number(b.left)+Number(b.right)<=0)e.push("horizontal collision span must be > 0.");
    if(Number(b.bottom)+Number(b.top)<=0)e.push("vertical collision span must be > 0.");
  }
  if(doc.behavior?.kind!=="chase_contact")e.push("behavior.kind must be chase_contact.");
  if(doc.behavior?.aggro!=="when_attacked")e.push("behavior.aggro must be when_attacked.");
  if(!(Number.isFinite(Number(doc.behavior?.home_leash_radius))&&Number(doc.behavior.home_leash_radius)>0)){
    e.push("home_leash_radius must be > 0.");
  }
  return e;
}

function updateDirty(){
  state.dirty=Boolean(state.selectedPath)&&canonical(state.doc)!==state.original;
  els.saveButton.disabled=!state.dirty;
  els.dirtyBadge.textContent=state.dirty?"DIRTY":"CLEAN";
  els.dirtyBadge.classList.toggle("dirty",state.dirty);
}
function updateInspector(){
  const e=localErrors(state.doc);
  els.summaryId.textContent=state.doc?.id||"-";
  els.validationStatus.textContent=e.length?"Needs attention":"Schema v4 OK";
  els.validationStatus.className=e.length?"bad":"good";
  els.validationErrors.textContent=e.length?e.join("\n"):"Local shape valid. Save also runs the Rust runtime content validator.";
}
function syncRaw(){els.jsonEditor.value=state.doc?JSON.stringify(state.doc,null,2)+"\n":""}
function stopPreviewTimer(){if(state.previewTimer){clearInterval(state.previewTimer);state.previewTimer=null}}
function previewContext(){return els.previewCanvas.getContext("2d")}

function drawPreview(){
  const ctx=previewContext(),w=els.previewCanvas.width,h=els.previewCanvas.height;
  ctx.clearRect(0,0,w,h);
  ctx.fillStyle="#090d10";ctx.fillRect(0,0,w,h);

  const centerX=w/2,pxPerWorld=150,floorY=h*0.80;
  const bounds=state.doc?.collision_bounds;
  const validBounds=bounds&&["left","right","bottom","top"].every(
    k=>Number.isFinite(Number(bounds[k]))&&Number(bounds[k])>=0
  )&&Number(bounds.left)+Number(bounds.right)>0&&Number(bounds.bottom)+Number(bounds.top)>0;
  const left=validBounds?Number(bounds.left):0.4;
  const right=validBounds?Number(bounds.right):0.4;
  const bottom=validBounds?Number(bounds.bottom):0.6;
  const top=validBounds?Number(bounds.top):0.6;
  const entityY=floorY-bottom*pxPerWorld;

  ctx.strokeStyle="#1d2830";ctx.lineWidth=1;
  for(let x=centerX%30;x<w;x+=30){ctx.beginPath();ctx.moveTo(x,0);ctx.lineTo(x,floorY);ctx.stroke()}
  for(let y=floorY%30;y<floorY;y+=30){ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(w,y);ctx.stroke()}

  ctx.fillStyle="#182229";ctx.fillRect(0,floorY,w,h-floorY);
  ctx.strokeStyle="#93a5b0";ctx.lineWidth=4;
  ctx.beginPath();ctx.moveTo(0,floorY);ctx.lineTo(w,floorY);ctx.stroke();
  ctx.fillStyle="#b6c5ce";ctx.font="700 14px Segoe UI, Arial, sans-serif";
  ctx.fillText("FLOOR / COLLIDER CONTACT SURFACE",14,floorY+24);

  ctx.strokeStyle="#52616c";ctx.lineWidth=1;
  ctx.beginPath();ctx.moveTo(centerX,0);ctx.lineTo(centerX,floorY);ctx.stroke();
  ctx.fillStyle="#75c9d6";ctx.beginPath();ctx.arc(centerX,entityY,5,0,Math.PI*2);ctx.fill();
  ctx.font="700 12px Segoe UI, Arial, sans-serif";
  ctx.fillText("ENTITY ORIGIN",centerX+10,entityY-8);

  const p=state.presentation,img=state.previewImage;
  let spriteBottomOffset=null;
  if(p&&img&&img.complete&&img.naturalWidth){
    const frames=p.idle_frames||[],frame=frames[state.previewFrame%frames.length]||0;
    const fw=p.frame_size_px[0],fh=p.frame_size_px[1],cols=Math.floor(img.naturalWidth/fw);
    const sx=(frame%cols)*fw,sy=Math.floor(frame/cols)*fh;
    const worldW=p.world_size[0],worldH=p.world_size[1];
    const dw=worldW*pxPerWorld,dh=worldH*pxPerWorld;
    ctx.imageSmoothingEnabled=false;
    ctx.drawImage(img,sx,sy,fw,fh,centerX-dw/2,entityY-dh/2,dw,dh);
    els.previewUnavailable.classList.add("hidden");
    els.spriteReadout.textContent="Sprite: "+p.manifest_id+" · "+worldW.toFixed(2)+"×"+worldH.toFixed(2)+" wu";
    spriteBottomOffset=bottom-worldH/2;
  }else{
    els.previewUnavailable.classList.remove("hidden");
    els.spriteReadout.textContent="Sprite: no runtime presentation";
  }

  if(validBounds){
    const x=centerX-left*pxPerWorld,y=entityY-top*pxPerWorld;
    const bw=(left+right)*pxPerWorld,bh=(bottom+top)*pxPerWorld;
    ctx.fillStyle="rgba(224,106,112,.20)";
    ctx.strokeStyle="#e06a70";ctx.lineWidth=3;
    ctx.fillRect(x,y,bw,bh);ctx.strokeRect(x,y,bw,bh);
    els.hitboxReadout.textContent=
      "Bounds L"+left.toFixed(2)+" R"+right.toFixed(2)+" B"+bottom.toFixed(2)+" T"+top.toFixed(2)+" wu";
  }else{
    els.hitboxReadout.textContent="Hitbox: invalid";
  }

  if(spriteBottomOffset===null)els.anchorReadout.textContent="Sprite floor offset: -";
  else if(Math.abs(spriteBottomOffset)<0.005)els.anchorReadout.textContent="Sprite bottom: ON FLOOR";
  else if(spriteBottomOffset>0)els.anchorReadout.textContent="Sprite bottom: "+spriteBottomOffset.toFixed(2)+" wu ABOVE floor";
  else els.anchorReadout.textContent="Sprite bottom: "+Math.abs(spriteBottomOffset).toFixed(2)+" wu BELOW floor";
}

async function api(url,options){
  const r=await fetch(url,options);
  const data=await r.json();
  if(!r.ok)throw new Error(data.error||data.validation_errors?.join("\n")||"Request failed");
  return data;
}

async function loadSprites(){
  const data=await api("/api/sprites");
  state.sprites=data.items||[];
  els.spriteInput.replaceChildren();
  for(const sprite of state.sprites){
    const option=document.createElement("option");
    option.value=sprite.id;
    option.textContent=sprite.id+"  ·  "+sprite.frame_size_px[0]+"×"+sprite.frame_size_px[1]+" px";
    els.spriteInput.appendChild(option);
  }
  if(data.issues?.length)setStatus("Sprite scan warning: "+data.issues.join(" | "));
}

async function loadPresentation(){
  stopPreviewTimer();
  state.presentation=null;state.previewImage=null;state.previewFrame=0;drawPreview();
  if(!state.doc?.sprite)return;
  try{
    const p=await api("/api/presentation?sprite="+encodeURIComponent(state.doc.sprite));
    state.presentation=p;
    const img=new Image();
    img.onload=()=>{
      state.previewImage=img;drawPreview();stopPreviewTimer();
      state.previewTimer=setInterval(()=>{
        state.previewFrame+=1;drawPreview();
      },Math.max(50,Math.round((p.frame_seconds||0.1)*1000)));
    };
    img.onerror=()=>{state.previewImage=null;drawPreview()};
    img.src=p.atlas_url+"&v="+Date.now();
  }catch(e){
    setStatus("Preview: "+String(e.message||e));drawPreview();
  }
}

function renderSpriteOptions(){
  const selected=state.doc?.sprite||"";
  if(selected&&!state.sprites.some(sprite=>sprite.id===selected)){
    const option=document.createElement("option");
    option.value=selected;option.textContent=selected+"  ·  MISSING";
    els.spriteInput.appendChild(option);
  }
  els.spriteInput.value=selected;
}

function renderForm(){
  if(!state.doc)return;
  els.idInput.value=state.doc.id||"";
  els.contentIdInput.value=state.contentId??"UNALLOCATED";
  els.schemaInput.value=state.doc.schema_version??"";
  els.debugNameInput.value=state.doc.debug_name||"";
  renderSpriteOptions();
  els.healthInput.value=state.doc.health_max??"";
  const b=state.doc.collision_bounds||{};
  els.leftInput.value=b.left??"";els.rightInput.value=b.right??"";
  els.bottomInput.value=b.bottom??"";els.topInput.value=b.top??"";
  els.speedInput.value=state.doc.movement_speed??"";
  els.leashInput.value=state.doc.behavior?.home_leash_radius??"";
  els.kindInput.value=state.doc.behavior?.kind||"chase_contact";
  els.aggroInput.value=state.doc.behavior?.aggro||"when_attacked";
  syncRaw();updateInspector();updateDirty();drawPreview();
}

function applyForm(){
  if(!state.doc)return;
  state.doc.debug_name=els.debugNameInput.value;
  state.doc.sprite=els.spriteInput.value;
  state.doc.health_max=number(els.healthInput.value);
  state.doc.collision_bounds={
    left:number(els.leftInput.value),right:number(els.rightInput.value),
    bottom:number(els.bottomInput.value),top:number(els.topInput.value)
  };
  state.doc.movement_speed=number(els.speedInput.value);
  state.doc.behavior={
    kind:els.kindInput.value,aggro:els.aggroInput.value,
    home_leash_radius:number(els.leashInput.value)
  };
  syncRaw();updateInspector();updateDirty();drawPreview();
}

[
  "debugNameInput","healthInput","leftInput","rightInput","bottomInput","topInput",
  "speedInput","leashInput","kindInput","aggroInput"
].forEach(id=>els[id].addEventListener("input",applyForm));
els.spriteInput.addEventListener("change",async()=>{applyForm();await loadPresentation()});

function renderList(){
  const q=els.filterInput.value.trim().toLowerCase();
  els.monsterList.replaceChildren();
  state.items
    .filter(x=>!q||String(x.id||"").toLowerCase().includes(q)||String(x.debug_name||"").toLowerCase().includes(q))
    .forEach(item=>{
      const b=document.createElement("button");
      b.className="item"+(item.path===state.selectedPath?" selected":"")+(item.valid?"":" invalid");
      b.innerHTML="<strong>"+(item.debug_name||item.id||item.path)+"</strong><small>"+item.path+" · "+(item.sprite||"NO SPRITE")+"</small>";
      b.onclick=()=>openMonster(item.path);
      els.monsterList.appendChild(b);
    });
}

async function loadList(){
  const data=await api("/api/monsters");
  state.items=data.items;renderList();
}

async function openMonster(path){
  if(state.dirty&&!confirm("Discard unsaved changes?"))return;
  const data=await api("/api/monster?path="+encodeURIComponent(path));
  state.selectedPath=data.path;state.doc=data.document;state.contentId=data.content_id??null;
  state.original=canonical(state.doc);
  els.emptyState.classList.add("hidden");els.editor.classList.remove("hidden");
  els.documentTitle.textContent=state.doc.debug_name||state.doc.id;
  els.documentPath.textContent="content/definitions/monsters/"+data.path;
  renderForm();renderList();await loadPresentation();setStatus("Loaded "+data.path);
}

els.filterInput.addEventListener("input",renderList);
els.reloadButton.onclick=async()=>{
  await loadSprites();await loadList();
  if(state.selectedPath)await openMonster(state.selectedPath);
};
els.applyRawButton.onclick=async()=>{
  try{
    state.doc=JSON.parse(els.jsonEditor.value);
    renderForm();await loadPresentation();setStatus("Raw JSON applied in memory.");
  }catch(e){setStatus("Invalid JSON: "+e.message)}
};
els.saveButton.onclick=async()=>{
  const errors=localErrors(state.doc);
  if(errors.length){updateInspector();setStatus("Fix validation errors before save.");return}
  setStatus("Running runtime content validation...");
  try{
    const data=await api("/api/monster?path="+encodeURIComponent(state.selectedPath),{
      method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(state.doc)
    });
    state.original=canonical(state.doc);updateDirty();await loadList();
    setStatus("Saved and runtime-validated: "+data.path);
  }catch(e){setStatus(String(e.message||e))}
};
els.newButton.onclick=async()=>{
  const id=prompt("Monster authored label (must already have a stable numeric ContentId allocation):","monster.");
  if(!id)return;
  const name=prompt("Debug name:","New Monster");
  if(!name)return;
  setStatus("Creating and validating...");
  try{
    const data=await api("/api/new",{
      method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({id,debug_name:name})
    });
    await loadList();await openMonster(data.path);
  }catch(e){setStatus(String(e.message||e))}
};

(async()=>{
  try{await loadSprites();await loadList();setStatus("Ready.");}
  catch(e){setStatus(String(e.message||e))}
})();
window.addEventListener("beforeunload",stopPreviewTimer);
