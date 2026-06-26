const fs = require('fs');
const path = require('path');
const { render } = require('./render.cjs');
const scenes = require('./scenes.cjs');

const outDir = path.join(__dirname, '..', 'static', 'img');
for (const [name, scene] of Object.entries(scenes)) {
  const file = path.join(outDir, `${name}.svg`);
  fs.writeFileSync(file, render(scene));
  console.log(`wrote ${file}  (${scene.width}x${scene.height})`);
}
