# Broccoli docs house style

Shared rules for every documentation page, in all three audiences.

## Pick the audience first

Every page serves exactly one audience. The audience decides voice, location,
and structure.

- User facing. Contestants and volunteers. Section: Using Broccoli.
- Plugin developer facing. People building plugins on the SDKs. Section:
  Building plugins.
- Maintainer facing. People working on the host platform. Section: Internals.

A subject is not an audience. One subject often becomes several pages in several
sections.

## Never do this

- No fluff. Banned vocabulary includes powerful, seamless, robust, flexible,
  simply, just, easily, leverage, utilize. Cut any sentence that restates the
  heading or the obvious.
- Nothing ungrounded. Every command, flag, path, type, and behavior is read from
  the source and is real. Show a working example, do not describe one.
- No wrong shape. Page shape follows the reader task, not the code layout. One
  job per page, with a clear first step.
- No robotic tics. No em dashes anywhere. No In conclusion, Note that, or
  Overall scaffolding. No hedging when you can be definite. No identical
  paragraph template on every page.
- One name per concept, matching the code.

## Punctuation

- Em dashes are banned in all three audiences.
- No colons and no hyphens in prose applies to user facing docs only. Plugin
  developer and maintainer prose may use colons and hyphens.

## Bilingual

All three audiences ship English and Simplified Chinese. The Chinese page
translates the prose, keeps every command and code block identical, uses formal
technical Chinese, and relies on the CJK auto anchor for headings with no
explicit id.

## Mechanics

- Frontmatter is `title`, `sidebar_label`, `sidebar_position` and nothing else.
- Use ` ```bash ` for commands and ` ```toml ` for config. The site highlights
  both.
- Admonitions use bracket titles, for example `:::note[Title]`. Never write
  `:::note Title`.
- Diagrams follow `docs/authoring/diagrams.md`.

## Done means

- `cd website && pnpm clear && pnpm build` passes with no broken link or MDX
  error.
- Every command and flag is confirmed against the source.
- English and Chinese match in structure with identical code blocks.
