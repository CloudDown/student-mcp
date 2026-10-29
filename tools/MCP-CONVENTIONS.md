# MCP tool conventions

Rules for student-mcp tool names, parameters, descriptions and results.
Follow these when adding or changing tools. Prefer compatibility with existing
clients over cosmetic renames.

## Naming

```
{platform}_{resource}
{platform}_{resource}_{action}
```

- Platforms: `moodle_`, `omnivox_`, `brightspace_`.
- Escape hatch only: Moodle `ws_discover` / `ws_schema` / `ws_call`.
- Cross-service discovery: `student_status` (no platform prefix).
- Prefer verbs for writes and multi-step actions (`_submit`, `_create`, `_delete`).
- Prefer nouns for reads (`_grades`, `_deadlines`, `_course_content`).
- Keep platform vocabulary when the upstream product uses it
  (`brightspace_news`, not a forced `*_announcements` alias).

## Parameters

| Rule | Detail |
|------|--------|
| Case | `snake_case` only |
| Multi-account | Every platform tool accepts optional `account_id` (injected centrally). Required only when several accounts exist for that platform. |
| Course filter | Singular `courseid` / `course_id` for one course; plural `courseids` for lists |
| IDs | Positive integers when the platform uses numeric IDs; strings when the platform does (Omnivox `course_id` like `2434K5EM.1011`) |
| Limits | Always bound (`minimum` / `maximum`); clamp server-side too |
| Pagination | Preserve upstream cursors (`bookmark`, `page`, `nextPageFrom`). Never imply the first page is complete |
| Files | Never accept raw school cookies/tokens as tool args. Moodle uses `fileurl` from content tools; Brightspace uses `courseid` + file `topicid` |

Do **not** rename established keys only for symmetry (`courseid` vs
`course_id`). Document the platform difference instead.

## Descriptions

Write for the assistant that must choose a tool:

1. What the tool returns (compact summary vs raw dump).
2. Which sibling to prefer for the common job
   (`Prefer moodle_deadlines for due work`).
3. Where IDs come from (`courseid from moodle_my_courses`).
4. Limits and non-goals (`not full-text PDF search`, `not private email`).
5. Write side effects and confirmation expectations.

Keep descriptions under a short paragraph. Put long procedure notes in
`tools/GUIDE.md`.

## Annotations

| Annotation | When |
|------------|------|
| `readOnlyHint: true` | No server-side mutation |
| `readOnlyHint: false` | Any write, submit, dismiss, pin, subscribe, delete, or generic escape hatch |
| `destructiveHint: true` | Hard to undo (submit, delete, leave group) |
| `idempotentHint` | Safe to retry with the same args when true |
| `openWorldHint: true` | Talks to an external school system (typical) |

Write-tool approvals belong to the MCP client. Annotations describe the risk,
but clients other than Claude may not prompt automatically; confirm the user's
intent before performing irreversible actions.

## Prefer / avoid matrix

| Job | Prefer | Avoid as first choice |
|-----|--------|------------------------|
| “What is due?” | `*_deadlines` | Combining calendar + assignments + quizzes by hand |
| Find a TD / file | `moodle_search` or course content + query | Blind full course dumps |
| Read a PDF/Office doc | `moodle_file` / `brightspace_file` | Generic HTTP fetch of school URLs |
| Rare Moodle action | dedicated tool, else `ws_discover` → `ws_call` | `ws_call` for everyday reads |
| Account routing | `student_status` then `account_id` | Guessing which account |

## Results

- Errors: `isError: true` and a short plain-text reason. No stack traces, no
  tokens, no cookies.
- Ordinary JSON tools: pretty-printed text **and** `structuredContent` with the
  same object (machine-friendly).
- File tools: MCP content blocks only (resource / text / images). Do not put
  binary payloads in `structuredContent`.
- Scrub secrets from every payload before return.
- Keep responses compact: drop hidden/ended noise unless the caller opted in.

## Security gates

1. Dedicated student tools first; no open URL/path proxy.
2. `ws_call` when Moodle writes are off:
   - allow only catalogue-classified **read** functions;
   - refuse catalogue **write** functions;
   - refuse **unclassified** functions (plugins / unknown names).
3. Brightspace writes use fixed routes owned by the server, never caller URLs.
4. Omnivox stays read-only.

## Adding a tool checklist

1. Name matches `{platform}_{resource}[_action]`.
2. `inputSchema` is always an object; required keys listed under `required`.
3. Description says prefer/avoid and ID provenance.
4. Annotations match mutation behaviour.
5. Dispatch path resolves `account_id` and write gates.
6. Fixture test covers success + auth/write refusal.
7. Update `tools/GUIDE.md` only if the workflow is user-visible.

## Intentional platform differences

| Topic | Moodle | Brightspace | Omnivox |
|-------|--------|-------------|---------|
| Course id key | `courseid` (int) | `courseid` (int) | `course_id` (string) |
| News | `moodle_announcements` | `brightspace_news` | calendar/docs only |
| Search | cross-course materials | TOC titles/URLs in one course | none yet |
| File read | `moodle_file` + `fileurl` | `brightspace_file` + topic id | list only today |
| Escape hatch | `ws_*` | fixed `student_tools` routes | none |
| Writes | opt-in + catalogue class | opt-in + route method | no |
