'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const runtime = require('./character_lab_runtime.js');

global.window = {};
require('./humanoid_v0_contract.js');
const contract = global.window.PURGATORY_HUMANOID_V0_CONTRACT;

const REQUIRED_PARTS = [
  'head', 'torso',
  'upper_arm_back', 'lower_arm_back', 'hand_back',
  'upper_arm_front', 'lower_arm_front', 'hand_front',
  'upper_leg_back', 'lower_leg_back', 'foot_back',
  'upper_leg_front', 'lower_leg_front', 'foot_front'
];
const DRAW_ORDER = [
  'upper_arm_back', 'lower_arm_back', 'hand_back',
  'upper_leg_back', 'lower_leg_back', 'foot_back',
  'torso',
  'upper_leg_front', 'lower_leg_front', 'foot_front',
  'head',
  'upper_arm_front', 'lower_arm_front', 'hand_front'
];

function close(actual, expected, message) {
  assert.ok(Math.abs(actual - expected) < 1e-9, `${message}: ${actual} != ${expected}`);
}

function closePoint(actual, expected, message) {
  close(actual[0], expected[0], `${message} x`);
  close(actual[1], expected[1], `${message} y`);
}

function testGeneratedHumanoidContract() {
  assert.equal(contract.rig, 'humanoid_v0');
  assert.equal(contract.bones.lower_arm_back.r, 0.5);
  assert.equal(contract.template_v1.width, 2048);
  assert.equal(contract.template_v1.height, 2048);
  assert.equal(contract.template_v1.cells.length, 14);
  assert.deepEqual(new Set(contract.template_v1.cells.map(cell => cell.part)), new Set(REQUIRED_PARTS));

  const world = runtime.evaluateBoneWorld(contract.bones);
  close(world.get('torso').y, 0.72, 'torso world y');
  close(world.get('head').x, 0.07, 'head world x');
  close(world.get('head').y, 0.96, 'head world y');
  close(world.get('lower_arm_back').angle, 0.5, 'lower_arm_back world rotation');
  close(world.get('hand_back').x, 0.08 + Math.sin(0.5) * 0.1, 'rotated hand_back x');
  close(world.get('hand_back').y, 0.64 - Math.cos(0.5) * 0.1, 'rotated hand_back y');
}

function manifestFixture() {
  const visuals = REQUIRED_PARTS.map((part, index) => ({
    part,
    bone: part,
    rect_px: [(index % 7) * 64, Math.floor(index / 7) * 96, 32 + index, 48 + index],
    pivot_px: [8.25 + index * 0.1, 11.75 + index * 0.2]
  }));
  return runtime.buildVisualPackManifest({
    id: 'character.base.test_01',
    pixelsPerUnit: 320,
    atlas: { file: 'character.base.test_01.side.atlas.png', width: 512, height: 256, padding_px: 4 },
    visuals,
    complete: true,
    drawOrder: DRAW_ORDER
  });
}

