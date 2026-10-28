# Student documents and Moodle updates

`moodle_file` and `brightspace_file` share the same local document reader.
No university-specific filename or course-code convention is required.

| Format | Reading support |
| --- | --- |
| PDF | Original attachment; text/page search and page images with Poppler |
| Word `.docx`, `.docm` | Main-body text and table cells |
| PowerPoint `.pptx`, `.pptm` | Slide text and table cells in presentation order (package-number fallback if metadata is absent) |
| Excel `.xlsx`, `.xlsm`, `.xls` | Sheet names, row numbers, stored cell values |
| OpenDocument `.odt`, `.odp`, `.ods` | Document text, slides, or sheet cell values |
| Legacy Word `.doc` | Requires local `antiword` |
| Legacy PowerPoint `.ppt` | Requires local `catppt` (catdoc package) |
| Plain text / CSV | Existing text extraction, depending on the supplied MIME type |

Office extraction is text-only, not a visual rendering. Embedded charts, images,
equations, formatting, headers/footers and presenter notes may be absent. Export
to PDF and request page images for visual analysis. Spreadsheet formulas are not
recalculated: cached values can be stale. Macros and external links are not run.
Password-protected documents are not supported.

Office results contain `blocks` of at most 8,000 characters, at most two per call.
`page_from` / `page_to` select **text blocks**, not printed pages or slide numbers.
Each block includes `sourceUnit` (1-based slide/sheet/document index); the result
names the unit type. Follow `nextPageFrom` until `truncated` is false. `find`
locates matching blocks in extracted text. `format=images` requires PDF, not Office.

The existing 12 MiB download ceiling still applies. ZIP-based documents are
checked for at most 4096 entries and 32 MiB declared expanded size. Text, slide,
sheet and cell limits reject oversized results rather than silently omitting them.
These limits are safeguards, not an OS sandbox for file parsers. Keep dependencies
and optional legacy extraction utilities updated.

## Moodle tools

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
