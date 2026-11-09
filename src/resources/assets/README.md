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

`grok-icon.png` is the xAI Grok mark used by the assistant switcher. The Grok
brand belongs to xAI.

`local-*.png` are the logos shown for locally detected assistants, downloaded
from each vendor on 2026-11-09 and normalized to 128 × 128 (artwork fitted to
112 px, transparent margin, metadata stripped). Claude Desktop, Codex and Gemini
CLI reuse `claude-icon.png`, `chatgpt-icon.png` and `gemini-icon.png`:

- opencode: https://opencode.ai/favicon.svg
- cursor: https://cursor.com/marketing-static/icon-192x192-light.png
- vscode: https://code.visualstudio.com/assets/apple-touch-icon.png
- windsurf: https://windsurf.com/favicon.svg
- qwen: https://qwen.ai (Alibaba CDN favicon, 80 × 80 source)
- pi: https://pi.dev/favicon.svg
- omp: https://omp.sh/favicon.png
- kiro: https://kiro.dev/icon.svg
- amp: https://ampcode.com/app-icon.svg (press kit)
- crush: crop of https://stuff.charm.sh/crush/charm-crush.png
- devin: https://devin.ai/icon.png

These marks belong to their respective owners and only identify the app that
student-mcp can configure.

# Assistant setup guides

## Rendered captures (2026-11-09)

The four assistant guides use one render per step, after the link step. They are
**not** screenshots of a live session: each one is rendered from
`tools/assistant-guides.html` by `tools/render-assistant-guides.mjs`, so the
labels, the empty states and the arrow stay identical across releases. The page
frame and the app chrome are kept; the browser tab strip and address bar are not.

- One scene per assistant and step, **480 × 360 CSS px at 3×** → 1440 × 1080
  WebP. The captures are displayed at 480 px wide in the app, so their text is
  read at its natural size. Do not widen the canvas without widening that
  column: anything smaller becomes unreadable at 1:1.
- One thin arrow per capture, in the assistant's accent colour, pointing a few
  pixels away from the control to click. No numbered badges, no legend, no
  caption, no browser chrome. Clicking a capture opens the in-page viewer.
- Fictional data only: `student-mcp`, `https://example.com/mcp`, no account, no
  real MCP link, no school credential. Vendor labels are in English, as the
  assistants ship them; the surrounding UI is translated.
- Each form capture shows the state just before the final click: fields filled
  and the action button enabled.

Files: `guide-claude-menu|form|chat`, `guide-chatgpt-menu|form|chat`,
`guide-gemini-settings|form|chat`, `guide-grok-settings|form|chat`.

Regenerate after editing the scenes:

```bash
export PLAYWRIGHT_MODULE=/path/to/node_modules/playwright/index.mjs
export STUDENT_MCP_BROWSER=$(ls -d ~/.cache/ms-playwright/chromium-*/chrome-linux*/chrome | head -1)
node tools/render-assistant-guides.mjs
```

The script fails if a scene is not exactly 480 × 360, if an output is not
1440 × 1080, or if a WebP exceeds 120 KB. The arrow is part of the annotation,
not of the product: keep it out of any capture that claims to be a screenshot.

`grok-icon.png` is the official Grok app icon from
https://grok.com/images/android-chrome-192x192.png, resized to 128 × 128.
It identifies the assistant only; the brand remains its owner's property.
Grok instructions were checked against https://docs.x.ai/grok/connectors
and the signed-in Helium interface on 2026-11-08. Brand names and control
labels in the renders follow those interfaces; the products and brands remain
their owners' property. Refresh a capture whenever a vendor renames a control.

`tools/test-assistant-guides.mjs` checks the four assistant guides at phone,
tablet and desktop widths, using fixture accounts and example URLs only.
Set `STUDENT_MCP_GUIDE_SCREENS` to save local screenshots for visual review.