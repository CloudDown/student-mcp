# Icons

`app-icon.svg` is the editable source of the student-mcp mark: a warm yellow
graduation smiley with a slate-blue cap. Original art for this project.
The transparent background, broad facial features and inset silhouette are
designed for both light and dark surfaces, including small tray icons and tabs.

- `app-icon.png`: 256 × 256, for the README and larger displays.
- `app-icon-64.png`: 64 × 64, served as the web favicon.
- `app-icon-64.rgba`: the same 64 × 64 artwork as straight RGBA, for Linux/Windows trays.
- `app-icon-template.svg`: monochrome macOS menu-bar source, with transparent
  facial features so the system tint preserves the expression in either theme.
- `app-icon-template-64.png` / `.rgba`: rendered macOS template and raw tray pixels.

Regenerate these assets with `python3 tools/render-icons.py` (requires
`rsvg-convert` from librsvg and ImageMagick 7). PNG metadata is stripped and
compression is lossless. No image renderer is needed at build time or runtime.

`moodle-icon.png` is the square app symbol from
[Simple Icons](https://simpleicons.org/) (CC0), downloaded on 2026-10-18:

- https://cdn.jsdelivr.net/npm/simple-icons@latest/icons/moodle.svg

Rasterized at 256 × 256 with an orange fill. The brands and trademarks belong
to their respective owners. These symbols identify the connected services;
they do not imply endorsement.

`omnivox-icon.png` is the official Omnivox product mark from Skytech's site,
downloaded on 2026-10-22 and rasterized at 256 × 256:

https://skytech.com/assets/products/omnivox/icons/iconeOmnivox.svg

The Omnivox brand belongs to Skytech Communications; its use here identifies
the service only.

`brightspace-icon.png` is the Brightspace favicon from D2L's official CDN,
downloaded on 2026-10-20 and rasterized at 256 × 256:
https://s.brightspace.com/lib/branding/1.0.0/brightspace/favicon.ico
The Brightspace brand belongs to D2L; its use here identifies the service only.

`canvas-icon.png` is the square app symbol from
[Simple Icons](https://simpleicons.org/) (CC0), downloaded on 2026-11-04:
https://cdn.jsdelivr.net/npm/simple-icons@15.4.0/icons/canvas.svg
Rasterized at 256 × 256 with Canvas red `#E72429`. The Canvas brand belongs
to Instructure; its use here identifies the service only.

`claude-icon.png`, `chatgpt-icon.png` and `gemini-icon.png` are the square app
symbols from [Simple Icons](https://simpleicons.org/) (CC0), downloaded on
2026-11-08 from simple-icons@13:

- https://cdn.jsdelivr.net/npm/simple-icons@13/icons/claude.svg
- https://cdn.jsdelivr.net/npm/simple-icons@13/icons/openai.svg
- https://cdn.jsdelivr.net/npm/simple-icons@13/icons/googlegemini.svg

Rasterized at 128 × 128: Claude in terracotta `#D97757`, OpenAI in `#0D0D0D`,
Gemini with a blue-to-purple gradient (`#4E8CFF` → `#9B72CB` → `#D96570`).
The brands belong to Anthropic, OpenAI and Google; their use identifies the
assistants the link can be pasted into, without implying endorsement.

# Assistant setup guides

`guide-claude-connectors.webp`, `guide-chatgpt-plugins.webp` and
`guide-gemini-connected-apps.webp` are cropped screenshots of the official
setup instructions, captured on 2026-11-08. They show the vendor's own steps,
not the user's link, account or connector. Sources:

- Claude: https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp
  ("Add a custom connector", Pro and Max steps).
- ChatGPT: https://developers.openai.com/plugins/deploy/connect-chatgpt
  ("Add the MCP server").
- Gemini: https://support.google.com/gemini/answer/17209137
  ("Connect a custom app").

They are lossy WebP (quality 84) to keep the binary small, and served as
`image/webp` by the local UI. Screenshots of vendor interfaces are used only to
explain setup; the products and brands remain their owners' property. Refresh a
capture whenever the vendor documentation changes.
