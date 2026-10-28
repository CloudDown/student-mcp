//! Documented student workflows. Routes and query keys are owned by the server,
//! never supplied as URLs by an MCP caller. D2L reference: October 2026.
use super::*;

struct Route {
    name: &'static str,
    description: &'static str,
    method: &'static str,
    path: &'static str,
    query: &'static [&'static str],
}

// Each entry is a dedicated workflow, not an arbitrary HTTP proxy.
fn routes() -> Vec<Route> {
    vec![
        route("api_versions", "Supported API product versions on this institution. Does not prove role permissions.", "GET", "/d2l/api/versions/", &[]),
        route("discussion_forums", "Visible discussion forums in a course.", "GET", "/d2l/api/le/1.82/{courseid}/discussions/forums/", &[]),
        route("discussion_topics", "Visible topics in a discussion forum.", "GET", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/", &[]),
        route("discussion_posts", "One page of discussion posts. pageNumber starts at 1; pageSize defaults to 50 (max 100). Request subsequent pages until empty; no claim of completeness from one page.", "GET", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/{topicid}/posts/", &["pageNumber", "pageSize", "threadId", "threadsOnly"]),
        route("discussion_post", "Read one discussion post.", "GET", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/{topicid}/posts/{postid}", &[]),
        route("discussion_post_create", "Publish a discussion thread or reply. parentpostid selects a reply; omit for a new thread. Plain text only, no attachments or anonymous posts. Group-type topics are not supported by this API. Requires writes enabled; client confirmation depends on client settings.", "POST", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/{topicid}/posts/", &[]),
        route("discussion_post_delete", "Delete a post when the account has permission. Destructive write; do not retry automatically.", "DELETE", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/{topicid}/posts/{postid}", &[]),
        route("discussion_read_status", "Mark a discussion post read or unread (write).", "PUT", "/d2l/api/le/1.82/{courseid}/discussions/forums/{forumid}/topics/{topicid}/posts/{postid}/ReadStatus", &[]),
        route("content_due", "One native Brightspace page of scheduled content still due, excluding exemptions. Use Next/bookmark for subsequent pages.", "GET", "/d2l/api/le/1.82/{courseid}/content/myItems/due/", &["bookmark", "startDateTime", "endDateTime"]),
        route("content_recent", "Recently visited course topics.", "GET", "/d2l/api/le/1.82/{courseid}/content/recent", &[]),
        route("content_bookmarks", "Read bookmarked topics (does not create bookmarks).", "GET", "/d2l/api/le/1.82/{courseid}/content/bookmarks", &[]),
        route("content_completion", "Own aggregate completed/required content count.", "GET", "/d2l/api/le/1.82/{courseid}/content/completions/mycount/", &[]),
        route("course_overview", "Course overview including attachment metadata.", "GET", "/d2l/api/le/1.82/{courseid}/overview", &[]),
        route("content_topic", "Details of one content topic.", "GET", "/d2l/api/le/1.82/{courseid}/content/topics/{topicid}", &[]),
        route("search", "Search titles and URLs in one course's accessible table of contents. Not full-text PDF search or a global Brightspace search. Returns topic/module IDs.", "GET", "/d2l/api/le/1.82/{courseid}/content/toc", &[]),
        route("grade_final", "Own final calculated grade when released by the institution.", "GET", "/d2l/api/le/1.82/{courseid}/grades/final/values/myGradeValue", &[]),
        route("grade_value", "Own grade value for one grade item, including visible feedback.", "GET", "/d2l/api/le/1.82/{courseid}/grades/{gradeid}/values/myGradeValue", &[]),
        route("grade_categories", "Visible grade categories. Permissions may restrict this endpoint.", "GET", "/d2l/api/le/1.82/{courseid}/grades/categories/", &[]),
        route("course_updates", "Own update counters for a course (not an email inbox).", "GET", "/d2l/api/le/1.82/{courseid}/updates/myUpdates", &[]),
        route("alerts", "One page of the connected user's alerts. Preserves paging metadata. Not private email.", "GET", "/d2l/api/lp/1.49/alerts/user/{selfid}", &["bookmark"]),
        route("my_groups", "One page of own group memberships. Requires LP 1.64 (checked before use).", "GET", "/d2l/api/lp/1.64/{courseid}/groups/", &["bookmark"]),
        route("groups_available", "One page of groups available for self-enrollment. Requires LP 1.64; does not enroll the user.", "GET", "/d2l/api/lp/1.64/{courseid}/groups/selfEnrollmentAvailable", &["bookmark"]),
        route("group_categories", "Visible group categories in a course.", "GET", "/d2l/api/lp/1.49/{courseid}/groupcategories/", &[]),
        route("category_groups", "Visible groups in a category, including enrollments where permitted.", "GET", "/d2l/api/lp/1.49/{courseid}/groupcategories/{categoryid}/groups/", &[]),
        route("group_join", "Enroll only the connected user in a group, if institutional role permissions permit. May be refused even when the group is visible. Does not enroll another student.", "POST", "/d2l/api/lp/1.49/{courseid}/groupcategories/{categoryid}/groups/{groupid}/enrollments/", &[]),
        route("group_leave", "Remove only the connected user's group enrollment, if permitted (write).", "DELETE", "/d2l/api/lp/1.49/{courseid}/groupcategories/{categoryid}/groups/{groupid}/enrollments/{selfid}", &[]),
        route("my_sections", "Own section memberships in a course.", "GET", "/d2l/api/lp/1.49/{courseid}/sections/mysections/", &[]),
        route("classlist", "Visible course participants, subject to institutional privacy permissions.", "GET", "/d2l/api/le/1.82/{courseid}/classlist/", &[]),
        route("checklists", "Course checklist definitions (not student check-off state).", "GET", "/d2l/api/le/1.82/{courseid}/checklists/", &[]),
        route("checklist_items", "Item definitions in a checklist; does not mark items complete.", "GET", "/d2l/api/le/1.82/{courseid}/checklists/{checklistid}/items/", &[]),
        route("surveys", "Visible survey definitions. Does not answer or submit surveys.", "GET", "/d2l/api/le/1.82/{courseid}/surveys/", &["bookmark"]),
        route("news_item", "Read one announcement in full, including attachment metadata.", "GET", "/d2l/api/le/1.82/{courseid}/news/{newsid}", &[]),
        route("news_dismiss", "Hide an announcement for the connected user (write).", "POST", "/d2l/api/le/1.82/{courseid}/news/{newsid}/dismiss", &[]),
        route("news_restore", "Unhide a dismissed announcement (not restore a deleted course announcement).", "POST", "/d2l/api/le/1.82/{courseid}/news/{newsid}/restore", &[]),
        route("course_pin", "Pin a course in own enrollments (write).", "POST", "/d2l/api/lp/1.49/enrollments/myenrollments/{courseid}/pin", &[]),
        route("course_unpin", "Unpin a course in own enrollments (write).", "DELETE", "/d2l/api/lp/1.49/enrollments/myenrollments/{courseid}/pin", &[]),
        route("my_profile", "Own personal profile (not account credentials).", "GET", "/d2l/api/lp/1.49/profile/myProfile", &[]),
        route("my_locale", "Own locale account settings.", "GET", "/d2l/api/lp/1.49/accountSettings/mySettings/locale/", &[]),
        route("notification_channels", "Available notification delivery channels.", "GET", "/d2l/api/lp/1.49/notifications/instant/carriers/", &[]),
        route("notification_subscriptions", "Own subscriptions for one notification channel.", "GET", "/d2l/api/lp/1.49/notifications/instant/carriers/{carrierid}/subscriptions/", &[]),
        route("notification_subscribe", "Subscribe the connected account to a message type on a notification channel (write).", "PUT", "/d2l/api/lp/1.49/notifications/instant/carriers/{carrierid}/subscriptions/{messagetypeid}", &[]),
        route("notification_unsubscribe", "Unsubscribe the connected account from a message type on a notification channel (write).", "DELETE", "/d2l/api/lp/1.49/notifications/instant/carriers/{carrierid}/subscriptions/{messagetypeid}", &[]),
    ]
}

fn route(
    name: &'static str,
    description: &'static str,
    method: &'static str,
    path: &'static str,
    query: &'static [&'static str],
) -> Route {
    Route {
        name,
        description,
        method,
        path,
        query,
    }
}

fn parameters(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|s| s.split('}').next())
        .collect()
}

pub(super) fn tools() -> Vec<Value> {
    routes()
        .into_iter()
        .map(|r| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for key in parameters(r.path)
                .into_iter()
                .filter(|key| *key != "selfid")
            {
            let schema = if matches!(key, "carrierid" | "messagetypeid") {
                json!({"type":"string","pattern":"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$"})
            } else { json!({"type":"integer","minimum":1}) };
            properties.insert(key.into(), schema);
                required.push(key);
            }
            for key in r.query {
                let schema = match *key {
                    "pageNumber" | "threadId" => json!({"type":"integer","minimum":1}),
                    "pageSize" => json!({"type":"integer","minimum":1,"maximum":100,"default":50}),
                    "threadsOnly" => json!({"type":"boolean"}),
                    _ => json!({"type":"string","maxLength":2048}),
                };
                properties.insert((*key).into(), schema);
            }
            if r.name == "discussion_post_create" {
                for (key, max) in [("subject", 500), ("message", 20000)] {
                    properties.insert(
                        key.into(),
                        json!({"type":"string","minLength":1,"maxLength":max}),
                    );
                    required.push(key);
                }
                properties.insert("parentpostid".into(), json!({"type":"integer","minimum":1}));
            }
            if r.name == "discussion_read_status" {
                properties.insert("is_read".into(), json!({"type":"boolean"}));
                required.push("is_read");
            }
            if r.name == "search" {
                properties.insert(
                    "query".into(),
                    json!({"type":"string","minLength":1,"maxLength":200}),
                );
                required.push("query");
            }
            json!({"name":format!("brightspace_{}",r.name),"description":r.description,
            "inputSchema":{"type":"object","properties":properties,"required":required},
            "annotations":{"readOnlyHint":r.method=="GET","destructiveHint":r.method!="GET",
                "idempotentHint":matches!(r.method,"GET"|"PUT"|"DELETE"),"openWorldHint":true}})
        })
        .collect()
}

fn id(args: &Value, key: &str) -> Result<i64, String> {
    args.get(key)
        .and_then(Value::as_i64)
        .filter(|v| *v > 0)
        .ok_or_else(|| format!("Provide integer {key} > 0."))
}

fn text(args: &Value, key: &str, max: usize) -> Result<String, String> {
    let value = args
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Provide {key} as text."))?;
    if value.trim().is_empty() || value.chars().count() > max {
        return Err(format!("{key} must contain 1–{max} characters."));
    }
    Ok(value.into())
}

pub(crate) fn dispatch(
    client: &BrightspaceClient,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    let r = routes()
        .into_iter()
        .find(|r| name == format!("brightspace_{}", r.name))
        .ok_or("Unknown Brightspace tool.")?;
    let write = r.method != "GET";
    // Check before ANY file reads, identity queries, version checks or mutations.
    if write && !client.config.allow_write {
        return Err(
            "Brightspace writes are off. Enable Allow assistant changes in the app first.".into(),
        );
    }
    if !args.is_object() {
        return Err("Provide an arguments object.".into());
    }
    let mut path = r.path.to_string();
    for key in parameters(r.path)
        .into_iter()
        .filter(|key| *key != "selfid")
    {
        let value = if matches!(key, "carrierid" | "messagetypeid") {
            let value = args
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Provide {key} as a GUID."))?;
            let valid = value.len() == 36
                && value.bytes().enumerate().all(|(i, b)| {
                    if matches!(i, 8 | 13 | 18 | 23) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                });
            if !valid {
                return Err(format!("Provide {key} as a GUID."));
            }
            value.to_string()
        } else {
            id(args, key)?.to_string()
        };
        path = path.replace(&format!("{{{key}}}"), &value);
    }
    let mut query = Vec::new();
    for key in r.query {
        if let Some(value) = args.get(*key) {
            let encoded = match *key {
                "pageNumber" | "threadId" => id(args, key)?.to_string(),
                "pageSize" => {
                    let n = id(args, key)?;
                    if n > 100 {
                        return Err("pageSize must be <= 100.".into());
                    }
                    n.to_string()
                }
                "threadsOnly" => value
                    .as_bool()
                    .ok_or("threadsOnly must be boolean.")?
                    .to_string(),
                _ => {
                    let s = value
                        .as_str()
                        .ok_or_else(|| format!("{key} must be a string."))?;
                    if s.len() > 2048 {
                        return Err(format!("{key} is too long."));
                    }
                    s.into()
                }
            };
            query.push((*key, encoded));
        }
    }
    if r.name == "discussion_posts" && args.get("pageSize").is_none() {
        query.push(("pageSize", "50".into()));
    }
    let mut body = match r.name {
        "discussion_post_create" => {
            let parent = if args.get("parentpostid").is_some() {
                json!(id(args, "parentpostid")?)
            } else {
                Value::Null
            };
            Some(
                json!({"Subject":text(args,"subject",500)?,"Message":{"Content":text(args,"message",20000)?,"Type":"Text"},"ParentPostId":parent,"IsAnonymous":false}),
            )
        }
        "discussion_read_status" => Some(
            json!({"IsRead":args.get("is_read").and_then(Value::as_bool).ok_or("Provide is_read as a boolean.")?}),
        ),
        _ => None,
    };
    let search = if r.name == "search" {
        Some(text(args, "query", 200)?.to_lowercase())
    } else {
        None
    };
    if path.contains("{selfid}") || r.name == "group_join" {
        let me = client.whoami()?;
        let raw = &me["Identifier"];
        let uid = raw
            .as_str()
            .and_then(|s| s.parse::<i64>().ok())
            .or_else(|| raw.as_i64())
            .filter(|id| *id > 0)
            .ok_or("Brightspace did not return a usable own user ID.")?;
        path = path.replace("{selfid}", &uid.to_string());
        if r.name == "group_join" {
            body = Some(json!({"UserId":uid}));
        }
    }
    if path.starts_with("/d2l/api/lp/1.64/") {
        let supported = client.get("/d2l/api/lp/versions/1.64", &[])?;
        if supported.get("Supported").and_then(Value::as_bool) != Some(true) {
            return Err(
                "This workflow requires Brightspace LP API 1.64, unavailable on this tenant."
                    .into(),
            );
        }
    }
    if !write {
        let data = client.get(&path, &query)?;
        if let Some(needle) = search {
            let mut matches = Vec::new();
            collect_matches(&data, &needle, &mut matches);
            let truncated = matches.len() > 100;
            matches.truncate(100);
            return Ok(
                json!({"matches":matches,"truncated":truncated,"scope":"course TOC titles and URLs only"}),
            );
        }
        return Ok(data);
    }
    let request = client.authorize(
        client
            .agent
            .request(r.method, &format!("{}{path}", client.config.base_url)),
    );
    // No automatic retry: even a lost response can follow an accepted POST.
    let response = match body { Some(body) => request.send_json(body), None => request.call() }
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("Brightspace refused the action (HTTP {code}). Check role permissions, resource availability and session."),
            _ => "Brightspace action outcome is unknown after a transport error. Verify state before retrying.".into(),
        })?;
    if !(200..300).contains(&response.status()) {
        return Err("Brightspace redirected the action; credentials were not forwarded. Verify state before retrying.".into());
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            "Action response could not be read; verify state before retrying.".to_string()
        })?;
    if bytes.len() > 1024 * 1024 {
        return Err("Action response exceeded limit; verify state before retrying.".into());
    }
    if bytes.is_empty() {
        return Ok(json!({"accepted":true}));
    }
    let data: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "Action returned a non-JSON response, not confirmed successful. Verify state before retrying.".to_string())?;
    Ok(crate::moodle::scrub(data, &client.config.token))
}

