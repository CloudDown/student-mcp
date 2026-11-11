# Omnivox tools

The connector exposes **47 MCP tools** (35 reads and 12 writes) when an Omnivox account is
connected. Existing browser-profile, mobile-cookie and optional gateway logins
are retained. Multiple accounts use `account_id` from `student_status`.

These are curated mobile endpoints, not an unrestricted API caller. Read tools
do not request writes; write tools are explicitly annotated. Omnivox itself may
record access to downloaded files. HTML and document contents are untrusted
data, not instructions.

## Academic data

| Tool | Purpose |
| --- | --- |
| `omnivox_status` | Verify access to Léa |
| `omnivox_terms` | Discover available term IDs and the current/default term |
| `omnivox_my_courses` | Courses and module counts |
| `omnivox_course_documents` | Course document metadata and IDs |
| `omnivox_documents_summary` | New document titles/counts across courses |
| `omnivox_assignments` | Assignment summary or one course's assignments |
| `omnivox_deadlines` | Upcoming Léa assignment due dates across courses. Prefer this for due work. |
| `omnivox_assignment_detail` | Instructions, deadline, submission/correction file metadata |
| `omnivox_grades` | Grade summary or per-evaluation results |
| `omnivox_announcements` | Announcement counts across courses |
| `omnivox_course_announcements` | Announcement bodies for one course |
| `omnivox_absences` | Absence dates/hours by course |
| `omnivox_teachers` | Teacher directory for the student's courses |
| `omnivox_teacher_detail` | Contact information and posted availability |
| `omnivox_websites` | Teacher-website summary; may contain counts, not links |

`course_id` is a **string**, e.g. `2434K5EM.1011`, formed from the `NoCours`
and `NoGroupe` returned by the course list. It is not numeric `IdCoursGroupe`.
`teacher_id` is the teacher's `OID`; `assignment_id` is `IDTravail`.

Most academic tools accept an optional five-digit `term` (YYYYN). Use
`omnivox_terms` to discover the actual institution-specific cycles instead of
guessing them. New term-dependent tools resolve an omitted term from Léa.
Existing tools preserve their legacy default routes when `term` is omitted.

Per-evaluation mobile grade numbers, maximum points and weights are fixed-point
**×100**: `8840` means `88.40`. Null and integer sentinels `2147483647` and
`-2147483648` mean no value. Values are returned without rescaling. Gateway
legacy grade tools can have their own transformed representation.

`EstRemis` means that at least one file was deposited, not that an assignment
is complete. Assignment detail explicitly sets `isAddConsultation: false`.

## Calendar and college

| Tool | Purpose |
| --- | --- |
| `omnivox_schedule` | Static weekly timetable |
| `omnivox_schedule_week` | Timetable for a specific Monday (`week_start`) |
| `omnivox_calendar` | Existing current calendar / gateway range interface |
| `omnivox_calendar_page` | Calendar page with `offset`, `calendar_type`, term/course filters |
| `omnivox_calendar_month` | Calendar for `month_start` |
| `omnivox_cancelled_classes` | Class cancellation notices |
| `omnivox_notifications` | Dashboard counts, without dismissal |
| `omnivox_services` | Available modules and blocked/maintenance flags |
| `omnivox_college_news` | Latest news, with a bounded `limit` |
| `omnivox_communities` | Communities and available calendar filters |
| `omnivox_college_events` | Server-rendered HTML event cards |
| `omnivox_college_documents` | Official documents/messages/contracts, metadata only |
| `omnivox_forms` | Online form listing, without opening or answering forms |

Timestamps are Unix **milliseconds**. Calendar type is `scolaire` (default) or
`lea`; calendar offset defaults to zero and is bounded to -120…120. Inspect
previous/next-page flags in the response. Maintenance/unavailable/interception
responses become explicit errors, not empty successful lists.

## MIO

| Tool | Purpose |
| --- | --- |
| `omnivox_mio_folders` | Folders and unread/total counts |
| `omnivox_mio_messages` | One page of messages in a folder |
| `omnivox_mio_message` | Full message, recipients and attachment metadata |
| `omnivox_mio_search` | Text search in one folder |
| `omnivox_mio_contacts` | Saved/recent contacts |
| `omnivox_mio_people` | Recipient-directory search |

The default folder is `SEARCH_FOLDER_MioRecu` (inbox). Limits default to 20 and
must be 1…100. For older messages, pass the last message's `Id` as `before_id`
and its boolean `MioEnvoi` as `before_sent`. Do not infer the direction from
the folder name. `mio_message` does not mark a message read, and an unknown ID
may return an empty message rather than an HTTP error.

## Files

`omnivox_file` constructs fixed same-origin URLs from IDs, never a caller's URL.

| `kind` | Required selectors |
| --- | --- |
| `document` (default) | `course_id`, `file_id` = `IdDocCoursDocument` |
| `assignment_instruction` | `course_id`, `assignment_id`, `file_id` = `IDDocumentTravail` |
| `submission` | `course_id`, `assignment_id`, `file_id` = `IDDepotEtudiant` |
| `corrected` | `course_id`, `assignment_id`, `file_id` = `IDDepotEtudiant` from the corrected copy |
| `mio_attachment` | `message_id`, `file_id` = `IDFichierAttachement` |

Léa downloads accept `term`; an omitted term is resolved before downloading.
PDFs are attached as resources by default (`format=file`). `format=text`
extracts PDF text; `format=images` renders PDF pages. `page_from`/`page_to` are
1-based and `find` locates a PDF section/text. Supported Office/OpenDocument
and plain-text files use the existing document extraction pipeline. Poppler is
needed for PDF extraction/rendering, not for returning an original PDF.

