# Brightspace student API coverage

Audit date: 2026-10-28. Reference: D2L Developer Platform, October 2026.
This is an implementation backlog, **not a claim that these features work**.
No live student submissions or other mutations were made for this audit.

## Definition of complete

Cover the documented student workflows exposed by Brightspace, including writes,
not just parity with Moodle or a count of tool names. Distinguish:

- Implemented and fixture-tested.
- Implemented and verified on a real tenant with appropriate authorization.
- Documented but not implemented.
- Not available for this tenant/version/role/module.
- No documented public route found.

An API route existing does not prove a student can call it. OAuth scopes,
institutional role permissions, API versions, and enabled modules all matter.
Instructor/admin features are outside this student connector's default scope.

## Implementation progress (first expansion)

The connector now advertises **55 tools** (13 baseline + 42 added), including:

- Discussions: forums, topics, paginated posts, individual post, thread/reply
  creation (plain text), deletion, read/unread status.
- Content: native due items, recent/bookmarked topics, completion count,
  overview, topic details, and course-local TOC title/URL search.
- Grades: own final grade, one item value, visible categories.
- Alerts and course updates; notification channels/subscriptions and own
  subscribe/unsubscribe actions (carrier and message type IDs are GUIDs).
- Groups: categories, category groups, own memberships, available groups,
  own join/leave when institution permissions allow; own sections and classlist.
- Checklist/item definitions; survey definitions.
- Full news item, dismiss/unhide, course pin/unpin, own profile/locale reads.
- API version discovery. New LP 1.64 group discovery routes check support
  before use; baseline routes still use LP 1.49 / LE 1.82.

Each new workflow is exercised through MCP against local HTTP fixtures, including
method/path, body, auth, write gates, identity selection, version support, and
pagination preservation. These tests **do not prove live institutional access**.
No live writes were performed. The permission gate is checked before identity
queries or mutations. Unexpected action responses do not report success;
transport failures report uncertain outcomes rather than invite blind retries.

Still outstanding: discussion edit/attachments/votes/ratings, full assignment
attachment/feedback/rubric access, group/multi-file submissions, native overdue
items, individual content progress writes, calendar writes/detail, notification
settings, profile writes, lockers, awards, ePortfolio, CPD, and outcomes.
The table below records the **original audit baseline**, not current completion.

## Original baseline (before expansion)

13 tools: account status, enrolled courses, course TOC, content-topic file,
grades, assignment list/status/file submission, quiz list/status, calendar,
aggregated deadlines, and news. Student file submission is fixture-tested,
not proven end-to-end on an institutional dropbox. Only content-topic files
are handled by the current Brightspace file tool; that does not cover all
Brightspace attachments.

## Missing or partial student workflows

| Area | Documented capability to investigate/implement | Current gap |
| --- | --- | --- |
| Content | Overview + attachment; module/topic details; recent topics; bookmarked topics; scheduled/due/overdue items; completion counts and own progress | TOC + topic file only; deadlines omit native content due/overdue APIs |
| Search | Connector-side search over accessible content/news/discussions, with clearly bounded coverage | No cross-course search; do not advertise an unverified global Brightspace search endpoint |
| Assignments | Individual/group submissions; source attachments; submitted files; published feedback attachments/media; rubric assessments | Single-file individual submission; feedback/status only partial |
| Discussions | Forums/topics/posts; create thread/reply; edit/delete when permitted; attachments; read status, flag, own vote/rating | Not implemented |
| Grades | Own final grade, individual grade value, categories/schemes/statistics when visible; rubric evaluations | Per-course own grade list only |
| Calendar | Event details and recurring occurrences; create/update/delete where student's permissions permit | Upcoming event lists only; personal calendar write context needs verification |
| News | Item details/attachments; dismiss and restore hidden announcements | Compact per-course list only |
| Alerts/updates | Own alert pages and course update counters | Not implemented |
| Notification preferences | Read channels/subscriptions/settings; subscribe/unsubscribe and update settings when permitted | Not implemented; these are not an email inbox |
| Groups/sections | Own memberships; visible members; available self-enrollment groups; enrollment/withdrawal when role permits | Not implemented; newer self-enrollment discovery route needs LP 1.64+ |
| Classlist | Visible course participants and profiles, subject to privacy permissions | Not implemented |
| Checklists | Checklist/category/item definitions | Not implemented; public checklist CRUD is not proof of student check-off support |
| Quizzes/surveys | Definitions, categories, attempt metadata where permitted | Quiz metadata partial; surveys absent; attempt reads can require elevated permissions |
| Locker | Own/group folder listing, file reading/upload, folder creation/rename and delete | Not implemented; group operations depend on membership and permissions |
| Profile/preferences | Own profile/image, pronouns/visibility, locale settings; course pin/unpin | Status only |
| Awards | Earned/available awards, certificates/PDF, share/unshare own awards on profile | Not implemented; separate BAS product/version |
| ePortfolio | Own/shared objects, file artifacts, reflections, collections, comments/tags, sharing, invites/subscriptions, import/export where supported | Not implemented; separate eP API and tenant module |
| Continuing professional development | Own records, progress, attachments, create/update where permitted | Not implemented; optional module, LE 1.84+ |
| Outcomes/competencies | Student-visible outcomes, objectives, achievement/evaluation information where authorized | Not implemented; distinguish learner views from instructor management |

