const state={
  items:[],sprites:[],selectedPath:null,doc:null,contentId:null,original:"",dirty:false,
  presentation:null,previewImage:null,previewFrame:0,previewTimer:null,previewTime:0,previewStarted:false,previewFinished:false,markerFeedback:"",
  manifestDoc:null,manifestOriginal:"",manifestPath:null,manifestDirty:false,previewClip:"idle",activeTab:"atlas",
  selectedFrame:0,selectedSocket:null,socketPlacement:false,previewPlaying:false,showSprite:true,showHitbox:true,showGuides:false,newTemplatePath:null
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
  "manifestFrameSecondsInput","manifestPixelsPerUnitInput","manifestFacingInput","manifestPreviewClipInput","manifestClipList",
  "manifestJsonEditor","applyManifestRawButton","saveManifestButton","resetManifestButton",
  "duplicateButton","typeFilterInput","previewAnimationInput","previewPlayButton","previewSpriteToggle",
  "previewStopButton","previewHitboxToggle","previewGuidesToggle","sheetGridReadout","frameSheetGrid","selectedFrameCanvas",
  "selectedFrameIndex","selectedFramePixels","selectedFrameWorld","quickThumbCanvas","quickTypeInput",
  "quickSourceFile","quickAtlas","quickImageSize","quickFrames","quickFrameSize","quickWorldSize",
  "footerMonsterCount","footerSchemaStatus","footerDirtyStatus","docContentId","docSchema",
  "atlasCanvas","v2FrameList","v2FrameIndexInput","v2RectXInput","v2RectYInput",
  "v2RectWidthInput","v2RectHeightInput","v2OriginXInput","v2OriginYInput",
  "addFrameButton","deleteFrameButton","frameActionMessage","addSocketButton","socketList","socketMessage",
  "v1AnimationEditor","v2AnimationEditor","v2ClipSelect","addClipButton","renameClipButton","deleteClipButton",
  "v2ClipLoopInput","animationMessage","v2StepList","v2AnnotationList","addStepButton","deleteStepButton",
  "moveStepEarlierButton","moveStepLaterButton","addAnnotationButton","v2Timeline","timelineTotal","annotationFeedback"
].forEach(id=>els[id]=$(id));

