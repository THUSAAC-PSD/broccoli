---
name: broccoli-diagrams
description:
  Use when adding or changing a diagram in the Broccoli docs. Wraps the
  hand-drawn renderer in website/diagrams and the ThemedImage dark-mode
  contract.
---

# Broccoli diagrams

Full system: `docs/authoring/diagrams.md`. This skill is the quick path.

## Add or change a diagram

1. Edit `website/diagrams/scenes.cjs`. Add a scene with `width`, `height`,
   `alt`, `boxes`, `arrows`, `notes`. Box colors come from `PALETTE`. For a
   bilingual diagram add a second scene keyed `name.zh`.
2. Run `cd website && node diagrams/build.cjs`. It writes `name.svg` and
   `name.dark.svg` per scene into `website/static/img/`.
3. Embed with `ThemedImage`, never a bare `![](...)`:

   ```md
   import ThemedImage from '@theme/ThemedImage';

   <ThemedImage alt="describe what the diagram shows"
   sources={{ light: '/img/name.svg', dark: '/img/name.dark.svg' }} />
   ```

4. Run `cd website && pnpm clear && pnpm build` and confirm it is clean.

Never reintroduce an in-SVG `prefers-color-scheme` rule. It causes dark-on-dark.
