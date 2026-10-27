The generated `functions.json` catalog contains function names, descriptions
and parameter information extracted from Moodle 5.1.6 source code
(Moodle Pty Ltd and contributors). Those extracted portions retain their
original copyright and are offered under the GNU GPL version 3 or later.
Moodle source and license: https://download.moodle.org/releases/latest/
and https://www.gnu.org/licenses/gpl-3.0.html.

The Rust connector source is licensed separately under the MIT license in the
repository root. The embedded catalogue retains GPL-3.0-or-later notices and
licence terms. Run `student-mcp --licenses` to read these notices from an installed
binary. Source is distributed with each GitHub release via its source archive.

`tools/compact-catalog.py` projects a Moodle metadata dump onto the fields used
at runtime, preserving parameter metadata and read/write classifications. Run
`python tools/compact-catalog.py dump.json` to regenerate `functions.json`.
