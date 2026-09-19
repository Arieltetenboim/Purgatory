const state={
  items:[],sprites:[],selectedPath:null,doc:null,contentId:null,original:"",dirty:false,
  presentation:null,previewImage:null,previewFrame:0,previewTimer:null,
  manifestDoc:null,manifestOriginal:"",manifestPath:null,manifestDirty:false,previewClip:"idle",activeTab:"atlas",
  selectedFrame:0,previewPlaying:true,showSprite:true,showHitbox:true,showGuides:true,newTemplatePath:null
};
const $=id=>document.getElementById(id);
const els={};
[
  "monsterList","filterInput","newButton","reloadButton","saveButton","documentTitle","documentPath",
  "dirtyBadge","emptyState","editor","idInput","contentIdInput","schemaInput","debugNameInput",
  "spriteInput","healthInput","leftInput","rightInput","bottomInput","topInput","speedInput",
  "leashInput","kindInput","aggroInput","jsonEditor","applyRawButton","validationStatus",
  "validationErrors","summaryId","statusText","previewCanvas","previewUnavailable","spriteReadout",
  "hitboxReadout","anchorReadout","newMonsterDialog","newMonsterForm","newIdInput","newNameInput",
  "newSpriteInput","newCancelButton","newCreateButton","monsterCount","manifestDirtyBadge","manifestIdInput",
  "manifestAtlasInput","manifestGridColsInput","manifestGridRowsInput","manifestFrameWidthInput",
  "manifestFrameHeightInput","manifestWorldWidthInput","manifestWorldHeightInput",
  "manifestFrameSecondsInput","manifestFacingInput","manifestPreviewClipInput",
  "manifestMoveFramesInput","manifestIdleFramesInput","manifestAttackFramesInput",
  "manifestJsonEditor","applyManifestRawButton","saveManifestButton","resetManifestButton",
  "duplicateButton","typeFilterInput","previewAnimationInput","previewPlayButton","previewSpriteToggle",
  "previewHitboxToggle","previewGuidesToggle","sheetGridReadout","frameSheetGrid","selectedFrameCanvas",
  "selectedFrameIndex","selectedFramePixels","selectedFrameWorld","quickThumbCanvas","quickTypeInput",
  "quickSourceFile","quickAtlas","quickImageSize","quickFrames","quickFrameSize","quickWorldSize",
  "footerMonsterCount","footerSchemaStatus","footerDirtyStatus"
].forEach(id=>els[id]=$(id));

const canonical=v=>JSON.stringify(v);
const number=v=>Number(v);
const int=v=>Math.trunc(Number(v));
const framesText=frames=>Array.isArray(frames)?frames.join(","):"";
function parseFrames(value){
  const text=String(value??"").trim();
  if(!text)return[];
  return text.split(",").map(part=>Number(part.trim())).filter(Number.isInteger);
}

