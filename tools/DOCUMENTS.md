# Student documents and Moodle updates

`moodle_file`, `omnivox_file`, `brightspace_file` and `canvas_file` share the same local document reader.
No university-specific filename or course-code convention is required.

| Format | Reading support |
| --- | --- |
| PDF | Study: text by page plus a full page image for every selected page, with Poppler |
| Word `.docx`, `.docm` | Isolated local PDF conversion; fallback: main-body text and table cells |
| PowerPoint `.pptx`, `.pptm` | Isolated local PDF conversion; fallback: text and table cells in presentation order |
| Excel `.xlsx`, `.xlsm`, `.xls` | Sheet names, row numbers, stored cell values |
| OpenDocument `.odt`, `.odp`, `.ods` | PDF conversion for documents/slides; structured cell text for sheets |
| Legacy Word `.doc` | Isolated PDF conversion; text fallback requires local `antiword` |
| Legacy PowerPoint `.ppt` | Isolated PDF conversion; text fallback requires local `catppt` |
| Plain text / CSV | Existing text extraction, depending on the supplied MIME type |
| HTML (Moodle book chapters) | Original `index.html`, or HTML source in text blocks; links/images are not rendered or fetched |

## Download vs study

`format=file` (default) always returns the **original**, with no extraction or
conversion, including Office files. Share `links.download` if present, never a
school-internal URL. Links last 24 hours by default (`link_ttl_hours` in the config, 1–168) while the app runs, and survive restarts until that deadline; unexpired links are kept in the private folder `~/.student-mcp.links/` and deleted at expiry. A link/resource does
not guarantee that a client passes the file contents to its model.

`format=text` and `format=images` both study a PDF window: 4 pages by default,
8 maximum, page-labeled text (20,000 characters maximum) and **every page image**.
Vector figures are preserved, not guessed from raster images or text density.
An incomplete render is an error, not silent omission; retry a smaller window.
`find` searches the text layer before selecting pages; scans have no searchable
text unless they already contain a text layer. No OCR or cloud conversion.
Follow `nextPageFrom` and `textTruncated`; never assume omitted pages were read.
The `coverage` object identifies original downloads, page windows or fallback
text blocks. `allPagesDelivered`/`allBlocksDelivered` describe **this response**,
not accumulated reads or model comprehension; a download alone delivers no study
window. For an exhaustive request, read each relevant source and all its windows.

Each response exposes `documentId` and `expiresInSeconds`. Pass that ID back as
`document_id` to the **same tool/account** instead of the school identifiers.
Originals and one-time study preparation remain in memory for at most 5 minutes
(4 originals, 24 MiB total; eviction possible). Re-load from the school after
expiry or app restart. An ID cannot authorize access to another/disabled account.

## Optional Office conversion

On Linux, system LibreOffice (`/usr/bin/libreoffice` or `soffice`), Bubblewrap and
`prlimit` enable isolated PDF conversion. The sandbox exposes only system runtime
files and a private job directory, not the host home or school credentials. It
has no network, an empty inherited environment, a new profile with macros disabled,
30-second wall deadline, 25-second CPU limit, 2 GiB address-space limit and
32 MiB per-file/output-PDF limit. Temporary files are removed after the operation.
Converted PDFs use the same study path; downloads remain the original Office file.
Conversion can change pagination/fonts; it is a rendering, not proof of identical
Microsoft Office layout. External network assets cannot be fetched.

There is **no unsandboxed conversion**. Missing dependencies, unsupported platforms
or failed conversion fall back to text with `conversion=unavailable` and
`visualLimitations`: charts, equations, layout, headers/footers and notes may be
absent. Spreadsheets stay sheet/row/cell text; formulas are not recalculated and
cached values may be stale. Password-protected documents are unsupported for study.

Fallback Office, spreadsheet and plain-text results contain `blocks` of at most
8,000 characters, at most two per call.
`page_from` / `page_to` select **text blocks**, not printed pages or slide numbers.
Each block includes `sourceUnit` (1-based slide/sheet/document index); the result
names the unit type. Follow `nextPageFrom` until `truncated` is false. `find`
locates matching blocks in extracted text. These blocks are not page images,
even when the requested format was `images`.

The existing 12 MiB download ceiling still applies. ZIP-based documents are
checked for at most 4096 entries and 32 MiB declared expanded size. Text, slide,
sheet and cell limits reject oversized results rather than silently omitting them.
These limits are safeguards, not an OS sandbox for file parsers. Keep dependencies
and optional legacy extraction utilities updated.

