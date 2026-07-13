# Documentation site

The Broccoli docs site, built with [Docusaurus](https://docusaurus.io/). Pages
live in `docs/`, with the Simplified Chinese mirror under
`i18n/zh-CN/docusaurus-plugin-content-docs/`.

Write pages against the guides in [`../docs/authoring`](../docs/authoring):
`house-style.md` for voice and structure, `diagrams.md` for diagrams.

## Develop

```bash
pnpm install
pnpm start
```

`pnpm start` serves the English site with hot reload. To preview the Chinese
locale, pass it explicitly:

```bash
pnpm start --locale zh-CN
```

## Build

```bash
pnpm build
pnpm serve
```

`pnpm build` writes the static site for every locale to `build/` and `pnpm serve`
serves that output. The build fails on a broken internal link, because the config
sets `onBrokenLinks: 'throw'`. Run `pnpm clear` first if a stale cache produces
results that do not match the source.

## Diagrams

Diagrams are generated, not drawn by hand in an editor. Edit the scene data and
regenerate:

```bash
pnpm diagrams
```

See [`diagrams/README.md`](diagrams/README.md) for the scene format and the dark
mode contract.

## Translations

Heading anchors for Chinese pages come from the CJK auto-anchor, so headings need
no explicit id. When you add UI strings or theme labels that Docusaurus owns,
regenerate the translation stubs:

```bash
pnpm write-translations --locale zh-CN
```