function derivedType(itemOrId){
  const id=typeof itemOrId==="string"?itemOrId:itemOrId?.id;
  const part=String(id||"").split(".")[1]||"other";
  return part.replaceAll("_"," ");
}
function spriteRecord(id){return state.sprites.find(sprite=>sprite.id===id)||null}
function atlasUrl(spriteId){return "/api/presentation-atlas?sprite="+encodeURIComponent(spriteId)}
function frameGeometry(){
  const p=state.presentation,img=state.previewImage;
  if(!p||!img||!img.naturalWidth)return null;
  const cols=Math.max(1,Math.trunc(Number(p.grid_size?.[0])||Math.floor(img.naturalWidth/p.frame_size_px[0])||1));
  const rows=Math.max(1,Math.trunc(Number(p.grid_size?.[1])||Math.floor(img.naturalHeight/p.frame_size_px[1])||1));
  return {cols,rows,fw:img.naturalWidth/cols,fh:img.naturalHeight/rows,total:cols*rows};
}
function drawAtlasFrame(ctx,frame,dw,dh){
  const g=frameGeometry(),img=state.previewImage;
  if(!g||!img)return false;
  const index=Math.max(0,Math.min(g.total-1,Number(frame)||0));
  const sx=(index%g.cols)*g.fw,sy=Math.floor(index/g.cols)*g.fh;
  ctx.clearRect(0,0,dw,dh);
  ctx.imageSmoothingEnabled=false;
  ctx.drawImage(img,sx,sy,g.fw,g.fh,0,0,dw,dh);
  return true;
}
function renderSelectedFrame(){
  const g=frameGeometry(),canvas=els.selectedFrameCanvas;
  if(!g||!canvas)return;
  state.selectedFrame=Math.max(0,Math.min(g.total-1,state.selectedFrame));
  drawAtlasFrame(canvas.getContext("2d"),state.selectedFrame,canvas.width,canvas.height);
  els.selectedFrameIndex.textContent=String(state.selectedFrame);
  els.selectedFramePixels.textContent=Math.round(g.fw)+" × "+Math.round(g.fh);
  const w=Number(state.presentation?.world_size?.[0])||1,h=Number(state.presentation?.world_size?.[1])||1;
  els.selectedFrameWorld.textContent=w.toFixed(2)+" × "+h.toFixed(2)+" wu";
}
function renderFrameSheet(){
  const g=frameGeometry();
  els.frameSheetGrid.replaceChildren();
  if(!g)return;
  els.frameSheetGrid.style.setProperty("--sheet-cols",String(g.cols));
  els.sheetGridReadout.textContent="("+g.cols+" × "+g.rows+")";
  for(let index=0;index<g.total;index++){
    const button=document.createElement("button");
    button.type="button";
    button.className="frame-cell"+(index===state.selectedFrame?" selected":"");
    const canvas=document.createElement("canvas");
    canvas.width=96;canvas.height=96;
    drawAtlasFrame(canvas.getContext("2d"),index,canvas.width,canvas.height);
    const badge=document.createElement("span");
    badge.className="frame-index-badge";badge.textContent=String(index);
    button.append(canvas,badge);
    button.onclick=()=>{state.selectedFrame=index;renderFrameSheet();renderSelectedFrame()};
    els.frameSheetGrid.appendChild(button);
  }
  renderSelectedFrame();
}
function renderQuickThumb(){
  const canvas=els.quickThumbCanvas;
  if(!canvas)return;
  drawAtlasFrame(canvas.getContext("2d"),0,canvas.width,canvas.height);
}
function updateQuickInfo(){
  if(!state.doc)return;
  const g=frameGeometry();
  els.quickTypeInput.value=derivedType(state.doc);
  els.quickSourceFile.textContent=state.selectedPath||"-";
  els.quickAtlas.textContent=state.manifestDoc?.atlas||"-";
  els.quickImageSize.textContent=state.previewImage?.naturalWidth?state.previewImage.naturalWidth+" × "+state.previewImage.naturalHeight:"-";
  els.quickFrames.textContent=g?g.total+" ("+g.cols+" × "+g.rows+")":"-";
  els.quickFrameSize.textContent=g?Math.round(g.fw)+" × "+Math.round(g.fh):"-";
  const world=state.manifestDoc?.world_size;
  els.quickWorldSize.textContent=Array.isArray(world)?Number(world[0]).toFixed(2)+" × "+Number(world[1]).toFixed(2)+" wu":"-";
  renderQuickThumb();
}
function populateTypeFilter(){
  const current=els.typeFilterInput.value;
  const types=[...new Set(state.items.map(item=>derivedType(item)).filter(Boolean))].sort();
  els.typeFilterInput.replaceChildren(new Option("All Types",""));
  for(const type of types)els.typeFilterInput.appendChild(new Option(type.replace(/\b\w/g,c=>c.toUpperCase()),type));
  els.typeFilterInput.value=types.includes(current)?current:"";
}

