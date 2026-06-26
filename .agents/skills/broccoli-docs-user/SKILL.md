---
name: broccoli-docs-user
description:
  Use when writing user-facing Broccoli docs for contestants and volunteers.
  Lives in the Using Broccoli section. Read docs/authoring/house-style.md first.
---

# User-facing docs

For contestants and volunteers. The reader may be non technical and is often
under time pressure.

Read `docs/authoring/house-style.md` and `docs/authoring/diagrams.md` first.
This skill adds only the user-facing specifics.

## Voice

Warm, calm, plain, second person. No jargon. If a term is unavoidable, define it
once. Lead with the action or command, then one line of why. Say contestants and
volunteers, never users or staff.

## Punctuation

Strict. No em dashes, no colons, no hyphens in prose.

## Location and shape

- English `website/docs/<section>/<name>.md`. Chinese mirror under
  `website/i18n/zh-CN/docusaurus-plugin-content-docs/current/<same path>`.
- Section in the sidebar: Using Broccoli.
- Task oriented. Open with what it is and who it is for, then install or first
  setup, then the main tasks in the order the reader does them. Lead each
  practical section with a real command.
- Reference page model: `website/docs/plugins/printing.md`.

## Wire it in

Add the page id to `website/sidebars.ts` under the Using Broccoli section.
