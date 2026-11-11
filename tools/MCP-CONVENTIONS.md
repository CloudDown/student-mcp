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
| Multi-account | Every platform tool accepts `account_id` (injected centrally). Optional when that platform has one enabled account. Required, with an `enum` of ids and names in the description, when several accounts are enabled. Disabled accounts are omitted from `tools/list`, `initialize` instructions and `student_status`. |
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
intent before performing irreversible actions. Definitions are normalized so
every `readOnlyHint:false` tool also carries `destructiveHint:true`,
`openWorldHint:true` and an explicit `idempotentHint` (default `false`).

## Prefer / avoid matrix

| Job | Prefer | Avoid as first choice |
|-----|--------|------------------------|
| “What is due?” | `moodle_deadlines`, `brightspace_deadlines`, `omnivox_deadlines`, `canvas_deadlines` | Combining calendar + assignments + quizzes by hand |
| Find a TD / file | `moodle_search` or course content + query | Blind full course dumps |
| Read a PDF/Office doc | `moodle_file` / `brightspace_file` / `omnivox_file` | Generic HTTP fetch of school URLs |
| Rare Moodle action | dedicated tool, else `ws_discover` → `ws_call` | `ws_call` for everyday reads |
| Account routing | Match the person/school in `initialize` instructions or `student_status`, then pass that `account_id` | Guessing which account, mixing course IDs across accounts |

## Results

- Errors: `isError: true` and a short plain-text reason. No stack traces, no
  tokens, no cookies.
- Ordinary JSON tools: pretty-printed text **and** `structuredContent` with the
  same object (machine-friendly).
- File tools: MCP content blocks only (resource / text / images). Do not put
  binary payloads in `structuredContent`.
- Scrub secrets from every payload before return.
- Keep responses compact: drop hidden/ended noise unless the caller opted in.

## Capability boundaries

There is **no server-side write permission switch**. Annotations describe the
risk; the MCP client decides whether to ask for confirmation or run the action.
student-mcp still enforces technical boundaries:

1. Dedicated student tools first; no open URL/path proxy.
2. `ws_call` accepts only functions the token advertises. Moodle still enforces
   the token's own capabilities and role permissions.
3. Brightspace writes use fixed routes owned by the server, never caller URLs.
4. Omnivox writes use fixed mobile endpoints with explicit payload validation.
5. A write whose response is ambiguous reports "not confirmed" instead of
   pretending success. No write is retried automatically.

## Adding a tool checklist

1. Name matches `{platform}_{resource}[_action]`.
2. `inputSchema` is always an object; required keys listed under `required`.
3. Description says prefer/avoid and ID provenance.
4. Annotations match mutation behaviour.
5. Dispatch path resolves `account_id` and validates arguments.
6. Fixture test covers success and auth/refusal.
7. Update `tools/GUIDE.md` only if the workflow is user-visible.

## Intentional platform differences

| Topic | Moodle | Brightspace | Omnivox |
|-------|--------|-------------|---------|
| Course id key | `courseid` (int) | `courseid` (int) | `course_id` (string) |
| News | `moodle_announcements` | `brightspace_news` | calendar/docs only |
| Search | cross-course materials | TOC titles/URLs in one course | none yet |
| File read | `moodle_file` + `fileurl` | `brightspace_file` + topic id | `omnivox_file` + category/id |
| Escape hatch | `ws_*` | fixed `student_tools` routes | none |
| Writes | dedicated tools + `ws_call` (token capabilities) | fixed routes, method from the route | fixed mobile endpoints |
| Write result | Moodle JSON exception/errorcode | empty 2xx or JSON without error fields | explicit confirmation fields |