function setStatus(v){els.statusText.textContent=v}
function setEditorTab(name){
  state.activeTab=name;
  document.querySelectorAll(".editor-tab").forEach(button=>{
    button.classList.toggle("active",button.dataset.tab===name);
  });
  document.querySelectorAll(".tab-panel").forEach(panel=>{
    panel.classList.toggle("active",panel.dataset.panel===name);
  });
}
document.querySelectorAll(".editor-tab").forEach(button=>{
  button.addEventListener("click",()=>setEditorTab(button.dataset.tab));
});


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
  els.footerDirtyStatus.textContent=(state.dirty||state.manifestDirty)?"Unsaved changes":"Saved";
  els.footerDirtyStatus.classList.toggle("warn",state.dirty||state.manifestDirty);
}
function updateInspector(){
  const e=localErrors(state.doc);
  els.summaryId.textContent=state.doc?.id||"-";
  els.validationStatus.textContent=e.length?"Needs attention":"Schema v4 OK";
  els.validationStatus.className=e.length?"bad":"good";
  els.validationErrors.textContent=e.length?e.join("\n"):"Local shape valid. Save also runs the Rust runtime content validator.";
  els.footerSchemaStatus.textContent=e.length?"Schema needs attention":"Schema v4 OK";
  els.footerSchemaStatus.className=e.length?"bad":"good";
}
function syncRaw(){els.jsonEditor.value=state.doc?JSON.stringify(state.doc,null,2)+"\n":""}
function updateManifestDirty(){
  state.manifestDirty=Boolean(state.manifestDoc)&&canonical(state.manifestDoc)!==state.manifestOriginal;
  els.saveManifestButton.disabled=!state.manifestDirty;
  els.manifestDirtyBadge.textContent=state.manifestDirty?"MANIFEST DIRTY":"MANIFEST CLEAN";
  els.manifestDirtyBadge.classList.toggle("dirty",state.manifestDirty);
  if(els.footerDirtyStatus){
    els.footerDirtyStatus.textContent=(state.dirty||state.manifestDirty)?"Unsaved changes":"Saved";
    els.footerDirtyStatus.classList.toggle("warn",state.dirty||state.manifestDirty);
  }
}
function syncManifestRaw(){
  els.manifestJsonEditor.value=state.manifestDoc?JSON.stringify(state.manifestDoc,null,2)+"\n":"";
}
function manifestToPresentation(){
  if(!state.manifestDoc||!state.presentation)return;
  const doc=state.manifestDoc,clip=doc.clips?.[state.previewClip]||doc.clips?.idle;
  state.presentation.frame_size_px=doc.frame_size_px;
  state.presentation.grid_size=doc.grid_size||null;
  state.presentation.world_size=doc.world_size;
  state.presentation.frame_seconds=Number(doc.frame_seconds);
  state.presentation.idle_frames=Array.isArray(clip?.frames)?clip.frames:[0];
  state.presentation.authored_facing=doc.authored_facing;
}
function restartPreviewTimer(){
  stopPreviewTimer();
  const seconds=Number(state.presentation?.frame_seconds)||0.1;
  if(!state.previewPlaying)return;
  state.previewTimer=setInterval(()=>{state.previewFrame+=1;drawPreview()},Math.max(30,Math.round(seconds*1000)));
}
function renderManifestForm(){
  const m=state.manifestDoc;
  if(!m)return;
  els.manifestIdInput.value=m.id||"";
  els.manifestAtlasInput.value=m.atlas||"";
  els.manifestGridColsInput.value=m.grid_size?.[0]??"";
  els.manifestGridRowsInput.value=m.grid_size?.[1]??"";
  els.manifestFrameWidthInput.value=m.frame_size_px?.[0]??"";
  els.manifestFrameHeightInput.value=m.frame_size_px?.[1]??"";
  els.manifestWorldWidthInput.value=m.world_size?.[0]??"";
  els.manifestWorldHeightInput.value=m.world_size?.[1]??"";
  els.manifestFrameSecondsInput.value=m.frame_seconds??"";
  els.manifestFacingInput.value=m.authored_facing||"right";
  els.manifestPreviewClipInput.value=state.previewClip;
  els.previewAnimationInput.value=state.previewClip;
  els.manifestMoveFramesInput.value=framesText(m.clips?.move?.frames);
  els.manifestIdleFramesInput.value=framesText(m.clips?.idle?.frames);
  els.manifestAttackFramesInput.value=framesText(m.clips?.attack?.frames);
  syncManifestRaw();updateManifestDirty();manifestToPresentation();drawPreview();
}
function applyManifestForm(){
  const m=state.manifestDoc;
  if(!m)return;
  m.grid_size=[int(els.manifestGridColsInput.value),int(els.manifestGridRowsInput.value)];
  m.frame_size_px=[int(els.manifestFrameWidthInput.value),int(els.manifestFrameHeightInput.value)];
  m.world_size=[number(els.manifestWorldWidthInput.value),number(els.manifestWorldHeightInput.value)];
  m.frame_seconds=number(els.manifestFrameSecondsInput.value);
  m.authored_facing=els.manifestFacingInput.value;
  m.clips=m.clips||{};
  for(const [name,input] of [["move",els.manifestMoveFramesInput],["idle",els.manifestIdleFramesInput],["attack",els.manifestAttackFramesInput]]){
    m.clips[name]=m.clips[name]||{frames:[],loop:name!=="attack"};
    m.clips[name].frames=parseFrames(input.value);
  }
  state.previewClip=els.manifestPreviewClipInput.value;
  els.previewAnimationInput.value=state.previewClip;
  syncManifestRaw();updateManifestDirty();manifestToPresentation();state.previewFrame=0;renderFrameSheet();updateQuickInfo();restartPreviewTimer();drawPreview();
}

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

  if(state.showGuides){
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
  }

  const p=state.presentation,img=state.previewImage;
  let spriteBottomOffset=null;
  if(p&&img&&img.complete&&img.naturalWidth){
    const frames=p.idle_frames||[],frame=frames[state.previewFrame%frames.length]||0;
    const grid=p.grid_size;
    const cols=Math.max(1,Math.trunc(Number(grid?.[0])||Math.floor(img.naturalWidth/p.frame_size_px[0])||1));
    const rows=Math.max(1,Math.trunc(Number(grid?.[1])||Math.floor(img.naturalHeight/p.frame_size_px[1])||1));
    const fw=img.naturalWidth/cols,fh=img.naturalHeight/rows;
    const sx=(frame%cols)*fw,sy=Math.floor(frame/cols)*fh;
    const worldW=Math.max(0.01,Number(p.world_size?.[0])||1),worldH=Math.max(0.01,Number(p.world_size?.[1])||1);
    const dw=worldW*pxPerWorld,dh=worldH*pxPerWorld;
    if(state.showSprite){
      ctx.imageSmoothingEnabled=false;
      ctx.drawImage(img,sx,sy,fw,fh,centerX-dw/2,entityY-dh/2,dw,dh);
    }
    els.previewUnavailable.classList.add("hidden");
    spriteBottomOffset=bottom-worldH/2;
  }else{
    els.previewUnavailable.classList.remove("hidden");
  }

  if(validBounds&&state.showHitbox){
    const x=centerX-left*pxPerWorld,y=entityY-top*pxPerWorld;
    const bw=(left+right)*pxPerWorld,bh=(bottom+top)*pxPerWorld;
    ctx.fillStyle="rgba(224,106,112,.20)";
    ctx.strokeStyle="#e06a70";ctx.lineWidth=3;
    ctx.fillRect(x,y,bw,bh);ctx.strokeRect(x,y,bw,bh);
  }
  const clipFrames=state.presentation?.idle_frames||[];
  const clipLength=Math.max(clipFrames.length,1);
  const clipFrame=(state.previewFrame%clipLength)+1;
  els.spriteReadout.textContent="Frame "+clipFrame+"/"+clipLength;
  els.hitboxReadout.textContent=(Number(state.presentation?.frame_seconds)||0.1).toFixed(2)+"s";
  els.anchorReadout.textContent="1.00×";
}

