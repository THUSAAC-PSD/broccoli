# Diagrams

Hand-drawn (Excalidraw-style) docs diagrams, generated to `../static/img/*.svg`.

- `render.cjs` — the renderer. Turns a scene into an SVG using rough.js and the
  embedded Excalifont. Backgrounds are transparent and adapt to dark mode.
- `scenes.cjs` — the scenes (plain data: boxes, arrows, notes). One factory per
  diagram renders both locales from a shared layout.
- `build.cjs` — writes every scene to `static/img`.

## Regenerate

```bash
pnpm diagrams
```

## Add a diagram

Add an entry to the `module.exports` map in `scenes.cjs` keyed by output name
(e.g. `my-diagram` writes `static/img/my-diagram.svg`), then run `pnpm diagrams`
and reference it from a page with `![alt](/img/my-diagram.svg)`.

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
