const assert = require('node:assert');
const { render } = require('./render.cjs');

const scene = {
  width: 200,
  height: 120,
  alt: 't',
  boxes: [{ x: 20, y: 20, w: 80, h: 50, color: 'blue', lines: ['A'] }],
  arrows: [{ x1: 100, y1: 45, x2: 160, y2: 45, ax: 100, ay: 45 }],
  notes: [{ x: 100, y: 100, text: 'n' }],
};

const light = render(scene, false);
const dark = render(scene, true);

assert.ok(!light.includes('prefers-color-scheme'), 'light must not embed media query');
assert.ok(!dark.includes('prefers-color-scheme'), 'dark must not embed media query');
assert.ok(light.includes('#1e1e1e'), 'light edges/text use dark ink');
assert.ok(dark.includes('#c9c9c9'), 'dark edges use light stroke');
assert.notStrictEqual(light, dark, 'light and dark differ');

console.log('render.test.cjs passed');