async function api(url,options){
  const r=await fetch(url,options);
  const data=await r.json();
  if(!r.ok){
    let message=data.error||data.validation_errors?.join("\n")||"Request failed";
    if(data.validator_output)message+="\n\n"+data.validator_output;
    throw new Error(message);
  }
  return data;
}

async function loadSprites(){
  const data=await api("/api/sprites");
  state.sprites=data.items||[];
  if(!els.spriteInput)throw new Error("Mob Lab HTML/JS version mismatch: spriteInput is missing. Reload the page.");
  els.spriteInput.replaceChildren();
  if(els.newSpriteInput)els.newSpriteInput.replaceChildren();
  for(const sprite of state.sprites){
    const grid=sprite.grid_size?(" · "+sprite.grid_size[0]+"×"+sprite.grid_size[1]+" grid"):"";
    const label=sprite.id+"  ·  "+sprite.frame_size_px[0]+"×"+sprite.frame_size_px[1]+" px"+grid;
    const option=document.createElement("option");
    option.value=sprite.id;
    option.textContent=label;
    els.spriteInput.appendChild(option);
    if(els.newSpriteInput){
      const newOption=document.createElement("option");
      newOption.value=sprite.id;
      newOption.textContent=label;
      els.newSpriteInput.appendChild(newOption);
    }
  }
  if(data.issues?.length)setStatus("Sprite scan warning: "+data.issues.join(" | "));
}