fn collect_matches(value: &Value, needle: &str, out: &mut Vec<Value>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_matches(value, needle, out);
            }
        }
        Value::Object(object) => {
            if ["Title", "Url"].iter().any(|key| {
                object
                    .get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.to_lowercase().contains(needle))
            }) {
                out.push(json!({"id":object.get("Id"),"title":object.get("Title"),"url":object.get("Url"),"type":object.get("Type")}));
            }
            for child in object.values().filter(|v| v.is_array() || v.is_object()) {
                collect_matches(child, needle, out);
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn server(
        responses: Vec<(u16, &'static str)>,
    ) -> (BrightspaceClient, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = BrightspaceClient::new(BrightspaceConfig {
            base_url: format!("http://{}", listener.local_addr().unwrap()),
            token: "fixture-private".into(),
            allow_write: true,
        });
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut headers = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                    if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap();
                    }
                    headers.push_str(&line);
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                requests.push(headers + &String::from_utf8_lossy(&bytes));
                write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
            requests
        });
        (client, worker)
    }

    fn args() -> Value {
        json!({"courseid":42,"forumid":7,"topicid":8,"postid":9,"gradeid":10,
            "categoryid":11,"checklistid":12,"newsid":13,"carrierid":"11111111-2222-3333-4444-555555555555","groupid":15,"messagetypeid":"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "subject":"Question","message":"My question","is_read":true,"query":"syllabus"})
    }

    #[test]
    fn every_new_route_dispatches_through_mcp_with_expected_method_and_path() {
        let definitions = tools();
        let mut names = std::collections::HashSet::new();
        for r in routes() {
            let name = format!("brightspace_{}", r.name);
            assert!(names.insert(name.clone()));
            let definition = definitions.iter().find(|t| t["name"] == name).unwrap();
            assert_eq!(definition["annotations"]["readOnlyHint"], r.method == "GET");
            let mut replies = Vec::new();
            if r.path.contains("{selfid}") || r.name == "group_join" {
                replies.push((200, r#"{"Identifier":"123"}"#));
            }
            if r.path.starts_with("/d2l/api/lp/1.64/") {
                replies.push((200, r#"{"Supported":true}"#));
            }
            replies.push((
                200,
                if r.method == "GET" {
                    r#"{"Objects":[],"Next":"next","message":"fixture-private"}"#
                } else {
                    ""
                },
            ));
            let (client, worker) = server(replies);
            let settings = crate::config::Settings {
                brightspace: Some(crate::config::BrightspaceSettings {
                    url: Some(client.config.base_url.clone()),
                    token: Some(client.config.token.clone()),
                    allow_write: Some(true),
                }),
                ..Default::default()
            };
            let result = crate::mcp::dispatch_tool(&settings, &name, &args());
            assert_eq!(result["isError"], false, "{name}: {result}");
            assert!(!result.to_string().contains("fixture-private"));
            let requests = worker.join().unwrap();
            let request = requests.last().unwrap();
            let mut expected = r.path.to_string();
            for key in parameters(r.path) {
                let value = if key == "selfid" {
                    "123".into()
                } else if matches!(key, "carrierid" | "messagetypeid") {
                    args()[key].as_str().unwrap().to_string()
                } else {
                    args()[key].as_i64().unwrap().to_string()
                };
                expected = expected.replace(&format!("{{{key}}}"), &value.to_string());
            }
            assert!(
                request.starts_with(&format!("{} {}", r.method, expected)),
                "{name}: {request}"
            );
            assert!(request.contains(session::BROWSER_UA));
            if r.name == "discussion_post_create" {
                assert!(request.contains(r#""Message":{"Content":"My question","Type":"Text"}"#));
                assert!(request.contains(r#""ParentPostId":null"#));
            }
        }
    }

    #[test]
    fn invalid_arguments_and_disabled_writes_never_access_network() {
        let mut client = BrightspaceClient::new(BrightspaceConfig {
            base_url: "http://127.0.0.1:1".into(),
            token: "secret".into(),
            allow_write: false,
        });
        for r in routes().into_iter().filter(|r| r.method != "GET") {
            let error = dispatch(&client, &format!("brightspace_{}", r.name), &args()).unwrap_err();
            assert!(error.contains("writes are off"));
        }
        client.config.allow_write = true;
        for invalid in [
            json!({"courseid":"../evil"}),
            json!({"courseid":0}),
            json!({"courseid":1,"forumid":1,"topicid":1,"pageSize":101}),
            json!({"courseid":1,"forumid":1,"topicid":1,"pageNumber":1.5}),
        ] {
            let error = dispatch(&client, "brightspace_discussion_posts", &invalid).unwrap_err();
            assert!(!error.contains("Cannot reach"));
        }
        let mut invalid = args();
        invalid["message"] = json!("");
        assert!(
            dispatch(&client, "brightspace_discussion_post_create", &invalid)
                .unwrap_err()
                .contains("message")
        );
    }

    #[test]
    fn versions_pagination_own_identity_and_no_false_success() {
        let (client, worker) = server(vec![(200, r#"{"Supported":false}"#)]);
        assert!(dispatch(&client, "brightspace_my_groups", &args())
            .unwrap_err()
            .contains("1.64"));
        assert_eq!(worker.join().unwrap().len(), 1);
        let (client, worker) = server(vec![
            (200, r#"{"Identifier":"123"}"#),
            (200, r#"{"Objects":[],"Next":"more"}"#),
        ]);
        let mut a = args();
        a["bookmark"] = json!("a&evil=1");
        a["selfid"] = json!(999);
        let data = dispatch(&client, "brightspace_alerts", &a).unwrap();
        assert_eq!(data["Next"], "more");
        let requests = worker.join().unwrap();
        assert!(
            requests[1].starts_with("GET /d2l/api/lp/1.49/alerts/user/123?bookmark=a%26evil%3D1 ")
        );
        for (code, body) in [(302, ""), (403, "secret"), (200, "<html>login</html>")] {
            let (client, worker) = server(vec![(code, body)]);
            let error = dispatch(&client, "brightspace_news_dismiss", &args()).unwrap_err();
            assert!(!error.contains("secret"));
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }

    #[test]
    fn search_only_matches_titles_and_urls_without_nested_duplicates() {
        let mut matches = Vec::new();
        collect_matches(
            &json!({"Modules":[{"Id":1,"Title":"Week","Topics":[{"Id":2,"Title":"Syllabus","Url":"/file.pdf"}]}]}),
            "syllabus",
            &mut matches,
        );
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["id"], 2);
    }

    #[test]
    fn group_join_uses_own_identity_and_cookie_writes_include_csrf() {
        let (mut client, worker) = server(vec![(200, r#"{"Identifier":"123"}"#), (200, "")]);
        client.config.token = "d2lSessionVal=sess; d2lSecureSessionVal=secure".into();
        let mut input = args();
        input["userid"] = json!(999);
        dispatch(&client, "brightspace_group_join", &input).unwrap();
        let requests = worker.join().unwrap();
        assert!(requests[1].contains(r#"{"UserId":123}"#));
        assert!(!requests[1].contains("999"));
        let headers = requests[1].to_lowercase();
        assert!(headers.contains("x-csrf-token: sess"));
        assert!(headers.contains("cookie: d2lsessionval=sess"));
        assert!(!headers.contains("authorization:"));
    }

    #[test]
    fn notification_ids_must_be_guids_and_reply_parent_is_validated() {
        let client = BrightspaceClient::new(BrightspaceConfig {
            base_url: "http://127.0.0.1:1".into(),
            token: "secret".into(),
            allow_write: true,
        });
        for bad in [
            json!(14),
            json!("../evil"),
            json!("11111111-2222-3333-4444-55555555555z"),
        ] {
            let mut input = args();
            input["carrierid"] = bad;
            assert!(
                dispatch(&client, "brightspace_notification_subscribe", &input)
                    .unwrap_err()
                    .contains("GUID")
            );
        }
        let mut input = args();
        input["parentpostid"] = json!(-1);
        assert!(
            dispatch(&client, "brightspace_discussion_post_create", &input)
                .unwrap_err()
                .contains("parentpostid")
        );
        let (client, worker) = server(vec![(200, r#"{"PostId":100}"#)]);
        input["parentpostid"] = json!(9);
        dispatch(&client, "brightspace_discussion_post_create", &input).unwrap();
        assert!(worker.join().unwrap()[0].contains(r#""ParentPostId":9"#));
    }
}