const canonical=v=>JSON.stringify(v);
const number=v=>Number(v);
const int=v=>Math.trunc(Number(v));
const framesText=frames=>Array.isArray(frames)?frames.join(","):"";
const isV2=doc=>Number(doc?.schema_version)===2;
function v2Frame(doc,index){return isV2(doc)&&Array.isArray(doc.frames)?doc.frames[index]:null}
const authoredId=/^[a-z0-9][a-z0-9._-]*$/;
function v2Clip(doc,name){return doc?.clips?.[name]||null}
function clipDuration(clip){
  return (clip?.steps||[]).reduce((total,step)=>total+(Number.isInteger(step?.duration_ms)&&step.duration_ms>0?step.duration_ms:0),0);
}
function stepStartTime(clip,index){
  return (clip?.steps||[]).slice(0,Math.max(0,index)).reduce((total,step)=>total+Number(step?.duration_ms||0),0);
}
function annotationAbsoluteTime(clip,annotation){
  return stepStartTime(clip,annotation?.step)+Number(annotation?.offset_ms||0);
}
function timelinePlacement(clip){
  const total=clipDuration(clip)||1;
  return (clip?.steps||[]).map((step,index)=>({
    index,frame:step.frame,duration:step.duration_ms,start:stepStartTime(clip,index),width:step.duration_ms/total
  }));
}
function remapAnnotationStepOnMove(annotations,from,to){
  return (annotations||[]).map(annotation=>{
    const step=annotation.step;
    if(step===from)return {...annotation,step:to};
    if(from<to&&step>from&&step<=to)return {...annotation,step:step-1};
    if(to<from&&step>=to&&step<from)return {...annotation,step:step+1};
    return annotation;
  });
}
function remapAnnotationsAfterDelete(annotations,deleted){
  return (annotations||[]).filter(annotation=>annotation.step!==deleted).map(annotation=>(
    annotation.step>deleted?{...annotation,step:annotation.step-1}:annotation
  ));
}
function annotationEventsInRange(clip,start,end,includeStart){
  return (clip?.annotations||[]).map((annotation,index)=>({annotation,index,time:annotationAbsoluteTime(clip,annotation)}))
    .filter(item=>includeStart?item.time>=start&&item.time<=end:item.time>start&&item.time<=end)
    .sort((a,b)=>a.time-b.time||a.index-b.index).map(item=>item.annotation);
}
function advanceV2Playback(clip,player,elapsedMs){
  if(!clip||!Array.isArray(clip.steps)||!clip.steps.length||!Number.isFinite(elapsedMs)||elapsedMs<=0||player.finished)return[];
  const total=clipDuration(clip),events=[];
  if(!total)return[];
  if(!player.started){player.started=true;events.push(...annotationEventsInRange(clip,0,0,true))}
  let remaining=elapsedMs;
  while(remaining>0){
    const amount=Math.min(remaining,total-player.time);
    events.push(...annotationEventsInRange(clip,player.time,player.time+amount,false));
    player.time+=amount;remaining-=amount;
    if(player.time>=total){
      if(clip.loop){player.time=0;events.push(...annotationEventsInRange(clip,0,0,true))}
      else{player.time=total;player.finished=true;break}
    }
  }
  return events;
}
function frameReferences(doc,index){
  const refs=[];
  for(const [clipName,clip] of Object.entries(doc?.clips||{})){
    for(let step=0;step<(clip?.steps||[]).length;step++){
      if(clip.steps[step]?.frame===index)refs.push(clipName+" step "+step);
    }
  }
  return refs;
}
function socketReferences(doc,index,name){
  const refs=[];
  for(const [clipName,clip] of Object.entries(doc?.clips||{})){
    for(let annotation=0;annotation<(clip?.annotations||[]).length;annotation++){
      const item=clip.annotations[annotation],step=clip.steps?.[item?.step];
      if(item?.socket===name&&step?.frame===index)refs.push(clipName+" annotation "+(item.name||annotation));
    }
  }
  return refs;
}
function mirroredSocketDisplayX(socketX,originX,mirrored){
  return mirrored?originX-(socketX-originX):socketX;
}
window.mobLabTestHelpers={frameReferences,socketReferences,mirroredSocketDisplayX,clipDuration,stepStartTime,annotationAbsoluteTime,timelinePlacement,remapAnnotationStepOnMove,remapAnnotationsAfterDelete,advanceV2Playback};
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
  const cols=Math.max(1,Math.trunc(Number(p.grid_size?.[0])||Math.floor(img.naturalWidth/(p.frame_size_px?.[0]||img.naturalWidth))||1));
  const rows=Math.max(1,Math.trunc(Number(p.grid_size?.[1])||Math.floor(img.naturalHeight/(p.frame_size_px?.[1]||img.naturalHeight))||1));
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
  const declared=state.manifestDoc?.frame_size_px;
  els.selectedFramePixels.textContent=Array.isArray(declared)?declared[0]+" × "+declared[1]:Math.round(g.fw)+" × "+Math.round(g.fh);
  const w=Number(state.presentation?.world_size?.[0])||1,h=Number(state.presentation?.world_size?.[1])||1;
  els.selectedFrameWorld.textContent=w.toFixed(2)+" × "+h.toFixed(2)+" wu";
}
function renderFrameSheet(){
  if(isV2(state.manifestDoc)){els.frameSheetGrid.replaceChildren();return}
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
  const first=state.presentation?.idle_frames?.[0]??0;
  drawAtlasFrame(canvas.getContext("2d"),first,canvas.width,canvas.height);
}
function updateQuickInfo(){
  if(!state.doc)return;
  const g=frameGeometry();
  const type=derivedType(state.doc);
  els.quickTypeInput.replaceChildren(new Option(type.replace(/\b\w/g,c=>c.toUpperCase()),type));
  els.quickTypeInput.value=type;
  els.quickSourceFile.textContent=(state.selectedPath||"-").split("/").pop();
  els.quickAtlas.textContent=state.manifestDoc?.atlas||"-";
  els.quickImageSize.textContent=state.previewImage?.naturalWidth?state.previewImage.naturalWidth+" × "+state.previewImage.naturalHeight:"-";
  els.quickFrames.textContent=isV2(state.manifestDoc)?(state.manifestDoc.frames?.length||0)+" explicit":(g?g.total+" ("+g.cols+" × "+g.rows+")":"-");
  const declared=state.manifestDoc?.frame_size_px;
  const selected=v2Frame(state.manifestDoc,state.selectedFrame);
  els.quickFrameSize.textContent=isV2(state.manifestDoc)&&selected?selected.rect_px[2]+" × "+selected.rect_px[3]:(Array.isArray(declared)?declared[0]+" × "+declared[1]:(g?Math.round(g.fw)+" × "+Math.round(g.fh):"-"));
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

function updateSaveState(){
  const dirty=Boolean(state.dirty||state.manifestDirty);
  els.saveButton.disabled=!dirty;
  els.footerDirtyStatus.textContent=dirty?"Unsaved changes":"Saved";
  els.footerDirtyStatus.classList.toggle("warn",dirty);
}
function updateDirty(){
  state.dirty=Boolean(state.selectedPath)&&canonical(state.doc)!==state.original;
  els.dirtyBadge.textContent=state.dirty?"DIRTY":"CLEAN";
  els.dirtyBadge.classList.toggle("dirty",state.dirty);
  updateSaveState();
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
  updateSaveState();
}
function syncManifestRaw(){
  els.manifestJsonEditor.value=state.manifestDoc?JSON.stringify(state.manifestDoc,null,2)+"\n":"";
}
function manifestToPresentation(){
  if(!state.manifestDoc||!state.presentation)return;
  const doc=state.manifestDoc,clip=doc.clips?.[state.previewClip]||doc.clips?.idle;
  if(isV2(doc))return;
  state.presentation.frame_size_px=doc.frame_size_px;
  state.presentation.grid_size=doc.grid_size||null;
  state.presentation.world_size=doc.world_size;
  state.presentation.frame_seconds=Number(doc.frame_seconds);
  state.presentation.idle_frames=Array.isArray(clip?.frames)?clip.frames:[0];
  state.presentation.authored_facing=doc.authored_facing;
}
function restartPreviewTimer(){
  stopPreviewTimer();
  if(!state.previewPlaying)return;
  let last=performance.now(),v1Carry=0;
  state.previewTimer=setInterval(()=>{
    const now=performance.now(),elapsed=now-last;last=now;
    if(isV2(state.manifestDoc)){
      const clip=v2Clip(state.manifestDoc,state.previewClip)||v2Clip(state.manifestDoc,"idle");
      const player={time:state.previewTime,started:state.previewStarted,finished:state.previewFinished};
      const events=advanceV2Playback(clip,player,elapsed);
      state.previewTime=player.time;state.previewStarted=player.started;state.previewFinished=player.finished;
      events.forEach(annotation=>{
        state.markerFeedback=`${annotation.name} @ ${annotationAbsoluteTime(clip,annotation)} ms`;
        els.annotationFeedback.textContent=state.markerFeedback;
      });
      drawPreview();renderV2Timeline();
    }else{
      const seconds=Number(state.presentation?.frame_seconds)||0.1;
      v1Carry+=elapsed;
      if(v1Carry>=seconds*1000){state.previewFrame+=Math.floor(v1Carry/(seconds*1000));v1Carry%=seconds*1000;drawPreview()}
    }
  },30);
}
function renderManifestClipEditors(){
  const clips=state.manifestDoc?.clips||{};
  els.manifestClipList.replaceChildren();
  for(const [name,clip] of Object.entries(clips)){
    const row=document.createElement("div");
    row.className="clip-editor-row";

    const framesLabel=document.createElement("label");
    framesLabel.className="clip-frames-field";
    const title=document.createElement("span");
    title.textContent=name+" Frames";
    const framesInput=document.createElement("input");
    framesInput.value=framesText(clip?.frames);
    framesInput.placeholder="0,1,2,3";
    framesInput.dataset.clipFrames=name;
    framesInput.addEventListener("input",applyManifestForm);
    framesLabel.append(title,framesInput);

    const loopLabel=document.createElement("label");
    loopLabel.className="clip-loop-field";
    const loopInput=document.createElement("input");
    loopInput.type="checkbox";
    loopInput.checked=Boolean(clip?.loop);
    loopInput.dataset.clipLoop=name;
    loopInput.addEventListener("change",applyManifestForm);
    const loopText=document.createElement("span");
    loopText.textContent="Loop";
    loopLabel.append(loopInput,loopText);

    row.append(framesLabel,loopLabel);
    els.manifestClipList.appendChild(row);
  }
  if(isV2(state.manifestDoc)){
    els.v1AnimationEditor.classList.add("hidden");
    els.v2AnimationEditor.classList.remove("hidden");
    renderV2AnimationEditor();
  }else{
    els.v1AnimationEditor.classList.remove("hidden");
    els.v2AnimationEditor.classList.add("hidden");
  }
}

function v2ValidationMessage(clip){
  if(!clip)return"Select a clip.";
  const errors=[];
  (clip.steps||[]).forEach((step,index)=>{
    if(!Number.isInteger(step.frame)||!v2Frame(state.manifestDoc,step.frame))errors.push(`Step ${index} must reference an existing frame.`);
    if(!Number.isInteger(step.duration_ms)||step.duration_ms<=0)errors.push(`Step ${index} duration must be a positive integer.`);
  });
  (clip.annotations||[]).forEach((annotation,index)=>{
    const step=clip.steps?.[annotation.step],frame=v2Frame(state.manifestDoc,step?.frame);
    if(!authoredId.test(String(annotation.name||"")))errors.push(`Annotation ${index} has an invalid name.`);
    if(!step)errors.push(`Annotation ${index} references an invalid step.`);
    else if(!Number.isInteger(annotation.offset_ms)||annotation.offset_ms<0||annotation.offset_ms>=step.duration_ms)errors.push(`Annotation ${index} offset must be within its step.`);
    if(annotation.socket!=null&&(!frame||!Object.hasOwn(frame.sockets||{},annotation.socket)))errors.push(`Annotation ${index} socket is missing from its referenced frame.`);
  });
  return errors.join(" ");
}
function selectedV2Clip(){return v2Clip(state.manifestDoc,state.previewClip)||v2Clip(state.manifestDoc,"idle")}
function resetV2Playback(){
  state.previewTime=0;state.previewStarted=false;state.previewFinished=false;state.markerFeedback="";
  if(els.annotationFeedback)els.annotationFeedback.textContent="";
}
function renderV2AnimationEditor(){
  const clips=state.manifestDoc?.clips||{},names=Object.keys(clips);
  els.v2ClipSelect.replaceChildren(...names.map(name=>new Option(name,name)));
  els.v2ClipSelect.value=state.previewClip;
  const clip=selectedV2Clip();
  els.v2ClipLoopInput.checked=Boolean(clip?.loop);
  renderV2StepList(clip);renderV2AnnotationList(clip);renderV2Timeline();
}
function renderV2StepList(clip){
  els.v2StepList.replaceChildren();
  (clip?.steps||[]).forEach((step,index)=>{
    const row=document.createElement("div");row.className="v2-step-row"+(index===currentV2Step()?" selected":"");
    const indexLabel=document.createElement("span");indexLabel.className="step-index";indexLabel.textContent=String(index);
    const frameLabel=document.createElement("label");frameLabel.textContent="Frame";
    const frameInput=document.createElement("select");
    (state.manifestDoc.frames||[]).forEach((_frame,frame)=>frameInput.appendChild(new Option(String(frame),String(frame))));
    frameInput.value=String(step.frame);frameInput.onchange=()=>updateV2Step(index,"frame",Number(frameInput.value));
    frameLabel.append(frameInput);
    const durationLabel=document.createElement("label");durationLabel.textContent="Duration ms";
    const durationInput=document.createElement("input");durationInput.type="number";durationInput.min="1";durationInput.step="1";durationInput.value=step.duration_ms;
    durationInput.onchange=()=>updateV2Step(index,"duration_ms",durationInput.value);
    durationLabel.append(durationInput);
    row.onclick=()=>{state.selectedStep=index;renderV2AnimationEditor();drawPreview()};
    row.append(indexLabel,frameLabel,durationLabel);els.v2StepList.appendChild(row);
  });
}
function currentV2Step(){return Number.isInteger(state.selectedStep)?state.selectedStep:0}
function renderV2AnnotationList(clip){
  els.v2AnnotationList.replaceChildren();
  (clip?.annotations||[]).forEach((annotation,index)=>{
    const row=document.createElement("div");row.className="v2-annotation-row";
    const name=document.createElement("input");name.value=annotation.name||"";name.title="Annotation name";
    const step=document.createElement("select");(clip.steps||[]).forEach((_s,i)=>step.appendChild(new Option(String(i),String(i))));step.value=String(annotation.step);
    const offset=document.createElement("input");offset.type="number";offset.min="0";offset.step="1";offset.value=annotation.offset_ms;
    const socket=document.createElement("select");socket.appendChild(new Option("No socket",""));
    const frame=v2Frame(state.manifestDoc,clip.steps?.[annotation.step]?.frame);
    Object.keys(frame?.sockets||{}).forEach(name=>socket.appendChild(new Option(name,name)));
    socket.value=annotation.socket||"";
    const remove=document.createElement("button");remove.type="button";remove.textContent="×";remove.title="Delete annotation";
    name.onchange=()=>updateV2Annotation(index,"name",name.value);
    step.onchange=()=>updateV2Annotation(index,"step",Number(step.value));
    offset.onchange=()=>updateV2Annotation(index,"offset_ms",offset.value);
    socket.onchange=()=>updateV2Annotation(index,"socket",socket.value||null);
    remove.onclick=()=>{clip.annotations.splice(index,1);commitV2Edit("Annotation deleted.")};
    row.append(name,step,offset,socket,remove);els.v2AnnotationList.appendChild(row);
  });
}
function renderV2Timeline(){
  const clip=selectedV2Clip();if(!els.v2Timeline)return;
  const total=clipDuration(clip),placements=timelinePlacement(clip);
  els.timelineTotal.textContent=`${total} ms`;
  els.v2Timeline.replaceChildren();
  placements.forEach(item=>{
    const segment=document.createElement("div");segment.className="timeline-segment";
    segment.style.width=`${item.width*100}%`;
    if(item.index===currentV2Step())segment.classList.add("selected");
    const current=total>0&&state.previewTime>=item.start&&(state.previewTime<item.start+item.duration||item.index===placements.length-1&&state.previewTime===total);
    if(current)segment.classList.add("current");
    segment.textContent=`F${item.frame} · ${item.duration}ms`;
    els.v2Timeline.appendChild(segment);
  });
  (clip?.annotations||[]).map((annotation,index)=>({annotation,index,time:annotationAbsoluteTime(clip,annotation)}))
    .sort((a,b)=>a.time-b.time||a.index-b.index).forEach(item=>{
      const marker=document.createElement("span");marker.className="timeline-marker";
      marker.style.left=`${total?item.time/total*100:0}%`;marker.title=`${item.annotation.name} @ ${item.time} ms`;
      marker.textContent=item.annotation.name;
      els.v2Timeline.appendChild(marker);
    });
}
function commitV2Edit(message){
  syncManifestRaw();updateManifestDirty();resetV2Playback();renderManifestForm();restartPreviewTimer();drawPreview();setStatus(message||"Animation updated.");
}
function updateV2Step(index,key,value){
  const clip=selectedV2Clip(),step=clip?.steps?.[index];if(!step)return;
  const parsed=Number(value);
  if(!Number.isInteger(parsed)||(key==="frame"&&!v2Frame(state.manifestDoc,parsed))||(key==="duration_ms"&&parsed<=0)){
    setStatus(key==="duration_ms"?"Duration must be a positive integer.":"Frame must reference an existing frame.");renderV2AnimationEditor();return;
  }
  step[key]=parsed;state.selectedStep=index;
  const error=v2ValidationMessage(clip);commitV2Edit(error||"Step updated.");
  if(error)setStatus(error);
}
function updateV2Annotation(index,key,value){
  const clip=selectedV2Clip(),annotation=clip?.annotations?.[index];if(!annotation)return;
  if(key==="name"&&!authoredId.test(String(value))){setStatus("Annotation names must use lowercase authored-id characters.");renderV2AnimationEditor();return}
  if(key==="offset_ms"&&(!Number.isInteger(Number(value))||Number(value)<0)){setStatus("Offset must be a non-negative integer.");renderV2AnimationEditor();return}
  annotation[key]=key==="offset_ms"?Number(value):value;
  if(key==="step"){renderV2AnimationEditor()}
  const error=v2ValidationMessage(clip);if(error){setStatus(error);renderV2AnnotationList(clip);renderV2Timeline();syncManifestRaw();updateManifestDirty();return}
  commitV2Edit("Annotation updated.");
}
function addV2Clip(){
  const clips=state.manifestDoc.clips,frames=state.manifestDoc.frames||[];
  let name="clip",suffix=1;while(Object.hasOwn(clips,name))name=`clip_${suffix++}`;
  clips[name]={loop:true,steps:[{frame:frames.length?state.selectedFrame:0,duration_ms:100}],annotations:[]};
  state.previewClip=name;state.selectedStep=0;commitV2Edit(`Added clip ${name}.`);
}
function renameV2Clip(){
  const oldName=state.previewClip;if(oldName==="idle"){setStatus("The required idle clip cannot be renamed.");return}
  const name=prompt("New clip name",oldName)?.trim();
  if(name===null||name===undefined)return;
  if(!authoredId.test(name)){setStatus("Clip names must use lowercase authored-id characters.");return}
  if(Object.hasOwn(state.manifestDoc.clips,name)){setStatus("A clip with that name already exists.");return}
  const entries=Object.entries(state.manifestDoc.clips),next={};
  entries.forEach(([key,value])=>{next[key===oldName?name:key]=value});
  state.manifestDoc.clips=next;state.previewClip=name;commitV2Edit(`Renamed clip to ${name}.`);
}
function deleteV2Clip(){
  const name=state.previewClip;
  if(name==="idle"){setStatus("The required idle clip cannot be deleted.");return}
  if(!confirm(`Delete optional clip "${name}"?`))return;
  delete state.manifestDoc.clips[name];state.previewClip="idle";state.selectedStep=0;commitV2Edit(`Deleted clip ${name}.`);
}
function addV2Step(){
  const clip=selectedV2Clip();if(!clip)return;
  const frame=state.manifestDoc.frames?.length?state.selectedFrame:0;
  clip.steps.push({frame,duration_ms:100});state.selectedStep=clip.steps.length-1;commitV2Edit("Step added.");
}
function deleteV2Step(){
  const clip=selectedV2Clip(),index=currentV2Step();if(!clip)return;
  if(clip.steps.length<=1){setStatus("Every clip must retain at least one step.");return}
  if((clip.annotations||[]).some(annotation=>annotation.step===index)){
    setStatus(`Cannot delete step ${index}; delete or move its annotations first.`);return;
  }
  clip.steps.splice(index,1);clip.annotations=remapAnnotationsAfterDelete(clip.annotations,index);
  state.selectedStep=Math.max(0,index-1);commitV2Edit("Step deleted.");
}
function moveV2Step(delta){
  const clip=selectedV2Clip(),from=currentV2Step(),to=from+delta;if(!clip||to<0||to>=clip.steps.length)return;
  [clip.steps[from],clip.steps[to]]=[clip.steps[to],clip.steps[from]];
  clip.annotations=remapAnnotationStepOnMove(clip.annotations,from,to);
  state.selectedStep=to;commitV2Edit("Step order updated.");
}
function addV2Annotation(){
  const clip=selectedV2Clip();if(!clip)return;
  const step=Math.min(currentV2Step(),Math.max(0,clip.steps.length-1));
  clip.annotations.push({name:"marker",step,offset_ms:0});
  commitV2Edit("Annotation added.");
}

function v2AtlasTransform(){
  const img=state.previewImage,canvas=els.atlasCanvas;
  if(!img?.naturalWidth||!canvas)return null;
  const scale=Math.min(canvas.width/img.naturalWidth,canvas.height/img.naturalHeight);
  return {scale,ox:(canvas.width-img.naturalWidth*scale)/2,oy:(canvas.height-img.naturalHeight*scale)/2};
}
function renderV2Atlas(){
  const canvas=els.atlasCanvas,img=state.previewImage;
  if(!canvas||!img?.naturalWidth)return;
  const ctx=canvas.getContext("2d"),t=v2AtlasTransform();
  ctx.clearRect(0,0,canvas.width,canvas.height);
  ctx.fillStyle="#091219";ctx.fillRect(0,0,canvas.width,canvas.height);
  ctx.imageSmoothingEnabled=false;
  ctx.drawImage(img,t.ox,t.oy,img.naturalWidth*t.scale,img.naturalHeight*t.scale);
  (state.manifestDoc.frames||[]).forEach((frame,index)=>{
    const [x,y,w,h]=frame.rect_px||[];
    const selected=index===state.selectedFrame;
    ctx.strokeStyle=selected?"#f4dc73":"#56c7da";
    ctx.lineWidth=selected?3:1.5;
    ctx.globalAlpha=selected?1:.72;
    ctx.strokeRect(t.ox+x*t.scale,t.oy+y*t.scale,w*t.scale,h*t.scale);
    ctx.globalAlpha=1;
    ctx.fillStyle=selected?"#fff0a1":"#d5edf2";
    ctx.font="bold 12px Consolas";
    ctx.fillText(String(index),t.ox+x*t.scale+4,t.oy+y*t.scale+14);
    if(selected){
      const [ox,oy]=frame.origin_px||[0,0];
      ctx.fillStyle="#f39b62";ctx.beginPath();
      ctx.arc(t.ox+(x+ox)*t.scale,t.oy+(y+oy)*t.scale,5,0,Math.PI*2);ctx.fill();
      Object.entries(frame.sockets||{}).forEach(([name,[sx,sy]])=>{
        const socketSelected=name===state.selectedSocket;
        ctx.fillStyle=socketSelected?"#fff":"#bd7cff";
        ctx.strokeStyle="#081117";ctx.lineWidth=2;
        ctx.beginPath();ctx.arc(t.ox+(x+sx)*t.scale,t.oy+(y+sy)*t.scale,socketSelected?5:4,0,Math.PI*2);ctx.fill();ctx.stroke();
        ctx.fillStyle="#f0e8ff";ctx.font="11px Consolas";
        ctx.fillText(name,t.ox+(x+sx)*t.scale+7,t.oy+(y+sy)*t.scale-5);
      });
    }
  });
}
function renderV2Editor(){
  const frames=state.manifestDoc?.frames||[];
  if(frames.length)state.selectedFrame=Math.max(0,Math.min(frames.length-1,state.selectedFrame));
  els.v2FrameList.replaceChildren();
  frames.forEach((frame,index)=>{
    const button=document.createElement("button");
    button.type="button";button.textContent=String(index);
    button.className=index===state.selectedFrame?"selected":"";
    button.title="Select frame "+index;
    button.onclick=()=>{state.selectedFrame=index;state.selectedSocket=null;renderV2Editor();renderV2Atlas();drawPreview()};
    els.v2FrameList.appendChild(button);
  });
  const frame=v2Frame(state.manifestDoc,state.selectedFrame);
  if(!frame)return;
  const [x,y,w,h]=frame.rect_px||[0,0,1,1], [ox,oy]=frame.origin_px||[0,0];
  els.v2FrameIndexInput.value=state.selectedFrame;
  els.v2RectXInput.value=x;els.v2RectYInput.value=y;
  els.v2RectWidthInput.value=w;els.v2RectHeightInput.value=h;
  els.v2OriginXInput.value=ox;els.v2OriginYInput.value=oy;
  els.selectedFrameIndex.textContent=String(state.selectedFrame);
  els.selectedFramePixels.textContent=w+" × "+h;
  const ppu=Number(state.manifestDoc.pixels_per_unit);
  els.selectedFrameWorld.textContent=Number.isFinite(ppu)&&ppu>0?(w/ppu).toFixed(2)+" × "+(h/ppu).toFixed(2)+" wu":"-";
  renderSocketList();renderV2Atlas();
}
function renderSocketList(){
  const frame=v2Frame(state.manifestDoc,state.selectedFrame);
  els.socketList.replaceChildren();
  for(const [name,coords] of Object.entries(frame?.sockets||{})){
    const row=document.createElement("div");
    row.className="socket-row"+(name===state.selectedSocket?" selected":"");
    row.dataset.socket=name;
    const nameInput=document.createElement("input");nameInput.value=name;nameInput.title="Socket name";
    const xInput=document.createElement("input");xInput.type="number";xInput.step="1";xInput.value=coords[0];
    const yInput=document.createElement("input");yInput.type="number";yInput.step="1";yInput.value=coords[1];
    const remove=document.createElement("button");remove.type="button";remove.textContent="×";remove.title="Delete socket";
    const nameLabel=document.createElement("label");nameLabel.textContent="Name";nameLabel.append(nameInput);
    const xLabel=document.createElement("label");xLabel.textContent="X";xLabel.append(xInput);
    const yLabel=document.createElement("label");yLabel.textContent="Y";yLabel.append(yInput);
    row.append(nameLabel,xLabel,yLabel,remove);els.socketList.appendChild(row);
    row.onclick=()=>{state.selectedSocket=name;state.socketPlacement=true;renderSocketList();renderV2Atlas()};
    nameInput.onchange=()=>renameSocket(name,nameInput.value.trim());
    const applyCoordinate=()=>updateSocket(name,xInput.value,yInput.value);
    xInput.onchange=applyCoordinate;yInput.onchange=applyCoordinate;
    remove.onclick=event=>{event.stopPropagation();deleteSocket(name)};
  }
}
function applyV2GlobalForm(){
  const m=state.manifestDoc,ppu=Number(els.manifestPixelsPerUnitInput.value);
  if(!Number.isFinite(ppu)||ppu<=0){
    setStatus("pixels_per_unit must be finite and greater than zero.");return;
  }
  if(!["left","right"].includes(els.manifestFacingInput.value)){
    setStatus("authored_facing must be left or right.");return;
  }
  m.pixels_per_unit=ppu;m.authored_facing=els.manifestFacingInput.value;
  syncManifestRaw();updateManifestDirty();renderV2Atlas();drawPreview();updateQuickInfo();
}
function updateSelectedFrame(){
  const frame=v2Frame(state.manifestDoc,state.selectedFrame),values=[
    Number(els.v2RectXInput.value),Number(els.v2RectYInput.value),
    Number(els.v2RectWidthInput.value),Number(els.v2RectHeightInput.value),
    Number(els.v2OriginXInput.value),Number(els.v2OriginYInput.value)
  ];
  const [x,y,w,h,ox,oy]=values;
  const rawValues=[els.v2RectXInput.value,els.v2RectYInput.value,els.v2RectWidthInput.value,els.v2RectHeightInput.value,els.v2OriginXInput.value,els.v2OriginYInput.value];
  if(rawValues.some(value=>String(value).trim()==="")||!values.every(Number.isInteger)||x<0||y<0||w<=0||h<=0){
    setStatus("Frame rectangle requires non-negative integer X/Y and positive integer width/height.");return;
  }
  const iw=state.previewImage?.naturalWidth,ih=state.previewImage?.naturalHeight;
  if(iw&&ih&&(x+w>iw||y+h>ih)){setStatus("Frame rectangle must fit inside the atlas.");return}
  frame.rect_px=[x,y,w,h];frame.origin_px=[ox,oy];
  const clip=selectedV2Clip(),error=v2ValidationMessage(clip);
  syncManifestRaw();updateManifestDirty();renderSocketList();renderV2Atlas();resetV2Playback();drawPreview();
  if(error)setStatus(error);
}
function addFrame(){
  const img=state.previewImage;
  if(!img?.naturalWidth||!img.naturalHeight){setStatus("Cannot add a frame until the atlas is loaded.");return}
  state.manifestDoc.frames.push({rect_px:[0,0,1,1],origin_px:[0,0],sockets:{}});
  state.selectedFrame=state.manifestDoc.frames.length-1;state.selectedSocket=null;
  syncManifestRaw();updateManifestDirty();renderV2Editor();drawPreview();
}
function deleteSelectedFrame(){
  const frames=state.manifestDoc.frames,index=state.selectedFrame;
  if(index!==frames.length-1){
    els.frameActionMessage.textContent="Only the final frame may be deleted; deleting this one would renumber later references.";
    return;
  }
  const refs=frameReferences(state.manifestDoc,index);
  if(refs.length){els.frameActionMessage.textContent="Cannot delete frame "+index+"; referenced by "+refs.join(", ")+".";
    return}
  if(frames.length<=1){els.frameActionMessage.textContent="A V2 manifest must retain at least one frame.";return}
  frames.pop();state.selectedFrame=Math.max(0,index-1);state.selectedSocket=null;
  els.frameActionMessage.textContent="";syncManifestRaw();updateManifestDirty();renderV2Editor();drawPreview();
}
function renameSocket(oldName,newName){
  const frame=v2Frame(state.manifestDoc,state.selectedFrame);
  if(!newName||!/^[a-z0-9][a-z0-9._-]*$/.test(newName)){setStatus("Socket names must use lowercase authored-id characters.");renderSocketList();return}
  if(newName!==oldName&&Object.hasOwn(frame.sockets,newName)){setStatus("Duplicate socket names are not allowed.");renderSocketList();return}
  const refs=socketReferences(state.manifestDoc,state.selectedFrame,oldName);
  if(refs.length){els.socketMessage.textContent="Cannot rename "+oldName+"; referenced by "+refs.join(", ")+".";
    renderSocketList();return}
  frame.sockets[newName]=frame.sockets[oldName];delete frame.sockets[oldName];state.selectedSocket=newName;
  els.socketMessage.textContent="";syncManifestRaw();updateManifestDirty();renderSocketList();renderV2Atlas();
}
function updateSocket(name,xValue,yValue){
  const x=Number(xValue),y=Number(yValue),frame=v2Frame(state.manifestDoc,state.selectedFrame);
  if(String(xValue).trim()===""||String(yValue).trim()===""||!Number.isInteger(x)||!Number.isInteger(y)){setStatus("Socket coordinates must be signed integers.");renderSocketList();return}
  frame.sockets[name]=[x,y];syncManifestRaw();updateManifestDirty();renderV2Atlas();drawPreview();
}
function addSocket(){
  const frame=v2Frame(state.manifestDoc,state.selectedFrame);let name="socket",index=1;
  while(Object.hasOwn(frame.sockets,name))name="socket_"+index++;
  frame.sockets[name]=[0,0];state.selectedSocket=name;state.socketPlacement=true;
  syncManifestRaw();updateManifestDirty();renderSocketList();renderV2Atlas();
}
function deleteSocket(name){
  const refs=socketReferences(state.manifestDoc,state.selectedFrame,name);
  if(refs.length){els.socketMessage.textContent="Cannot delete "+name+"; referenced by "+refs.join(", ")+".";
    return}
  const frame=v2Frame(state.manifestDoc,state.selectedFrame);delete frame.sockets[name];
  state.selectedSocket=null;els.socketMessage.textContent="";syncManifestRaw();updateManifestDirty();renderSocketList();renderV2Atlas();drawPreview();
}

function renderManifestForm(){
  const m=state.manifestDoc;
  if(!m)return;
  const v2=isV2(m);
  document.querySelectorAll(".v1-manifest-control").forEach(node=>node.classList.toggle("hidden",v2));
  document.querySelectorAll(".v2-manifest-control").forEach(node=>node.classList.toggle("hidden",!v2));
  els.manifestIdInput.value=m.id||"";
  els.manifestAtlasInput.value=m.atlas||"";
  els.manifestGridColsInput.value=m.grid_size?.[0]??"";
  els.manifestGridRowsInput.value=m.grid_size?.[1]??"";
  els.manifestFrameWidthInput.value=m.frame_size_px?.[0]??"";
  els.manifestFrameHeightInput.value=m.frame_size_px?.[1]??"";
  els.manifestWorldWidthInput.value=m.world_size?.[0]??"";
  els.manifestWorldHeightInput.value=m.world_size?.[1]??"";
  els.manifestFrameSecondsInput.value=m.frame_seconds??"";
  els.manifestPixelsPerUnitInput.value=m.pixels_per_unit??"";
  els.manifestFacingInput.value=m.authored_facing||"right";
  const clipNames=Object.keys(m.clips||{});
  if(!clipNames.includes(state.previewClip)){
    state.previewClip=clipNames.includes("idle")?"idle":(clipNames[0]||"idle");
  }
  els.manifestPreviewClipInput.replaceChildren(...clipNames.map(name=>new Option(name,name)));
  els.previewAnimationInput.replaceChildren(...clipNames.map(name=>new Option("Animation: "+name,name)));
  els.manifestPreviewClipInput.value=state.previewClip;
  els.previewAnimationInput.value=state.previewClip;
  renderManifestClipEditors();
  if(v2)renderV2Editor();
  syncManifestRaw();updateManifestDirty();manifestToPresentation();drawPreview();
}
function applyManifestForm(){
  const m=state.manifestDoc;
  if(!m)return;
  if(isV2(m)){
    applyV2GlobalForm();
    return;
  }
  const gridCols=int(els.manifestGridColsInput.value),gridRows=int(els.manifestGridRowsInput.value);
  if(gridCols>0&&gridRows>0)m.grid_size=[gridCols,gridRows];
  else if(Object.hasOwn(m,"grid_size"))m.grid_size=null;
  m.frame_size_px=[int(els.manifestFrameWidthInput.value),int(els.manifestFrameHeightInput.value)];
  m.world_size=[number(els.manifestWorldWidthInput.value),number(els.manifestWorldHeightInput.value)];
  m.frame_seconds=number(els.manifestFrameSecondsInput.value);
  m.authored_facing=els.manifestFacingInput.value;
  m.clips=m.clips||{};
  els.manifestClipList.querySelectorAll("[data-clip-frames]").forEach(input=>{
    const name=input.dataset.clipFrames;
    if(m.clips[name])m.clips[name].frames=parseFrames(input.value);
  });
  els.manifestClipList.querySelectorAll("[data-clip-loop]").forEach(input=>{
    const name=input.dataset.clipLoop;
    if(m.clips[name])m.clips[name].loop=input.checked;
  });
  state.previewClip=els.manifestPreviewClipInput.value;
  els.previewAnimationInput.value=state.previewClip;
  syncManifestRaw();updateManifestDirty();manifestToPresentation();state.previewFrame=0;renderFrameSheet();updateQuickInfo();restartPreviewTimer();drawPreview();
}

function stopPreviewTimer(){if(state.previewTimer){clearInterval(state.previewTimer);state.previewTimer=null}}
function previewContext(){return els.previewCanvas.getContext("2d")}

function drawV2Preview(ctx,centerX,entityY,pxPerWorld){
  const clip=selectedV2Clip(),total=clipDuration(clip);
  let activeStep=0;
  if(clip?.steps?.length&&total){
    const time=Math.min(state.previewTime,total);
    activeStep=clip.steps.findIndex((step,index)=>time<stepStartTime(clip,index)+step.duration_ms);
    if(activeStep<0)activeStep=clip.steps.length-1;
  }
  const activeFrame=clip?.steps?.[activeStep]?.frame;
  const frame=v2Frame(state.manifestDoc,activeFrame??state.selectedFrame),img=state.previewImage;
  if(!frame||!img?.naturalWidth)return false;
  const [sx,sy,fw,fh]=frame.rect_px,[ox,oy]=frame.origin_px;
  const ppu=Number(state.manifestDoc.pixels_per_unit);
  if(!Number.isFinite(ppu)||ppu<=0)return false;
  const scale=pxPerWorld/ppu,mirrored=state.manifestDoc.authored_facing==="left";
  if(state.showSprite){
    ctx.save();ctx.translate(centerX,entityY);
    if(mirrored)ctx.scale(-1,1);
    ctx.imageSmoothingEnabled=false;
    ctx.drawImage(img,sx,sy,fw,fh,-ox*scale,-oy*scale,fw*scale,fh*scale);
    ctx.restore();
  }
  ctx.fillStyle="#f39b62";
  ctx.beginPath();ctx.arc(centerX,entityY,4,0,Math.PI*2);ctx.fill();
  for(const [name,[socketX,socketY]] of Object.entries(frame.sockets||{})){
    const displayX=mirroredSocketDisplayX(socketX,ox,mirrored);
    const px=centerX+(displayX-ox)*scale,py=entityY+(socketY-oy)*scale;
    ctx.fillStyle=name===state.selectedSocket?"#fff":"#bd7cff";
    ctx.strokeStyle="#081117";ctx.lineWidth=2;
    ctx.beginPath();ctx.arc(px,py,name===state.selectedSocket?5:4,0,Math.PI*2);ctx.fill();ctx.stroke();
    ctx.fillStyle="#f0e8ff";ctx.font="11px Consolas";ctx.fillText(name,px+7,py-5);
  }
  els.spriteReadout.textContent=`Step ${activeStep} · Frame ${activeFrame??state.selectedFrame}`;
  els.hitboxReadout.textContent=`${state.previewTime.toFixed(0)}/${total} ms`;
  els.anchorReadout.textContent=(mirrored?"Mirrored ":"")+"Origin "+ox+","+oy;
  return true;
}

function drawPreview(){
  const ctx=previewContext(),w=els.previewCanvas.width,h=els.previewCanvas.height;
  ctx.clearRect(0,0,w,h);

  const centerX=w/2;
  const pxPerWorld=Math.min(190,Math.max(145,h*0.48));
  const floorY=h*0.82;
  const bounds=state.doc?.collision_bounds;
  const validBounds=bounds&&["left","right","bottom","top"].every(
    k=>Number.isFinite(Number(bounds[k]))&&Number(bounds[k])>=0
  )&&Number(bounds.left)+Number(bounds.right)>0&&Number(bounds.bottom)+Number(bounds.top)>0;
  const left=validBounds?Number(bounds.left):0.4;
  const right=validBounds?Number(bounds.right):0.4;
  const bottom=validBounds?Number(bounds.bottom):0.6;
  const top=validBounds?Number(bounds.top):0.6;
  const entityY=floorY-bottom*pxPerWorld;

  ctx.strokeStyle="#93a5b0";
  ctx.globalAlpha=.78;
  ctx.lineWidth=4;
  ctx.beginPath();
  ctx.moveTo(w*0.24,floorY);
  ctx.lineTo(w*0.76,floorY);
  ctx.stroke();
  ctx.globalAlpha=1;

  const p=state.presentation,img=state.previewImage;
  const v2Preview=isV2(state.manifestDoc);
  if(v2Preview){
    if(drawV2Preview(ctx,centerX,entityY,pxPerWorld))els.previewUnavailable.classList.add("hidden");
    else els.previewUnavailable.classList.remove("hidden");
  }else if(p&&img&&img.complete&&img.naturalWidth){
    const frames=p.idle_frames||[];
    const frame=frames[state.previewFrame%Math.max(frames.length,1)]||0;
    const grid=p.grid_size;
    const cols=Math.max(1,Math.trunc(Number(grid?.[0])||Math.floor(img.naturalWidth/p.frame_size_px[0])||1));
    const rows=Math.max(1,Math.trunc(Number(grid?.[1])||Math.floor(img.naturalHeight/p.frame_size_px[1])||1));
    const fw=img.naturalWidth/cols,fh=img.naturalHeight/rows;
    const sx=(frame%cols)*fw,sy=Math.floor(frame/cols)*fh;
    const worldW=Math.max(0.01,Number(p.world_size?.[0])||1);
    const worldH=Math.max(0.01,Number(p.world_size?.[1])||1);
    const dw=worldW*pxPerWorld,dh=worldH*pxPerWorld;
    if(state.showSprite){
      ctx.imageSmoothingEnabled=false;
      ctx.drawImage(img,sx,sy,fw,fh,centerX-dw/2,entityY-dh/2,dw,dh);
    }
    els.previewUnavailable.classList.add("hidden");
  }else{
    els.previewUnavailable.classList.remove("hidden");
  }

  if(validBounds&&state.showHitbox){
    const x=centerX-left*pxPerWorld,y=entityY-top*pxPerWorld;
    const bw=(left+right)*pxPerWorld,bh=(bottom+top)*pxPerWorld;
    ctx.fillStyle="rgba(224,95,104,.20)";
    ctx.strokeStyle="#df626b";
    ctx.lineWidth=3;
    ctx.fillRect(x,y,bw,bh);
    ctx.strokeRect(x,y,bw,bh);
  }

  if(validBounds&&state.showGuides){
    ctx.strokeStyle="#7a909c";
    ctx.lineWidth=1;
    ctx.setLineDash([5,4]);
    ctx.beginPath();
    ctx.moveTo(centerX,Math.max(18,entityY-top*pxPerWorld-22));
    ctx.lineTo(centerX,Math.min(floorY+18,h-18));
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle="#72cddd";
    ctx.beginPath();
    ctx.arc(centerX,entityY,4,0,Math.PI*2);
    ctx.fill();
  }

  if(v2Preview)return;
  const clipFrames=state.presentation?.idle_frames||[];
  const clipLength=Math.max(clipFrames.length,1);
  const clipFrame=(state.previewFrame%clipLength)+1;
  els.spriteReadout.textContent="Frame "+clipFrame+"/"+clipLength;
  els.hitboxReadout.textContent=(Number(state.presentation?.frame_seconds)||0.1).toFixed(2)+"s";
  els.anchorReadout.textContent="1.00x";
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
    const detail=sprite.id+" · "+sprite.frame_size_px[0]+"×"+sprite.frame_size_px[1]+" px"+grid;
    const option=document.createElement("option");
    option.value=sprite.id;
    option.textContent=sprite.atlas||sprite.id;
    option.title=detail;
    els.spriteInput.appendChild(option);
    if(els.newSpriteInput){
      const newOption=document.createElement("option");
      newOption.value=sprite.id;
      newOption.textContent=detail;
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
      manifestToPresentation();drawPreview();renderFrameSheet();renderV2Editor();renderV2Atlas();updateQuickInfo();restartPreviewTimer();
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
  els.docContentId.textContent=state.contentId??"—";
  els.docSchema.textContent=state.doc.schema_version??"—";
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
  els.documentTitle.textContent=state.doc.debug_name||state.doc.id;
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
  manifestToPresentation();state.previewFrame=0;resetV2Playback();restartPreviewTimer();renderV2AnimationEditor();drawPreview();
});
els.previewPlayButton.onclick=()=>{
  state.previewPlaying=!state.previewPlaying;
  els.previewPlayButton.textContent=state.previewPlaying?"Ⅱ":"▶";
  els.previewPlayButton.title=state.previewPlaying?"Pause preview":"Play preview";
  restartPreviewTimer();
};
els.previewStopButton.onclick=()=>{
  state.previewPlaying=false;els.previewPlayButton.textContent="▶";els.previewPlayButton.title="Play preview";
  stopPreviewTimer();resetV2Playback();state.previewFrame=0;drawPreview();renderV2Timeline();
};
els.v2ClipSelect.onchange=()=>{
  state.previewClip=els.v2ClipSelect.value;state.selectedStep=0;resetV2Playback();
  els.previewAnimationInput.value=state.previewClip;els.manifestPreviewClipInput.value=state.previewClip;
  renderManifestForm();restartPreviewTimer();drawPreview();
};
els.v2ClipLoopInput.onchange=()=>{
  const clip=selectedV2Clip();if(clip){clip.loop=els.v2ClipLoopInput.checked;commitV2Edit("Clip loop updated.")}
};
els.addClipButton.onclick=addV2Clip;
els.renameClipButton.onclick=renameV2Clip;
els.deleteClipButton.onclick=deleteV2Clip;
els.addStepButton.onclick=addV2Step;
els.deleteStepButton.onclick=deleteV2Step;
els.moveStepEarlierButton.onclick=()=>moveV2Step(-1);
els.moveStepLaterButton.onclick=()=>moveV2Step(1);
els.addAnnotationButton.onclick=addV2Annotation;
els.previewSpriteToggle.addEventListener("change",()=>{state.showSprite=els.previewSpriteToggle.checked;drawPreview()});
els.previewHitboxToggle.addEventListener("change",()=>{state.showHitbox=els.previewHitboxToggle.checked;drawPreview()});
els.previewGuidesToggle.addEventListener("change",()=>{state.showGuides=els.previewGuidesToggle.checked;drawPreview()});

[
  "manifestGridColsInput","manifestGridRowsInput","manifestFrameWidthInput","manifestFrameHeightInput",
  "manifestWorldWidthInput","manifestWorldHeightInput","manifestFrameSecondsInput","manifestPixelsPerUnitInput","manifestFacingInput",
  "manifestPreviewClipInput"
].forEach(id=>els[id].addEventListener("input",applyManifestForm));
["v2RectXInput","v2RectYInput","v2RectWidthInput","v2RectHeightInput","v2OriginXInput","v2OriginYInput"]
  .forEach(id=>els[id].addEventListener("change",updateSelectedFrame));
els.addFrameButton.onclick=addFrame;
els.deleteFrameButton.onclick=deleteSelectedFrame;
els.addSocketButton.onclick=addSocket;
els.atlasCanvas.addEventListener("click",event=>{
  if(!isV2(state.manifestDoc))return;
  const t=v2AtlasTransform(),rect=els.atlasCanvas.getBoundingClientRect();
  if(!t)return;
  const ax=(event.clientX-rect.left)*(els.atlasCanvas.width/rect.width);
  const ay=(event.clientY-rect.top)*(els.atlasCanvas.height/rect.height);
  const x=(ax-t.ox)/t.scale,y=(ay-t.oy)/t.scale;
  const candidates=(state.manifestDoc.frames||[]).map((frame,index)=>{
    const [fx,fy,fw,fh]=frame.rect_px||[];
    return x>=fx&&x<=fx+fw&&y>=fy&&y<=fy+fh?index:-1;
  }).filter(index=>index>=0);
  if(state.socketPlacement&&state.selectedSocket){
    const frame=v2Frame(state.manifestDoc,state.selectedFrame),[fx,fy]=frame.rect_px;
    frame.sockets[state.selectedSocket]=[Math.round(x-fx),Math.round(y-fy)];
    state.socketPlacement=false;syncManifestRaw();updateManifestDirty();renderSocketList();renderV2Atlas();drawPreview();return;
  }
  if(candidates.length){
    state.selectedFrame=candidates[candidates.length-1];state.selectedSocket=null;
    renderV2Editor();drawPreview();
  }
});
els.applyManifestRawButton.onclick=()=>{
  try{
    const parsed=JSON.parse(els.manifestJsonEditor.value);
    if(parsed.id!==state.doc?.sprite)throw new Error("Manifest id must stay "+state.doc?.sprite);
    state.manifestDoc=parsed;state.previewClip=Object.hasOwn(parsed.clips||{},state.previewClip)?state.previewClip:"idle";resetV2Playback();renderManifestForm();restartPreviewTimer();
    setStatus("Raw manifest applied in memory. Preview is live; SAVE MANIFEST persists it.");
  }catch(e){setStatus("Invalid manifest JSON: "+e.message)}
};
els.resetManifestButton.onclick=()=>{
  if(!state.manifestOriginal)return;
  state.manifestDoc=JSON.parse(state.manifestOriginal);renderManifestForm();restartPreviewTimer();
  setStatus("Manifest changes reset.");
};
els.saveManifestButton.onclick=async()=>{
  if(!state.manifestDirty)return;
  setStatus("Validating and saving sprite manifest...");
  try{
    const data=await saveSpriteManifest();
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
  if((state.dirty||state.manifestDirty)&&!confirm("Discard unsaved Monster or sprite-manifest changes?"))return;
  const data=await api("/api/monster?path="+encodeURIComponent(path));
  state.selectedPath=data.path;state.doc=data.document;state.contentId=data.content_id??null;
  state.original=canonical(state.doc);
  els.duplicateButton.disabled=false;
  els.emptyState.classList.add("hidden");els.editor.classList.remove("hidden");
  els.documentTitle.textContent=state.doc.debug_name||state.doc.id;
  els.documentPath.textContent="content/definitions/monsters/"+data.path;
  state.selectedFrame=0;state.selectedSocket=null;state.socketPlacement=false;
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
async function saveMonsterDocument(){
  if(!state.dirty)return null;
  const errors=localErrors(state.doc);
  if(errors.length){
    updateInspector();
    throw new Error("Monster JSON has validation errors. Fix them before save.");
  }
  const data=await api("/api/monster?path="+encodeURIComponent(state.selectedPath),{
    method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(state.doc)
  });
  if(data.document){
    state.doc=data.document;
    state.original=canonical(data.document);
    renderForm();
  }
  updateDirty();
  return data;
}
async function saveSpriteManifest(){
  if(!state.manifestDirty)return null;
  if(!state.manifestDoc?.id)throw new Error("Sprite manifest id is missing.");
  const data=await api("/api/sprite-manifest?sprite="+encodeURIComponent(state.manifestDoc.id),{
    method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(state.manifestDoc)
  });
  if(data.document)state.manifestDoc=data.document;
  state.manifestOriginal=canonical(state.manifestDoc);
  renderManifestForm();
  updateManifestDirty();
  return data;
}
async function saveAllDirty(){
  if(!state.dirty&&!state.manifestDirty)return;
  setStatus("Saving dirty Monster and sprite-manifest data...");
  const saved=[];
  try{
    if(state.manifestDirty){
      const manifest=await saveSpriteManifest();
      if(manifest)saved.push("manifest "+manifest.path);
    }
    if(state.dirty){
      const monster=await saveMonsterDocument();
      if(monster)saved.push("monster "+monster.path);
    }
    await loadSprites();
    await loadList();
    await loadPresentation();
    updateQuickInfo();
    setStatus("Saved: "+saved.join(" · "));
  }catch(e){
    updateSaveState();
    setStatus(String(e.message||e));
  }
}
els.saveButton.onclick=saveAllDirty;
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
