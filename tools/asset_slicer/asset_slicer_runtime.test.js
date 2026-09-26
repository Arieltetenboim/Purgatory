const assert = require('node:assert/strict');
const slicer = require('./asset_slicer_runtime.js');

function rgba(width, height, points) {
  const data = new Uint8ClampedArray(width * height * 4);
  for (const [x, y, alpha = 255, rgb = [10, 20, 30]] of points) {
    const offset = (y * width + x) * 4;
    data[offset] = rgb[0];
    data[offset + 1] = rgb[1];
    data[offset + 2] = rgb[2];
    data[offset + 3] = alpha;
  }
  return data;
}

{
  const data = rgba(5, 3, [
    [0, 0], [1, 0], [0, 1],
    [4, 2],
  ]);
  const components = slicer.detectComponents(data, 5, 3, 0, 1);
  assert.equal(components.length, 2);
  assert.equal(components[0].count, 3);
  assert.deepEqual(
    [components[0].minX, components[0].minY, components[0].maxX, components[0].maxY],
    [0, 0, 1, 1],
  );
}

{
  const data = rgba(2, 2, [[0, 0], [1, 1]]);
  const components = slicer.detectComponents(data, 2, 2, 0, 1);
  assert.equal(components.length, 2, 'diagonal pixels stay separate: Character Lab uses 4-neighbor connectivity');
}

{
  const data = rgba(4, 1, [[0, 0], [1, 0], [3, 0]]);
  const components = slicer.detectComponents(data, 4, 1, 0, 2);
  assert.equal(components.length, 1);
  assert.equal(components[0].count, 2);
}

{
  const data = rgba(3, 1, [[0, 0, 1], [1, 0, 2], [2, 0, 255]]);
  assert.equal(slicer.detectComponents(data, 3, 1, 1, 1).length, 1);
  assert.equal(slicer.detectComponents(data, 3, 1, 2, 1).length, 1);
  assert.equal(slicer.detectComponents(data, 3, 1, 254, 1).length, 1);
}

{
  const data = rgba(4, 3, [
    [1, 1, 255, [100, 110, 120]],
    [2, 1, 255, [130, 140, 150]],
  ]);
  const [component] = slicer.detectComponents(data, 4, 3, 0, 1);
  const crop = slicer.extractComponent(data, 4, 3, component, 1);
  assert.equal(crop.width, 4);
  assert.equal(crop.height, 3);
  const first = (1 * crop.width + 1) * 4;
  assert.deepEqual(Array.from(crop.data.slice(first, first + 4)), [100, 110, 120, 255]);
  const transparentCorner = 0;
  assert.deepEqual(Array.from(crop.data.slice(transparentCorner, transparentCorner + 4)), [0, 0, 0, 0]);
}

console.log('asset_slicer_runtime tests passed');
