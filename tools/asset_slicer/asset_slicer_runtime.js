(function (root, factory) {
  const api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) root.PURGATORY_ASSET_SLICER = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, () => {
  'use strict';

  function asPositiveInt(value, fallback) {
    const n = Number(value);
    return Number.isFinite(n) && n >= 0 ? Math.floor(n) : fallback;
  }

  function detectComponents(rgba, width, height, alphaCut = 0, minPixels = 1) {
    if (!rgba || rgba.length !== width * height * 4) {
      throw new Error('RGBA buffer size does not match width × height');
    }

    const w = asPositiveInt(width, 0);
    const h = asPositiveInt(height, 0);
    if (w <= 0 || h <= 0) return [];

    const cut = Math.max(0, Math.min(254, asPositiveInt(alphaCut, 0)));
    const min = Math.max(1, asPositiveInt(minPixels, 1));
    const visited = new Uint8Array(w * h);
    const stack = [];
    const components = [];

    function isSolid(pixelIndex) {
      return rgba[pixelIndex * 4 + 3] > cut;
    }

    for (let y = 0; y < h; y += 1) {
      for (let x = 0; x < w; x += 1) {
        const start = y * w + x;
        if (visited[start] || !isSolid(start)) continue;

        visited[start] = 1;
        stack.length = 0;
        stack.push(start);

        let minX = x;
        let maxX = x;
        let minY = y;
        let maxY = y;
        const pixels = [];

        while (stack.length) {
          const index = stack.pop();
          const cx = index % w;
          const cy = (index / w) | 0;
          pixels.push(index);

          if (cx < minX) minX = cx;
          if (cx > maxX) maxX = cx;
          if (cy < minY) minY = cy;
          if (cy > maxY) maxY = cy;

          const left = index - 1;
          const right = index + 1;
          const up = index - w;
          const down = index + w;

          if (cx > 0 && !visited[left] && isSolid(left)) {
            visited[left] = 1;
            stack.push(left);
          }
          if (cx < w - 1 && !visited[right] && isSolid(right)) {
            visited[right] = 1;
            stack.push(right);
          }
          if (cy > 0 && !visited[up] && isSolid(up)) {
            visited[up] = 1;
            stack.push(up);
          }
          if (cy < h - 1 && !visited[down] && isSolid(down)) {
            visited[down] = 1;
            stack.push(down);
          }
        }

        if (pixels.length >= min) {
          components.push({
            minX,
            minY,
            maxX,
            maxY,
            count: pixels.length,
            pixels: Uint32Array.from(pixels),
          });
        }
      }
    }

    components.sort((a, b) => b.count - a.count);
    return components;
  }

  function extractComponent(rgba, sourceWidth, sourceHeight, component, padding = 0) {
    if (!component || !component.pixels) throw new Error('Missing component pixels');
    const pad = Math.max(0, asPositiveInt(padding, 0));
    const cropWidth = component.maxX - component.minX + 1;
    const cropHeight = component.maxY - component.minY + 1;
    const width = cropWidth + pad * 2;
    const height = cropHeight + pad * 2;
    const data = new Uint8ClampedArray(width * height * 4);

    for (const sourceIndex of component.pixels) {
      const sx = sourceIndex % sourceWidth;
      const sy = (sourceIndex / sourceWidth) | 0;
      if (sx < 0 || sx >= sourceWidth || sy < 0 || sy >= sourceHeight) continue;

      const dx = sx - component.minX + pad;
      const dy = sy - component.minY + pad;
      const sourceOffset = sourceIndex * 4;
      const targetOffset = (dy * width + dx) * 4;

      data[targetOffset] = rgba[sourceOffset];
      data[targetOffset + 1] = rgba[sourceOffset + 1];
      data[targetOffset + 2] = rgba[sourceOffset + 2];
      data[targetOffset + 3] = rgba[sourceOffset + 3];
    }

    return { width, height, data };
  }

  return {
    detectComponents,
    extractComponent,
  };
});
