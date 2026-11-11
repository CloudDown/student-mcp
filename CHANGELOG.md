# Changelog

Notable changes, newest first. Downloads for each release are on the
[releases page](https://github.com/CloudDown/student-mcp/releases).

## 1.2.0 — 2026-11-11

First 1.x release. The 0.x tags and releases stay published as they were.

### Added

- **Bounded course navigation.** `moodle_course_content` (`detail=overview`)
  returns at most 6 sections per page, 8 activities per section and 6 files per
  activity, with `nextPage`, `moduleCount`/`fileCount` and explicit `truncated`
  flags instead of silently dropping entries.
- **Direct section links.** Every section exposes a Moodle
  `course/view.php?id=…&section=…` URL so an assistant can point back to the
  course page.
- **Groups in course views.** The student's groups and the course groups are
  included, and sections or activities whose names plainly contain a group label
  are marked with `group` and `yourGroup`. `group_only=true` hides sections
  labelled for another group; unmarked sections stay visible.
- **File modification times.** Overview files carry `timemodified` when Moodle
  reports it.
- **Grade weights.** `moodle_grades` reports `weightraw` and `weightformatted`
  for each grade item plus Moodle's own course total. When the gradebook returns
  no weights, `weightsAvailable=false` and the response explains that the
  weighted total is unavailable rather than estimating one.
- **Local MCP request log.** Incoming requests are appended to
  `~/.student-mcp.mcp.log` (mode 0600, rotated at 1 MiB) with the time, method,
  redacted path, status and user-agent. Headers, bodies and connector tokens are
  never written.

### Changed

- Original-file share links now expire after **30 minutes** instead of three
  hours. The 128-file / 24 MiB store bounds and inline fallback are unchanged.

### Notes

- Group labels are navigation hints, not proof of an access restriction.
- Overviews and search hits remain navigation, not a complete reading of your
  instructions or attachments.

## 0.8.1 — 2026-11-11

- `moodle_course_content detail=full` reads complete instruction text in
  paginated blocks; media and linked resources are reported, not fetched.
- Clean-install fixes for the Linux installer and document handling.

## 0.8.0 — 2026-11-11

- Named account profiles, per-account assistant enable switches and a reworked
  account management screen.
- Connector instructions list enabled accounts and route `account_id` per
  platform.
- Extracted web UI assets (`index.html`, `app.css`, `app.js`).

## 0.7.0 — 2026-11-11

- Document study windows for PDFs (4 rendered pages per call) and Office/HTML
  text blocks, with `coverage` describing exactly what was delivered.
- Private per-account course index (15-minute freshness) for search and course
  overviews.