function testAssemblyRuntimeParity() {
  const manifest = manifestFixture();
  const resolved = runtime.resolveVisualPack(manifest, 512, 256, {
    requiredParts: REQUIRED_PARTS,
    drawOrder: DRAW_ORDER,
    boneLabels: Object.keys(contract.bones)
  });
  assert.deepEqual(resolved.issues, []);

  // Assembly source rectangles may live elsewhere on the source sheet, but
  // their cropped dimensions and sprite-local pivots are the exported values.
  const assemblyVisuals = resolved.visuals.map((visual, index) => ({
    ...visual,
    visual_key: `source.${visual.part}`,
    rect_px: [1000 + index * 3, 700 + index * 2, visual.rect_px[2], visual.rect_px[3]]
  }));
  const assembly = runtime.buildPreviewFrame(contract.bones, assemblyVisuals);
  const preview = runtime.buildPreviewFrame(contract.bones, resolved.visuals);
  const representative = [
    'torso', 'head', 'upper_arm_front', 'lower_arm_front',
    'lower_arm_back', 'hand_front', 'upper_leg_back', 'foot_front'
  ];
  for (const part of representative) {
    const left = assembly.entries.find(entry => entry.part === part);
    const right = preview.entries.find(entry => entry.part === part);
    assert.ok(left && right, `missing parity fixture ${part}`);
    close(left.boneWorld.x, right.boneWorld.x, `${part} bone translation x`);
    close(left.boneWorld.y, right.boneWorld.y, `${part} bone translation y`);
    close(left.boneWorld.angle, right.boneWorld.angle, `${part} bone rotation`);
    assert.deepEqual(left.pivot_px, right.pivot_px, `${part} pivot interpretation`);
    left.worldCorners.forEach((corner, index) => {
      closePoint(corner, right.worldCorners[index], `${part} world corner ${index}`);
    });

    const ppu = right.visual.pixels_per_unit;
    const boneCanvas = runtime.worldToCanvasBone(right.boneWorld, ppu);
    const localTopLeft = [-right.pivot_px[0], -right.pivot_px[1]];
    const cos = Math.cos(boneCanvas.angle);
    const sin = Math.sin(boneCanvas.angle);
    const drawnTopLeft = [
      boneCanvas.x + localTopLeft[0] * cos - localTopLeft[1] * sin,
      boneCanvas.y + localTopLeft[0] * sin + localTopLeft[1] * cos
    ];
    closePoint(
      drawnTopLeft,
      runtime.worldCornersToCanvas([right.worldCorners[3]], ppu)[0],
      `${part} canvas Y-down conversion`
    );
  }
  close(preview.entries.find(entry => entry.part === 'lower_arm_back').boneWorld.angle, 0.5,
    'parity fixture includes lower_arm_back bind rotation');
  close(runtime.worldToCanvasBone(
    preview.entries.find(entry => entry.part === 'lower_arm_back').boneWorld, 320
  ).angle, -0.5, 'canvas rotation mirrors runtime +Y-up rotation');
}

function testCurrentClientFixtureResolves() {
  const manifestPath = path.resolve(__dirname, '../../Graphic/character/base/character.base.dev_01.visual-pack.json');
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const resolved = runtime.resolveVisualPack(manifest, manifest.atlas.width, manifest.atlas.height, {
    requiredParts: REQUIRED_PARTS,
    drawOrder: DRAW_ORDER,
    boneLabels: Object.keys(contract.bones)
  });
  assert.deepEqual(resolved.issues, []);
  assert.equal(resolved.visuals.length, 14);

  const broken = JSON.parse(JSON.stringify(manifest));
  delete broken.visuals[0].pivot_px;
  broken.visuals[1].visual_key = broken.visuals[0].visual_key;
  const issues = runtime.resolveVisualPack(broken, broken.atlas.width, broken.atlas.height, {
    requiredParts: REQUIRED_PARTS,
    drawOrder: DRAW_ORDER,
    boneLabels: Object.keys(contract.bones)
  }).issues;
  assert.ok(issues.some(issue => issue.includes('Invalid pivot')));
  assert.ok(issues.some(issue => issue.includes('Duplicate visual key')));
}

function setAlpha(rgba, width, x, y, value) {
  rgba[(y * width + x) * 4 + 3] = value;
}

function validTemplatePixels() {
  const { width, height, cells } = contract.template_v1;
  const rgba = new Uint8Array(width * height * 4);
  for (const cell of cells) {
    setAlpha(rgba, width, cell.x + cell.pivot[0], cell.y + cell.pivot[1], 255);
  }
  return rgba;
}

function testTemplateValidation() {
  const template = contract.template_v1;
  const valid = validTemplatePixels();
  assert.deepEqual(runtime.validateTemplatePixels(valid, 2048, 2048, template, {
    expectedParts: REQUIRED_PARTS,
    edgeMargin: 8
  }).issues, []);

  const boundary = validTemplatePixels();
  setAlpha(boundary, 2048, 0, 100, 255);
  assert.ok(runtime.validateTemplatePixels(boundary, 2048, 2048, template, {
    expectedParts: REQUIRED_PARTS,
    edgeMargin: 8
  }).issues.some(issue => issue.includes('head') && issue.includes('isolation')));

  const reserved = validTemplatePixels();
  setAlpha(reserved, 2048, 1024 + 100, 1536 + 100, 255);
  assert.ok(runtime.validateTemplatePixels(reserved, 2048, 2048, template, {
    expectedParts: REQUIRED_PARTS,
    edgeMargin: 8
  }).issues.some(issue => issue.includes('reserved cell 15')));

  assert.ok(runtime.validateTemplatePixels(valid, 1024, 2048, template).issues[0].includes('2048×2048'));
}