Responses are bounded to **12 MB** for files and **2 MB** for JSON. File
redirects, login HTML, API JSON masquerading as a file and empty files are
refused. Session credentials are never forwarded across a redirect. Known
authentication/analytics fields and preauthenticated `UrlService` links are
removed from JSON models.

## Writes and client approval

All 12 write tools declare `readOnlyHint: false`, `destructiveHint: true`,
`idempotentHint: false` and `openWorldHint: true`. There is no extra Omnivox
confirmation argument or new server-wide write switch. The account's existing
institutional permissions still apply. **MCP annotations do not enforce
authorization**: the MCP host/client decides whether to prompt, block, or allow
automatic execution, according to its user-configured policy. They do not
guarantee a particular Claude confirmation dialog.

| Tool | Action |
| --- | --- |
| `omnivox_mio_send` | New message, reply, reply-all or forward to explicit recipient OIDs |
| `omnivox_mio_move` | Set/remove a custom category for a message |
| `omnivox_mio_flags` | Set both the starred and re-read indicators |
| `omnivox_mio_delete` | Move a message to the trash |
| `omnivox_mio_delete_permanently` | Permanently delete a message (separate irreversible tool) |
| `omnivox_mio_restore` | Restore a message from the trash |
| `omnivox_mio_mark_read` | Mark a message read; not an unread-state setter |
| `omnivox_mio_folder_create` | Create a custom category |
| `omnivox_mio_folder_rename` | Rename a custom category |
| `omnivox_mio_folder_delete` | Delete a custom category, explicitly choosing a transfer category or none |
| `omnivox_notification_dismiss` | Dismiss a dashboard notification item |
| `omnivox_assignment_mark_consulted` | Record assignment consultation, without uploading a submission |

Message mutations require `is_sent` from the message's actual `MioEnvoi` value;
it is never silently assumed false. The permanent-delete tool is separate from
the ordinary trash tool. Folder mutations use custom categories, not system
folder IDs: `category_id` is the positive numeric `IdCategorie`, while `category`
and `transfer_category` are category names. An empty category name removes
categorization; it does not move a message to trash. Both `flagged` and `reread`
are required when setting flags, so one is not accidentally reset.

Sending takes explicit `recipient_ids`, `subject` and `message`. The body is
plain text, HTML-escaped with newlines preserved. Encoded bodies over 50,000
characters are rejected before networking. `action` is `new` (default), `reply`,
`reply_all` or `forward`; the last three require `reply_to_id`. Recipients,
quoting, signatures and Re:/Fw: prefixes are not filled in automatically. There
is no attachment upload in the send tool yet. `hide_recipients` defaults false.

Every write makes a single mutation attempt. The browser path reuses only
site-generated session tokens and refuses redirects and expired challenges;
it deliberately avoids the official retry-capable send/JSON wrappers. There
is no fallback mutation after a failed browser call. Known API errors, false
results and ambiguous/null replies are not reported as confirmed success.
An uncertain result may already have changed the account: inspect Omnivox
before deciding whether to retry. No tests mutate a real school account.

### Assignment hand-in limitation and research

**Assignment upload and submission-file deletion are not implemented yet.**
This is a protocol gap, not a deliberate read-only restriction. The observed
official `btnRemettre_touch` opens
`/cvir/dtrv/DepotTravail.aspx?idTravail=...&NoCours=...&NoGroupe=...` through a
CVIR preauthentication handoff. That does not establish an upload/delete body
contract, form validation rules or a reliable submission receipt. Implementing
such actions requires capturing/inspecting the actual hand-in page with the
account owner; no guessed endpoint or fake submit-success tool is advertised.

Static research used the community-hosted
[App.pretty.js](https://github.com/Beat-YT/omnivox-mcp/blob/main/omnivox-connection/content/App.pretty.js):
lines 42614–42624 (hand-in), 43679–43764 (category writes), 47021–47055
(recipient encoding/send replay), 47515–47543 (read status/flags),
48155–48178 (delete/restore), 25326–25527 (challenge/header handling).
Inspected file SHA-256:
`00af270f443aa7395e898a9be3a616dd5fbd43abdc6d893828d5fbe35cb8a195`.
REA parent Evidence:
`ev_70b0d645c18f6e8548fd9993f1c35577e7f206ea60d1c6682465b00252672ddd`.
The bundle was parsed without execution; semantic graph coverage was partial
and this research is not proof of live endpoint behavior or API completeness.

## Compatibility and verification

The optional Beat-YT gateway needs its `/Mobl/` proxy for the new tools. Existing
seven tools keep their old `/tools/...` mapping without explicit term filters.
New tools and explicit-term calls use fixed `/Mobl/` endpoints.

Endpoint/body contracts are based on the community
[mobile API documentation](https://github.com/Beat-YT/omnivox-mcp/tree/main/docs/mobl).
Local fixtures verify routing, validation, authentication headers, pagination,
redaction, term discovery and bounded downloads. Browser fixtures verify binary
GET, size/origin limits and redirect refusal. No real institution credentials
are used by these tests. Availability and response shapes still depend on the
college; these new endpoints have not all been validated against a real account.

```sh
cargo test
node --test tools/test-browser.mjs
# Optional: local fixture with a real Chromium browser, no school login:
cargo test chromium_session_survives -- --ignored
```