## Public API boundaries

- The quiz reference has quiz/question authoring and attempt retrieval, but no
  documented learner start-answer-submit workflow was found. Creating quiz
  questions is an instructor action, not taking a quiz.
- Survey attempt retrieval likewise does not establish a public response-submit
  workflow. No learner response submission route was found in this audit.
- No inbox/send-email route was found in the official routing table. Alerts,
  notification preferences, and ePortfolio comments are not private email.
- Content bookmark **reading** is documented. Do not promise bookmark mutations
  without a route. Own progress writes include an `unstable` API; stable
  completion writes have separate permissions and are not universally allowed.
- External LTI tools are separate applications; a Brightspace session/API does
  not automatically expose their data or actions.

Do not bypass an institution's permissions or silently substitute undocumented
browser endpoints for unsupported public API workflows.

## Delivery order and verification gates

1. Foundations: discover supported LE/LP/BAS/eP versions via
   `/d2l/api/versions/`; enforce minimum versions per route; bound pagination,
   downloads, and uploads; distinguish unavailable endpoints from empty results.
   Never convert denied attempt/submission reads into a confident empty status.
2. Learning workflow: native due/overdue content, search, full attachment reading,
   feedback/rubrics, group/multi-file submission, discussion reads and writes.
3. Student organization: alerts/updates, notification settings, groups/sections,
   classlist, checklists, grade detail/final grades, calendar/news actions,
   own content progress where supported.
4. Optional modules/account: locker, profile/preferences, awards/certificates,
   ePortfolio, CPD, and student-visible outcomes/competencies.

For each workflow, specify documented route/version, request schema, pagination,
role/scope requirements, read/write annotations, credential-safe transport,
fixture proof, and live-test status. Client confirmations depend on client
configuration; MCP annotations alone do not guarantee a confirmation dialog.
Retain server-side permission checks, never automatically retry a potentially
accepted non-idempotent write, and report uncertain outcomes rather than success.

Completion requires every in-scope workflow to be implemented and tested or
explicitly classified as unavailable/unsupported with evidence. A generic HTTP
tool alone is not a completed student workflow.

## Official sources

- [Reference and module inventory](https://docs.valence.desire2learn.com/reference.html)
- [Routing table](https://docs.valence.desire2learn.com/http-routingtable.html)
- [Versions](https://docs.valence.desire2learn.com/res/apiprop.html)
- [Content](https://docs.valence.desire2learn.com/res/content.html)
- [Assignments](https://docs.valence.desire2learn.com/res/dropbox.html)
- [Discussions](https://docs.valence.desire2learn.com/res/discuss.html)
- [Grades and assessments](https://docs.valence.desire2learn.com/res/grade.html), [rubrics](https://docs.valence.desire2learn.com/res/assessment.html)
- [Calendar](https://docs.valence.desire2learn.com/res/calendar.html), [news](https://docs.valence.desire2learn.com/res/news.html)
- [Users/alerts/notifications](https://docs.valence.desire2learn.com/res/user.html), [updates](https://docs.valence.desire2learn.com/res/updates.html)
- [Groups](https://docs.valence.desire2learn.com/res/groups.html), [enrollments/classlist](https://docs.valence.desire2learn.com/res/enroll.html)
- [Quizzes](https://docs.valence.desire2learn.com/res/quiz.html), [surveys](https://docs.valence.desire2learn.com/res/survey.html), [checklists](https://docs.valence.desire2learn.com/res/checklist.html)
- [Locker](https://docs.valence.desire2learn.com/res/locker.html), [settings](https://docs.valence.desire2learn.com/res/settings.html)
- [Awards](https://docs.valence.desire2learn.com/res/awards.html), [ePortfolio](https://docs.valence.desire2learn.com/res/epobject.html), [CPD](https://docs.valence.desire2learn.com/res/cpd.html)
