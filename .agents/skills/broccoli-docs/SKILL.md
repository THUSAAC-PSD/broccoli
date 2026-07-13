---
name: broccoli-docs
description:
  Use when writing or updating any documentation for Broccoli. Routes to the
  right audience sub-skill and points at the shared authoring guides. Trigger
  for any request to document a CLI, plugin, SDK, or host platform feature.
---

# Broccoli docs

All Broccoli docs live on the Docusaurus site under `website/` and ship in
English and Simplified Chinese. Start here, then use the child skill for the
audience.

## Read the shared guides first

- `docs/authoring/house-style.md` voice, never-do list, punctuation, bilingual
  rules. Applies to every page.
- `docs/authoring/diagrams.md` the hand-drawn diagram system and dark mode.

## Pick the audience, then the child skill

A subject is not an audience. One subject often becomes several pages.

- User facing, for contestants and volunteers. Use `broccoli-docs-user`.
- Plugin developer facing, for people building on the SDKs. Use
  `broccoli-docs-plugin-dev`.
- Maintainer facing, for people working on the host platform. Use
  `broccoli-docs-maintainer`.

For any diagram, use `broccoli-diagrams`.

## Always

- Ground every command, flag, path, and behavior in the source. Invent nothing.
- Write the English page, then the Chinese mirror with identical code blocks.
- Finish only when `cd website && pnpm clear && pnpm build` passes clean.