function testTemplateConversionGeometry() {
  const cell = contract.template_v1.cells.find(item => item.part === 'head');
  const placement = runtime.computeTemplatePlacement({
    sourcePpu: 256,
    targetPpu: 512,
    width: 100,
    height: 80,
    pivot: [20, 30],
    cell,
    margin: 8
  });
  assert.equal(placement.scale, 2);
  assert.equal(placement.dw / 512, 100 / 256, 'world width must be preserved');
  assert.equal(placement.dh / 512, 80 / 256, 'world height must be preserved');
  close(placement.dx + 20 * placement.scale, placement.targetPivot[0], 'converted pivot x');
  close(placement.dy + 30 * placement.scale, placement.targetPivot[1], 'converted pivot y');
  assert.equal(placement.fits, true);

  const overflow = runtime.computeTemplatePlacement({
    sourcePpu: 64,
    targetPpu: 512,
    width: 200,
    height: 200,
    pivot: [100, 100],
    cell,
    margin: 8
  });
  assert.equal(overflow.fits, false);
  assert.ok(Object.values(overflow.overflow).some(value => value > 0));
}

function testDeterministicAtlasAndManifest() {
  const rects = REQUIRED_PARTS.map((id, index) => ({ id, width: 20 + index, height: 30 + (index % 4) }));
  const firstPlan = runtime.planAtlas(rects);
  const secondPlan = runtime.planAtlas([...rects].reverse());
  assert.deepEqual(firstPlan, secondPlan, 'atlas layout must not depend on input order');

  const manifest = manifestFixture();
  const reversed = runtime.buildVisualPackManifest({
    id: manifest.id,
    pixelsPerUnit: manifest.pixels_per_unit,
    atlas: manifest.atlas,
    visuals: [...manifest.visuals].reverse(),
    complete: true,
    drawOrder: DRAW_ORDER
  });
  assert.equal(JSON.stringify(reversed), JSON.stringify(manifest), 'manifest serialization order must be deterministic');
}

function testHtmlUsesTheSharedRuntimePath() {
  const html = fs.readFileSync(require.resolve('./purgatory_character_part_tool_step1.html'), 'utf8');
  const assembly = html.slice(html.indexOf('function drawAssembly'), html.indexOf('function canvasToPngDataUrl'));
  const assemblyFrame = html.slice(html.indexOf('function assemblyPreviewFrame'), html.indexOf('function autoLayoutAssembly'));
  const runtimeFrame = html.slice(html.indexOf('function runtimePreviewFrame'), html.indexOf('function runtimePreviewBounds'));
  const roundTrip = html.slice(html.indexOf('async function previewCurrentExport'), html.indexOf("previewCurrentBtn.addEventListener"));
  assert.ok(assembly.includes('assemblyPreviewFrame()'));
  assert.ok(!assembly.includes('assemblyPos'), 'normal Assembly must not consume legacy assemblyPos');
  assert.ok(assemblyFrame.includes('LAB_RUNTIME.buildPreviewFrame(ENGINE_BIND_V0'));
  assert.ok(runtimeFrame.includes('LAB_RUNTIME.buildPreviewFrame(runtimeBindLocals()'));
  assert.ok(roundTrip.includes('buildAtlas()'));
  assert.ok(roundTrip.includes('buildRuntimeManifest(built)'));
  assert.ok(roundTrip.includes('resolveVisualPack'));
}

testGeneratedHumanoidContract();
testAssemblyRuntimeParity();
testCurrentClientFixtureResolves();
testTemplateValidation();
testTemplateConversionGeometry();
testDeterministicAtlasAndManifest();
testHtmlUsesTheSharedRuntimePath();
console.log('Character Lab runtime parity tests: PASS');
