const fs = require('fs');
const path = require('path');
const { render } = require('./render.cjs');
const scenes = require('./scenes.cjs');

const outDir = path.join(__dirname, '..', 'static', 'img');
for (const [name, scene] of Object.entries(scenes)) {
  const light = path.join(outDir, `${name}.svg`);
  const dark = path.join(outDir, `${name}.dark.svg`);
  fs.writeFileSync(light, render(scene, false));
  fs.writeFileSync(dark, render(scene, true));
  console.log(`wrote ${name}.svg + ${name}.dark.svg  (${scene.width}x${scene.height})`);
}
