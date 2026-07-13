# Broccoli docs diagrams

One hand drawn style across the whole site, produced by the renderer in
`website/diagrams/`. roughjs draws the strokes, Excalifont provides the
handwriting.

## How it works

- `website/diagrams/render.cjs` turns a scene object into an SVG string.
  `render(scene, dark)` returns the light variant by default and the dark
  variant when `dark` is true.
- `website/diagrams/scenes.cjs` holds one scene per diagram. A scene has
  `width`, `height`, `alt`, `boxes`, `arrows`, and `notes`. Box colors come from
  `PALETTE` (blue, green, yellow, gray, red, purple).
- `website/diagrams/build.cjs` writes `name.svg` and `name.dark.svg` for every
  scene into `website/static/img/`.

## Add a diagram

1. Add a scene to `scenes.cjs`. For a bilingual diagram add a second scene keyed
   `name.zh` with translated labels.
2. Run `cd website && node diagrams/build.cjs`.
3. Embed it with `ThemedImage` so it follows the site theme.

## Dark mode

Diagrams must follow the Docusaurus theme toggle, not the operating system.
Embed both variants through `ThemedImage`, never a bare `![](...)`.

```md
import ThemedImage from '@theme/ThemedImage';

<ThemedImage alt="describe what the diagram shows"
sources={{ light: '/img/name.svg', dark: '/img/name.dark.svg' }} />
```

The Chinese page points at `name.zh.svg` and `name.zh.dark.svg`.

Do not reintroduce an in-SVG `@media (prefers-color-scheme: dark)` rule. An
`<img>` embedded SVG reads the operating system scheme, which produces
dark-on-dark when the site theme and the OS disagree.