async function loadPresentation(){
  stopPreviewTimer();
  state.presentation=null;state.previewImage=null;state.previewFrame=0;
  state.manifestDoc=null;state.manifestOriginal="";state.manifestPath=null;updateManifestDirty();drawPreview();
  if(!state.doc?.sprite)return;
  try{
    const sprite=encodeURIComponent(state.doc.sprite);
    const [p,m]=await Promise.all([
      api("/api/presentation?sprite="+sprite),
      api("/api/sprite-manifest?sprite="+sprite)
    ]);
    state.presentation=p;
    state.manifestDoc=m.document;
    state.manifestOriginal=canonical(m.document);
    state.manifestPath=m.path;
    renderManifestForm();
    const img=new Image();
    img.onload=()=>{
      state.previewImage=img;
      els.previewUnavailable.classList.add("hidden");
      manifestToPresentation();drawPreview();renderFrameSheet();updateQuickInfo();restartPreviewTimer();
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
  syncRaw();updateInspector();updateDirty();drawPreview();updateQuickInfo();
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
els.previewAnimationInput.addEventListener("change",()=>{
  state.previewClip=els.previewAnimationInput.value;
  els.manifestPreviewClipInput.value=state.previewClip;
  manifestToPresentation();state.previewFrame=0;restartPreviewTimer();drawPreview();
});
els.previewPlayButton.onclick=()=>{
  state.previewPlaying=!state.previewPlaying;
  els.previewPlayButton.textContent=state.previewPlaying?"Ⅱ":"▶";
  els.previewPlayButton.title=state.previewPlaying?"Pause preview":"Play preview";
  restartPreviewTimer();
};
els.previewSpriteToggle.addEventListener("change",()=>{state.showSprite=els.previewSpriteToggle.checked;drawPreview()});
els.previewHitboxToggle.addEventListener("change",()=>{state.showHitbox=els.previewHitboxToggle.checked;drawPreview()});
els.previewGuidesToggle.addEventListener("change",()=>{state.showGuides=els.previewGuidesToggle.checked;drawPreview()});

[
  "manifestGridColsInput","manifestGridRowsInput","manifestFrameWidthInput","manifestFrameHeightInput",
  "manifestWorldWidthInput","manifestWorldHeightInput","manifestFrameSecondsInput","manifestFacingInput",
  "manifestPreviewClipInput","manifestMoveFramesInput","manifestIdleFramesInput","manifestAttackFramesInput"
].forEach(id=>els[id].addEventListener("input",applyManifestForm));
els.applyManifestRawButton.onclick=()=>{
  try{
    const parsed=JSON.parse(els.manifestJsonEditor.value);
    if(parsed.id!==state.doc?.sprite)throw new Error("Manifest id must stay "+state.doc?.sprite);
    state.manifestDoc=parsed;renderManifestForm();restartPreviewTimer();
    setStatus("Raw manifest applied in memory. Preview is live; SAVE MANIFEST persists it.");
  }catch(e){setStatus("Invalid manifest JSON: "+e.message)}
};
els.resetManifestButton.onclick=()=>{
  if(!state.manifestOriginal)return;
  state.manifestDoc=JSON.parse(state.manifestOriginal);renderManifestForm();restartPreviewTimer();
  setStatus("Manifest changes reset.");
};
els.saveManifestButton.onclick=async()=>{
  if(!state.manifestDoc||!state.doc?.sprite)return;
  setStatus("Validating and saving sprite manifest...");
  try{
    const data=await api("/api/sprite-manifest?sprite="+encodeURIComponent(state.doc.sprite),{
      method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(state.manifestDoc)
    });
    state.manifestOriginal=canonical(state.manifestDoc);updateManifestDirty();
    await loadSprites();
    await loadPresentation();
    setStatus("Saved manifest: "+data.path);
  }catch(e){setStatus(String(e.message||e))}
};


function renderList(){
  const q=els.filterInput.value.trim().toLowerCase();
  const type=els.typeFilterInput.value;
  els.monsterCount.textContent=String(state.items.length);
  els.footerMonsterCount.textContent="Loaded "+state.items.length+" monster"+(state.items.length===1?"":"s");
  els.monsterList.replaceChildren();
  state.items
    .filter(item=>{
      const text=(String(item.id||"")+" "+String(item.debug_name||"")).toLowerCase();
      return (!q||text.includes(q))&&(!type||derivedType(item)===type);
    })
    .forEach(item=>{
      const b=document.createElement("button");
      b.className="item monster-card"+(item.path===state.selectedPath?" selected":"")+(item.valid?"":" invalid");

      const thumb=document.createElement("div");
      thumb.className="monster-thumb";
      const sprite=spriteRecord(item.sprite);
      if(sprite){
        const cols=Math.max(1,Number(sprite.grid_size?.[0])||1);
        const rows=Math.max(1,Number(sprite.grid_size?.[1])||1);
        thumb.style.backgroundImage='url("'+atlasUrl(item.sprite)+'")';
        thumb.style.backgroundSize=(cols*100)+"% "+(rows*100)+"%";
        thumb.style.backgroundPosition="0% 0%";
      }

      const copy=document.createElement("div");
      copy.className="monster-card-copy";
      const name=document.createElement("strong");
      name.textContent=item.debug_name||item.id||item.path;
      const sub=document.createElement("small");
      sub.textContent=item.id||item.path;
      copy.append(name,sub);

      const contentId=document.createElement("span");
      contentId.className="monster-content-id";
      contentId.textContent=item.content_id??"—";

      b.append(thumb,copy,contentId);
      b.onclick=()=>openMonster(item.path);
      els.monsterList.appendChild(b);
    });
}
async function loadList(){
  const data=await api("/api/monsters");
  state.items=data.items;populateTypeFilter();renderList();
}

async function openMonster(path){
  if(state.dirty&&!confirm("Discard unsaved changes?"))return;
  const data=await api("/api/monster?path="+encodeURIComponent(path));
  state.selectedPath=data.path;state.doc=data.document;state.contentId=data.content_id??null;
  state.original=canonical(state.doc);
  els.duplicateButton.disabled=false;
  els.emptyState.classList.add("hidden");els.editor.classList.remove("hidden");
  els.documentTitle.textContent=state.doc.debug_name||state.doc.id;
  els.documentPath.textContent="content/definitions/monsters/"+data.path;
  state.selectedFrame=0;
  renderForm();renderList();setEditorTab(state.activeTab||"atlas");await loadPresentation();updateQuickInfo();setStatus("Loaded "+data.path);
}

els.filterInput.addEventListener("input",renderList);
els.typeFilterInput.addEventListener("change",renderList);
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
els.newButton.onclick=()=>{
  state.newTemplatePath=null;
  if(!els.newMonsterDialog||!els.newSpriteInput){
    setStatus("Mob Lab page is stale. Close this tab and relaunch Mob Lab.");
    return;
  }
  if(!state.sprites.length){
    setStatus("No valid creature sprites were found. Add/fix a manifest under Graphic/creature first.");
    return;
  }
  els.newIdInput.value="monster.";
  els.newNameInput.value="New Monster";
  els.newSpriteInput.value=state.sprites[0].id;
  els.newMonsterDialog.showModal();
  setTimeout(()=>els.newIdInput.focus(),0);
};
els.duplicateButton.onclick=()=>{
  if(!state.doc||!state.selectedPath)return;
  state.newTemplatePath=state.selectedPath;
  els.newIdInput.value=(state.doc.id||"monster.")+".copy";
  els.newNameInput.value=(state.doc.debug_name||"Monster")+" Copy";
  els.newSpriteInput.value=state.doc.sprite||state.sprites[0]?.id||"";
  els.newMonsterDialog.showModal();
  setTimeout(()=>els.newIdInput.focus(),0);
};
els.newCancelButton.onclick=()=>els.newMonsterDialog.close();
els.newMonsterForm.addEventListener("submit",async event=>{
  event.preventDefault();
  const id=els.newIdInput.value.trim();
  const name=els.newNameInput.value.trim();
  const sprite=els.newSpriteInput.value;
  if(!id||!name||!sprite)return;
  els.newCreateButton.disabled=true;
  setStatus("Allocating ContentId and validating new Monster...");
  try{
    const data=await api("/api/new",{
      method:"POST",
      headers:{"Content-Type":"application/json"},
      body:JSON.stringify({id,debug_name:name,sprite,template_path:state.newTemplatePath})
    });
    els.newMonsterDialog.close();
    state.newTemplatePath=null;
    await loadList();
    await openMonster(data.path);
    setStatus("Created "+data.document.id+" · ContentId "+data.content_id+" · "+data.document.sprite);
  }catch(e){
    setStatus(String(e.message||e));
  }finally{
    els.newCreateButton.disabled=false;
  }
});

(async()=>{
  try{
    await loadSprites();
    await loadList();
    if(!state.selectedPath&&state.items.length){
      await openMonster(state.items[0].path);
    }else{
      setStatus("Ready.");
    }
  }catch(e){setStatus(String(e.message||e))}
})();
window.addEventListener("beforeunload",stopPreviewTimer);