## Verification

`cargo test --offline --lib` covers mixed vector/text PDFs with Poppler, pagination,
original Office downloads, explicit fallback, numeric CSV, and account-isolated IDs.
PDF tests need Poppler and skip when it is absent. Optional local checks:

```sh
cargo test --offline --lib office_sandbox_hides_home_environment_and_host_network -- --ignored
cargo test --offline --lib office_conversion_renders_a_real_docx -- --ignored
```

The latter also needs LibreOffice. These tests use synthetic files, no school
credentials. They do not prove what Claude, ChatGPT or Grok displays: verify each
client's image/resource handling separately.

## Moodle tools

- `moodle_search`: ranked metadata search over full activity descriptions as well
  as titles, filenames and sections. Private account snapshots last 15 minutes;
  `refresh=true` fetches current metadata. Follow `continuation`, and check
  `partial`/`errors`. `source=global` queries Moodle's own index, whose coverage is
  not guaranteed. Attached document bytes are not indexed automatically.
- `moodle_course_content`: `detail=overview` is a bounded navigation view (6
  sections per page, 8 activities and 6 files per source) with `nextPage`,
  `moduleCount`/`fileCount` and truncation flags. Sections include a direct
  Moodle `url` and, when available, the student's groups plus
  `uservisible`/`groupRestricted` flags; `group_only=true` filters with Moodle's
  own `uservisible` and availability group/grouping conditions, never by section
  name. Nested
  `mod_subsection` activities are returned under `subsections` and are searchable.
  Files include `timemodified` when Moodle reports it. `detail=full` reads
  complete metadata text in paginated blocks. Optional `moduleid`/`sectionid`
  select a source; `query` selects matching activities/sections and returns their
  complete text in the same call. Follow `nextPage` for all text. Media and linked resources are
  not implicitly read. Recognized absolute links are listed;
  `richContentOmitted` and the source URL expose the remaining limits.
- `moodle_grades`: with `courseid`, reports each item's Moodle weight
  (`weightraw`/`weightformatted`) and Moodle's own course total. When grade items
  omit weights, the Moodle user report table is consulted; `weightsSource` names
  the source. If both return none, `weightsAvailable=false` and the response says
  the weighted total is unavailable instead of inventing one.
- `moodle_resources`: includes `sectionid` and the section name for every item,
  including resources inside a subsection.
- `moodle_assignment_status`: `assignid` is the assignment instance id; a course
  module id (`cmid`) is also accepted and resolved. Unknown or non-assignment
  modules return a clear id explanation instead of Moodle's raw error.
- `moodle_assignments`: one list call fills dates/status. By default it fetches
  `submissionStatus` only for open assignments and those due within 14 days, with
  a 3-minute cache and a 10-second budget; `full_status=true` checks all.
  `statusesMissing` names omitted entries and how to obtain them.
  `moodle_assignment_status` returns a coherent `status`, `timeSubmitted` only for
  submitted attempts, and `canSubmitReason`.
- `moodle_deadlines`: `include_expired=true` adds items already due within the
  same day window. The response states the exact `windowStart`/`windowEnd`.

- `moodle_assignments`: bounded summary of visible assignments (title, course,
  due/cutoff dates, scheduled submission window). It does not fetch each
  submission; use the returned `id` with `moodle_assignment_status` for
  submission and grading details. Full instructions need
  `moodle_course_content` with `detail=full` and `cmid` as `moduleid`.
- `moodle_file`: `fileurl` from course content may point to a Moodle book
  chapter `index.html`. HTML is returned as the original or as text blocks; it is
  not misreported as a login page. A real sign-in form, an unexpected HTML error,
  an HTTP challenge or a redirect produces a distinct message and is never
  followed or treated as content.
- `moodle_recent_grades`: latest **dated assessment** grades across current
  enrolled courses, or explicit `courseids` (max 30). `limit` defaults to 20.
  Reports per-course failures and counts grades without dates. Zero is a valid
  grade. Totals/categories and hidden grades are excluded. Moodle's grading date
  is not necessarily the publication date. Use `moodle_grades` for undated grades.
- `moodle_course_updates`: standard `core_course_get_updates_since` with
  `courseid` and `since` (Unix seconds). Resolves module names/links and preserves
  the reported change areas and warnings. This is not a complete audit log;
  deleted activities and unsupported plugins can be absent. An unavailable API
  returns an error, not a fabricated empty result.
