# Diagrams

Hand-drawn (Excalidraw-style) docs diagrams. rough.js draws the strokes,
Excalifont provides the handwriting. The renderer emits a light and a dark
variant for every diagram so they follow the site theme.

- `render.cjs` turns a scene into an SVG. `render(scene, dark)` returns the light
  variant by default and the dark variant when `dark` is true. Output is seeded,
  so regenerating produces identical bytes.
- `scenes.cjs` holds the scenes (plain data: boxes, arrows, notes). One factory
  per diagram can render several locales from a shared layout.
- `build.cjs` writes `name.svg` and `name.dark.svg` for every scene into
  `../static/img/`.

## Regenerate

```bash
pnpm diagrams
```

This rewrites every diagram. Because the renderer is seeded, only the scenes you
actually changed produce a diff.

## Add a diagram

1. Add an entry to the `module.exports` map in `scenes.cjs`, keyed by output name
   (so `my-diagram` writes `static/img/my-diagram.svg` and `my-diagram.dark.svg`).
   For a bilingual diagram, add a second entry keyed `my-diagram.zh` with
   translated labels.
2. Run `pnpm diagrams`.
3. Embed both variants on the page with `ThemedImage`, never a bare `![](...)`.
   See [`../../docs/authoring/diagrams.md`](../../docs/authoring/diagrams.md) for
   the full contract.

```md
import ThemedImage from '@theme/ThemedImage';

<ThemedImage alt="describe what the diagram shows"
sources={{ light: '/img/my-diagram.svg', dark: '/img/my-diagram.dark.svg' }} />
```

An `<img>` embedded SVG reads the operating system color scheme, so a single
image goes dark-on-dark when the OS and the site theme disagree. `ThemedImage`
swaps the two variants on the site toggle instead. Do not add an in-SVG
`@media (prefers-color-scheme: dark)` rule.

## The architecture diagram

`architecture.svg` and `architecture.dark.svg` are also used by the root
`README.md`, which embeds them with an HTML `<picture>` element so GitHub honors
its own dark mode. Keep the path in that README in step with this directory.

## Fonts

Excalifont (SIL OFL, `../.fonts/Excalifont-OFL.txt`) is embedded as a subset in
`../.fonts/Excalifont.woff2`. If new labels use characters outside the current
subset, re-subset from the full font:

```bash
python3 -m fontTools.subset .fonts/Excalifont-Regular.woff2 \
  --unicodes="U+0020-007E,U+00B7,U+2013,U+2014,U+2018,U+2019,U+201C,U+201D,U+2022,U+2026,U+2192" \
  --flavor=woff2 --output-file=.fonts/Excalifont.woff2
```

CJK labels fall back to the reader's system font, so they need no subsetting.
