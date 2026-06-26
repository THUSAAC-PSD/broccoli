const fs = require('fs');
const path = require('path');
const rough = require('roughjs');

const FONT =
  "Excalifont, 'PingFang SC', 'Microsoft YaHei', 'Noto Sans CJK SC', ui-sans-serif, sans-serif";

const PALETTE = {
  ink: '#1e1e1e',
  sub: '#4a4a4a',
  blue: '#a5d8ff',
  green: '#b2f2bb',
  yellow: '#ffec99',
  gray: '#e9ecef',
  red: '#ffc9c9',
  purple: '#d0bfff',
};

const gen = rough.generator();
const fontB64 = fs
  .readFileSync(path.join(__dirname, '..', '.fonts', 'Excalifont.woff2'))
  .toString('base64');

const esc = (s) =>
  String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function roundedRectPath(x, y, w, h, r) {
  return [
    `M${x + r},${y}`,
    `H${x + w - r}`,
    `Q${x + w},${y} ${x + w},${y + r}`,
    `V${y + h - r}`,
    `Q${x + w},${y + h} ${x + w - r},${y + h}`,
    `H${x + r}`,
    `Q${x},${y + h} ${x},${y + h - r}`,
    `V${y + r}`,
    `Q${x},${y} ${x + r},${y}`,
    'Z',
  ].join(' ');
}

function pathsFor(drawable) {
  return gen
    .toPaths(drawable)
    .map(
      (p) =>
        `<path d="${p.d}" stroke="${p.stroke}" stroke-width="${p.strokeWidth}" fill="${p.fill || 'none'}"/>`,
    )
    .join('');
}

function box(b) {
  const fill = PALETTE[b.color] || PALETTE.blue;
  const d = roundedRectPath(b.x, b.y, b.w, b.h, b.r ?? 12);
  const filled = gen.path(d, { fill, fillStyle: 'solid', stroke: 'none', roughness: 0 });
  const stroked = gen.path(d, {
    stroke: PALETTE.ink,
    strokeWidth: 1.6,
    roughness: 1.5,
    bowing: 1.2,
    fill: 'none',
  });
  let svg = pathsFor(filled) + pathsFor(stroked);
  const cx = b.x + b.w / 2;
  const titleSize = b.titleSize ?? 18;
  const subSize = 13.5;
  const lineH = 20;
  const totalH = titleSize + (b.lines.length - 1) * lineH;
  let ty = b.y + b.h / 2 - totalH / 2 + titleSize - 4;
  b.lines.forEach((ln, i) => {
    const size = i === 0 ? titleSize : subSize;
    const color = i === 0 ? PALETTE.ink : PALETTE.sub;
    svg += `<text x="${cx}" y="${ty}" font-family="${FONT}" font-size="${size}" fill="${color}" text-anchor="middle">${esc(ln)}</text>`;
    ty += lineH;
  });
  return svg;
}

function arrowHead(x, y, angle) {
  const len = 11;
  const spread = 0.5;
  const mk = (s) =>
    gen.line(x, y, x + len * Math.cos(s), y + len * Math.sin(s), {
      stroke: PALETTE.ink,
      strokeWidth: 1.6,
      roughness: 1.2,
    });
  return pathsFor(mk(angle + Math.PI - spread)) + pathsFor(mk(angle + Math.PI + spread));
}

function arrow(a) {
  const { x1, y1, x2, y2 } = a;
  const opts = { stroke: PALETTE.ink, strokeWidth: 1.6, roughness: 1.1, bowing: a.bow ?? 1 };
  let line = a.curve
    ? pathsFor(gen.path(a.curve, { ...opts, fill: 'none' }))
    : pathsFor(gen.line(x1, y1, x2, y2, opts));
  if (a.dash) line = line.replace(/<path /g, '<path stroke-dasharray="6 5" ');
  const ang = Math.atan2(y2 - (a.ay ?? y1), x2 - (a.ax ?? x1));
  line += arrowHead(x2, y2, ang);
  line = line.replace(/stroke="#1e1e1e"/g, 'class="edge"');
  let svg = line;
  if (a.label) {
    const mx = a.lx ?? (x1 + x2) / 2;
    const my = a.ly ?? (y1 + y2) / 2;
    const w = [...a.label].reduce((s, ch) => s + (/[　-鿿＀-￯]/.test(ch) ? 13 : 7.2), 0) + 16;
    svg += `<rect x="${mx - w / 2}" y="${my - 11}" width="${w}" height="22" rx="6" class="chip"/>`;
    svg += `<text x="${mx}" y="${my + 4}" font-family="${FONT}" font-size="12.5" class="note" text-anchor="middle">${esc(a.label)}</text>`;
  }
  return svg;
}

function render(scene, dark = false) {
  const { width, height } = scene;
  const theme = dark
    ? { edge: '#c9c9c9', note: '#c2c2c2', chipFill: '#26262b', chipStroke: '#45454d' }
    : { edge: '#1e1e1e', note: '#4a4a4a', chipFill: '#ffffff', chipStroke: '#e0e0e0' };
  let body = '';
  (scene.arrows || []).forEach((a) => (body += arrow(a)));
  (scene.boxes || []).forEach((b) => (body += box(b)));
  (scene.notes || []).forEach((n) => {
    body += `<text x="${n.x}" y="${n.y}" font-family="${FONT}" font-size="${n.size ?? 12.5}" class="note" text-anchor="${n.anchor ?? 'middle'}">${esc(n.text)}</text>`;
  });
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" role="img" aria-label="${esc(scene.alt || '')}">
<defs><style>
@font-face{font-family:'Excalifont';src:url(data:font/woff2;base64,${fontB64}) format('woff2');font-display:swap;}
.edge{stroke:${theme.edge};}
.note{fill:${theme.note};}
.chip{fill:${theme.chipFill};stroke:${theme.chipStroke};stroke-width:1;}
</style></defs>
${body}
</svg>`;
}

module.exports = { render, PALETTE };
