(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.PURGATORY_CHARACTER_LAB_RUNTIME = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, () => {
  'use strict';

  const finite = value => Number.isFinite(Number(value));
  const positive = value => finite(value) && Number(value) > 0;
  const integer = value => Number.isInteger(Number(value));

  function evaluateBoneWorld(locals, rotationOffsets = {}) {
    if (!locals || typeof locals !== 'object') {
      throw new Error('Humanoid contract bones are missing.');
    }

    const world = new Map();
    const visiting = new Set();
    const resolve = id => {
      if (world.has(id)) return world.get(id);
      if (visiting.has(id)) throw new Error(`Humanoid contract contains a cycle at ${id}.`);
      const local = locals[id];
      if (!local) throw new Error(`Humanoid contract is missing bone ${id}.`);
      if (!Array.isArray(local.t) || local.t.length !== 2 || !local.t.every(finite)) {
        throw new Error(`Humanoid bone ${id} has an invalid translation.`);
      }
      if (!finite(local.r || 0) || !finite(rotationOffsets[id] || 0)) {
        throw new Error(`Humanoid bone ${id} has an invalid rotation.`);
      }

      visiting.add(id);
      const parent = local.parent ? resolve(local.parent) : { x: 0, y: 0, angle: 0 };
      const tx = Number(local.t[0]);
      const ty = Number(local.t[1]);
      const c = Math.cos(parent.angle);
      const s = Math.sin(parent.angle);
      const value = {
        x: parent.x + tx * c - ty * s,
        y: parent.y + tx * s + ty * c,
        angle: parent.angle + Number(local.r || 0) + Number(rotationOffsets[id] || 0)
      };
      visiting.delete(id);
      world.set(id, value);
      return value;
    };

    Object.keys(locals).forEach(resolve);
    return world;
  }

  function visualLocalCorners(visual) {
    const rect = visual?.rect_px;
    const pivot = visual?.pivot_px;
    const ppu = Number(visual?.pixels_per_unit);
    if (!Array.isArray(rect) || rect.length !== 4 || !rect.every(finite)) {
      throw new Error(`Invalid rect for ${visual?.part || visual?.visual_key || 'visual'}.`);
    }
    if (!Array.isArray(pivot) || pivot.length !== 2 || !pivot.every(finite)) {
      throw new Error(`Invalid pivot for ${visual?.part || visual?.visual_key || 'visual'}.`);
    }
    if (!positive(ppu)) throw new Error('pixels_per_unit must be positive and finite.');

    const width = Number(rect[2]);
    const height = Number(rect[3]);
    const pivotX = Number(pivot[0]);
    const pivotY = Number(pivot[1]);
    return [
      [-pivotX / ppu, (pivotY - height) / ppu],
      [(width - pivotX) / ppu, (pivotY - height) / ppu],
      [(width - pivotX) / ppu, pivotY / ppu],
      [-pivotX / ppu, pivotY / ppu]
    ];
  }

  function transformPoint(world, local) {
    const c = Math.cos(world.angle);
    const s = Math.sin(world.angle);
    return [
      world.x + local[0] * c - local[1] * s,
      world.y + local[0] * s + local[1] * c
    ];
  }

  function visualWorldCorners(world, visual) {
    const bone = world.get(visual.bone);
    if (!bone) throw new Error(`Visual ${visual.visual_key || visual.part} references missing bone ${visual.bone}.`);
    return visualLocalCorners(visual).map(point => transformPoint(bone, point));
  }

  function buildPreviewFrame(locals, visuals, rotationOffsets = {}) {
    const world = evaluateBoneWorld(locals, rotationOffsets);
    const entries = [];
    for (const visual of visuals || []) {
      const boneWorld = world.get(visual.bone);
      if (!boneWorld) {
        throw new Error(`Visual ${visual.visual_key || visual.part} references missing bone ${visual.bone}.`);
      }
      entries.push({
        part: visual.part,
        visual,
        boneWorld: { ...boneWorld },
        pivot_px: [Number(visual.pivot_px[0]), Number(visual.pivot_px[1])],
        worldCorners: visualWorldCorners(world, visual)
      });
    }
    return { world, entries };
  }

  function worldToCanvasBone(bone, pixelsPerUnit) {
    const ppu = Number(pixelsPerUnit);
    if (!positive(ppu)) throw new Error('pixels_per_unit must be positive and finite.');
    return { x: bone.x * ppu, y: -bone.y * ppu, angle: -bone.angle };
  }

  function worldCornersToCanvas(corners, pixelsPerUnit) {
    const ppu = Number(pixelsPerUnit);
    if (!positive(ppu)) throw new Error('pixels_per_unit must be positive and finite.');
    return corners.map(([x, y]) => [x * ppu, -y * ppu]);
  }

  function boundsForCanvasCorners(corners) {
    if (!corners.length) return null;
    const xs = corners.map(point => point[0]);
    const ys = corners.map(point => point[1]);
    return {
      minX: Math.min(...xs),
      maxX: Math.max(...xs),
      minY: Math.min(...ys),
      maxY: Math.max(...ys)
    };
  }

  function resolveVisualPack(manifest, atlasWidth, atlasHeight, options = {}) {
    const issues = [];
    const requiredParts = [...(options.requiredParts || [])];
    const required = new Set(requiredParts);
    const drawOrder = [...(options.drawOrder || requiredParts)];
    const bones = new Set(options.boneLabels || []);

    if (!manifest || typeof manifest !== 'object') {
      return { issues: ['Runtime JSON not loaded'], visuals: [] };
    }
    if (manifest.schema_version !== 1) issues.push('Unsupported Runtime JSON schema_version');
    if (manifest.kind !== 'purgatory_visual_pack') issues.push('JSON kind is not purgatory_visual_pack');
    if (manifest.rig !== 'humanoid_v0') issues.push(`Unsupported rig ${manifest.rig || '(missing)'}`);
    if (manifest.view !== 'side') issues.push(`Unsupported view ${manifest.view || '(missing)'}`);
    if (manifest.authored_facing && manifest.authored_facing !== 'right') {
      issues.push(`Unsupported authored_facing ${manifest.authored_facing}`);
    }
    if (typeof manifest.id !== 'string' || !manifest.id.trim()) issues.push('Missing visual-pack id');
    if (!positive(manifest.pixels_per_unit)) issues.push('Invalid pixels_per_unit');
    if (!['partial_dev', 'complete'].includes(manifest.completeness)) {
      issues.push(`Unsupported completeness ${manifest.completeness || '(missing)'}`);
    }

    const atlas = manifest.atlas;
    if (!atlas || typeof atlas !== 'object') {
      issues.push('Missing atlas declaration');
    } else {
      if (typeof atlas.file !== 'string' || !atlas.file || /[\\/]/.test(atlas.file)) {
        issues.push('Atlas file must be a non-empty filename');
      }
      if (!integer(atlas.width) || Number(atlas.width) <= 0 ||
          !integer(atlas.height) || Number(atlas.height) <= 0) {
        issues.push('Atlas dimensions must be positive integers');
      }
      if (positive(atlasWidth) && positive(atlasHeight) &&
          (Number(atlas.width) !== Number(atlasWidth) || Number(atlas.height) !== Number(atlasHeight))) {
        issues.push(`Atlas size mismatch: JSON ${atlas.width}×${atlas.height}, PNG ${atlasWidth}×${atlasHeight}`);
      }
    }

    const rawVisuals = Array.isArray(manifest.visuals) ? manifest.visuals : [];
    if (!rawVisuals.length) issues.push('No visuals in manifest');
    const byKey = new Map();
    const partCounts = new Map();
    for (const visual of rawVisuals) {
      const label = visual?.part || visual?.visual_key || 'visual';
      if (typeof visual?.part !== 'string' || !visual.part) {
        issues.push(`Missing part for ${label}`);
      } else {
        partCounts.set(visual.part, (partCounts.get(visual.part) || 0) + 1);
        if (required.size && !required.has(visual.part)) issues.push(`Unsupported runtime part ${visual.part}`);
      }
      if (typeof visual?.visual_key !== 'string' || !visual.visual_key) {
        issues.push(`Missing visual_key for ${label}`);
      } else if (byKey.has(visual.visual_key)) {
        issues.push(`Duplicate visual key ${visual.visual_key}`);
      } else {
        byKey.set(visual.visual_key, visual);
      }
      if (typeof visual?.bone !== 'string' || !visual.bone || (bones.size && !bones.has(visual.bone))) {
        issues.push(`Invalid bone ${visual?.bone || '(missing)'} for ${label}`);
      }
      if (visual?.part && visual?.bone && visual.part !== visual.bone) {
        issues.push(`Part ${visual.part} must map to bone ${visual.part}, got ${visual.bone}`);
      }

      const rect = visual?.rect_px;
      if (!Array.isArray(rect) || rect.length !== 4 || !rect.every(integer)) {
        issues.push(`Invalid rect for ${label}`);
      } else {
        const [x, y, width, height] = rect.map(Number);
        if (x < 0 || y < 0 || width <= 0 || height <= 0 ||
            (positive(atlasWidth) && x + width > Number(atlasWidth)) ||
            (positive(atlasHeight) && y + height > Number(atlasHeight))) {
          issues.push(`Atlas rect outside PNG for ${label}`);
        }
      }
      const pivot = visual?.pivot_px;
      if (!Array.isArray(pivot) || pivot.length !== 2 || !pivot.every(finite)) {
        issues.push(`Invalid pivot for ${label}`);
      }
    }
    for (const [part, count] of partCounts) {
      if (count > 1) issues.push(`Duplicate runtime part ${part}`);
    }

    const appearance = manifest.base_appearance;
    if (!appearance || typeof appearance !== 'object' || Array.isArray(appearance)) {
      issues.push('Missing base_appearance map');
    }
    const resolved = [];
    for (const part of drawOrder) {
      const key = appearance?.[part];
      if (!key) {
        if (manifest.completeness === 'complete') {
          issues.push(`base_appearance is missing ${part}`);
        }
        continue;
      }
      const visual = byKey.get(key);
      if (!visual) {
        issues.push(`base_appearance ${part} references missing visual ${key}`);
        continue;
      }
      if (visual.part !== part) {
        issues.push(`base_appearance ${part} resolves visual for ${visual.part}`);
        continue;
      }
      resolved.push({ ...visual, pixels_per_unit: Number(manifest.pixels_per_unit) });
    }
    if (manifest.completeness === 'complete' && requiredParts.length && resolved.length !== requiredParts.length) {
      issues.push(`Complete pack must resolve ${requiredParts.length} base parts; got ${resolved.length}`);
    }
    return { issues: [...new Set(issues)], visuals: resolved };
  }

  function validateTemplatePixels(rgba, width, height, template, options = {}) {
    const issues = [];
    if (!template || typeof template !== 'object') return { issues: ['Template V1 contract unavailable'] };
    if (Number(width) !== Number(template.width) || Number(height) !== Number(template.height)) {
      return { issues: [`Template V1 PNG must be ${template.width}×${template.height}; got ${width}×${height}`] };
    }
    const cells = Array.isArray(template.cells) ? template.cells : [];
    const expectedParts = options.expectedParts || [];
    if (cells.length !== 14) issues.push(`Template V1 contract must contain 14 cells; got ${cells.length}`);
    const parts = new Set(cells.map(cell => cell.part));
    for (const part of expectedParts) if (!parts.has(part)) issues.push(`Template V1 contract is missing ${part}`);

    const stride = options.stride || 4;
    const alphaOffset = options.alphaOffset ?? 3;
    const edgeMargin = options.edgeMargin ?? 8;
    const alphaAt = (x, y) => Number(rgba[(y * width + x) * stride + alphaOffset] || 0);
    const usedGridCells = new Set();
    const cellSize = Number(template.cell_size);

    for (const cell of cells) {
      const x0 = Number(cell.x), y0 = Number(cell.y), w = Number(cell.w), h = Number(cell.h);
      if (![x0, y0, w, h].every(integer) || x0 < 0 || y0 < 0 || w <= 0 || h <= 0 ||
          x0 + w > width || y0 + h > height) {
        issues.push(`${cell.part}: invalid cell bounds`);
        continue;
      }
      if (positive(cellSize)) usedGridCells.add(`${x0 / cellSize},${y0 / cellSize}`);
      let count = 0;
      let edge = false;
      for (let y = y0; y < y0 + h; y++) {
        for (let x = x0; x < x0 + w; x++) {
          if (!alphaAt(x, y)) continue;
          count++;
          if (x - x0 < edgeMargin || y - y0 < edgeMargin ||
              x0 + w - 1 - x < edgeMargin || y0 + h - 1 - y < edgeMargin) edge = true;
        }
      }
      if (!count) issues.push(`${cell.part}: empty cell`);
      else if (edge) issues.push(`${cell.part}: artwork enters the ${edgeMargin}px isolation margin`);
    }

    if (positive(cellSize) && width % cellSize === 0 && height % cellSize === 0) {
      const cols = width / cellSize;
      const rows = height / cellSize;
      for (let row = 0; row < rows; row++) {
        for (let col = 0; col < cols; col++) {
          if (usedGridCells.has(`${col},${row}`)) continue;
          let occupied = false;
          outer: for (let y = row * cellSize; y < (row + 1) * cellSize; y++) {
            for (let x = col * cellSize; x < (col + 1) * cellSize; x++) {
              if (alphaAt(x, y)) { occupied = true; break outer; }
            }
          }
          if (occupied) issues.push(`reserved cell ${row * cols + col + 1}: must be transparent`);
        }
      }
    }
    return { issues: [...new Set(issues)] };
  }

  function computeTemplatePlacement({ sourcePpu, targetPpu, width, height, pivot, cell, margin = 8 }) {
    if (![sourcePpu, targetPpu, width, height, pivot?.[0], pivot?.[1]].every(finite) ||
        !positive(sourcePpu) || !positive(targetPpu) || !positive(width) || !positive(height)) {
      throw new Error('Invalid source/template geometry.');
    }
    const scale = Number(targetPpu) / Number(sourcePpu);
    const targetPivotX = Number(cell.x) + Number(cell.pivot[0]);
    const targetPivotY = Number(cell.y) + Number(cell.pivot[1]);
    const dw = Number(width) * scale;
    const dh = Number(height) * scale;
    const dx = targetPivotX - Number(pivot[0]) * scale;
    const dy = targetPivotY - Number(pivot[1]) * scale;
    const limits = {
      minX: Number(cell.x) + margin,
      minY: Number(cell.y) + margin,
      maxX: Number(cell.x) + Number(cell.w) - margin,
      maxY: Number(cell.y) + Number(cell.h) - margin
    };
    const overflow = {
      left: Math.max(0, limits.minX - dx),
      top: Math.max(0, limits.minY - dy),
      right: Math.max(0, dx + dw - limits.maxX),
      bottom: Math.max(0, dy + dh - limits.maxY)
    };
    return {
      scale, dx, dy, dw, dh, targetPivot: [targetPivotX, targetPivotY], overflow,
      fits: Object.values(overflow).every(value => value <= 1e-7)
    };
  }

  const nextPow2 = value => {
    let result = 1;
    while (result < value) result *= 2;
    return result;
  };

  function planAtlas(rectangles, options = {}) {
    const padding = options.padding ?? 4;
    const minWidth = options.minWidth ?? 512;
    const targetHeight = options.targetHeight ?? 2048;
    const maxDimension = options.maxDimension ?? 4096;
    const sprites = [...rectangles].map(item => ({
      id: item.id,
      width: Number(item.width),
      height: Number(item.height)
    }));
    if (!sprites.length) throw new Error('No sprites are available for atlas export.');
    for (const sprite of sprites) {
      if (!sprite.id || !integer(sprite.width) || !integer(sprite.height) || sprite.width <= 0 || sprite.height <= 0) {
        throw new Error(`Invalid atlas sprite ${sprite.id || '(missing id)'}.`);
      }
      if (sprite.width + padding * 2 > maxDimension || sprite.height + padding * 2 > maxDimension) {
        throw new Error(`${sprite.id}: ${sprite.width}×${sprite.height}px exceeds the ${maxDimension}px atlas limit.`);
      }
    }
    sprites.sort((a, b) => b.height - a.height || b.width - a.width || a.id.localeCompare(b.id));

    const packAt = width => {
      let x = padding, y = padding, rowHeight = 0, maxY = padding;
      const placements = [];
      for (const sprite of sprites) {
        if (x + sprite.width + padding > width && x > padding) {
          x = padding;
          y += rowHeight + padding;
          rowHeight = 0;
        }
        placements.push({ ...sprite, x, y });
        x += sprite.width + padding;
        rowHeight = Math.max(rowHeight, sprite.height);
        maxY = Math.max(maxY, y + sprite.height + padding);
      }
      return { placements, height: nextPow2(Math.max(64, maxY)) };
    };

    const widest = Math.max(...sprites.map(sprite => sprite.width + padding * 2));
    let width = nextPow2(Math.max(minWidth, widest));
    if (width > maxDimension) throw new Error(`Atlas width ${width}px exceeds the ${maxDimension}px limit.`);
    let packed = packAt(width);
    while (packed.height > targetHeight && width < maxDimension) {
      width = Math.min(maxDimension, width * 2);
      packed = packAt(width);
    }
    if (packed.height > maxDimension) {
      throw new Error(`Atlas height ${packed.height}px exceeds the ${maxDimension}px limit.`);
    }
    return { width, height: packed.height, padding, placements: packed.placements };
  }

  function buildVisualPackManifest({ id, pixelsPerUnit, atlas, visuals, complete, drawOrder }) {
    if (typeof id !== 'string' || !id) throw new Error('Visual-pack id is required.');
    if (!positive(pixelsPerUnit)) throw new Error('pixels_per_unit must be positive and finite.');
    if (!atlas || !integer(atlas.width) || !integer(atlas.height) ||
        Number(atlas.width) <= 0 || Number(atlas.height) <= 0) {
      throw new Error('Atlas dimensions must be positive integers.');
    }
    const byPart = new Map();
    for (const visual of visuals || []) {
      if (byPart.has(visual.part)) throw new Error(`Duplicate runtime part ${visual.part}.`);
      byPart.set(visual.part, visual);
    }
    const ordered = [];
    const baseAppearance = {};
    for (const part of drawOrder || []) {
      const visual = byPart.get(part);
      if (!visual) continue;
      const key = visual.visual_key || `${id}.${part}.side`;
      ordered.push({
        part,
        bone: visual.bone || part,
        visual_key: key,
        rect_px: visual.rect_px.map(Number),
        pivot_px: visual.pivot_px.map(Number)
      });
      baseAppearance[part] = key;
    }
    return {
      schema_version: 1,
      kind: 'purgatory_visual_pack',
      id,
      rig: 'humanoid_v0',
      view: 'side',
      authored_facing: 'right',
      pixels_per_unit: Number(pixelsPerUnit),
      completeness: complete ? 'complete' : 'partial_dev',
      atlas: {
        file: atlas.file || `${id}.side.atlas.png`,
        width: Number(atlas.width),
        height: Number(atlas.height),
        padding_px: Number(atlas.padding_px ?? 4)
      },
      visuals: ordered,
      base_appearance: baseAppearance,
      draw_order: ordered.map(visual => visual.part)
    };
  }

  return {
    evaluateBoneWorld,
    visualLocalCorners,
    visualWorldCorners,
    buildPreviewFrame,
    worldToCanvasBone,
    worldCornersToCanvas,
    boundsForCanvasCorners,
    resolveVisualPack,
    validateTemplatePixels,
    computeTemplatePlacement,
    planAtlas,
    buildVisualPackManifest
  };
});
