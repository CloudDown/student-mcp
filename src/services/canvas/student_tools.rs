//! Dedicated Canvas student reads, following Brightspace's route catalogue.
//! Paths and fixed query parameters belong to the connector, not the caller.
//! API reference: https://canvas.instructure.com/doc/api/ (November 2026).
use super::*;

struct Route {
    name: &'static str,
    description: &'static str,
    path: &'static str,
    list_key: Option<&'static str>,
    query: &'static [(&'static str, &'static str)],
}

fn routes() -> Vec<Route> {
    vec![
        Route { name: "course", description: "Course details and full syllabus HTML when visible.", path: "/api/v1/courses/{courseid}", list_key: None, query: &[("include[]", "syllabus_body")] },
        Route { name: "modules", description: "Course modules, their order, item counts and own progression. Use canvas_module_items for each module's content; embedded items can be incomplete.", path: "/api/v1/courses/{courseid}/modules", list_key: Some("modules"), query: &[] },
        Route { name: "module_items", description: "Items in one module: pages, assignments, quizzes, files and links. Use content_id with the matching Canvas tool (fileid for canvas_file); page_url with canvas_page.", path: "/api/v1/courses/{courseid}/modules/{moduleid}/items", list_key: Some("items"), query: &[("include[]", "content_details")] },
        Route { name: "pages", description: "List course pages. Pass the url slug or page_id as the page_url argument to canvas_page to read their full body.", path: "/api/v1/courses/{courseid}/pages", list_key: Some("pages"), query: &[] },
        Route { name: "page", description: "Read the full HTML body of a course page. Supply page_url from module items or url from canvas_pages, or the numeric page_id as a string.", path: "/api/v1/courses/{courseid}/pages/{page_url}", list_key: None, query: &[] },
        Route { name: "assignment", description: "Full assignment instructions, rubric, dates and own submission when visible. assignmentid comes from canvas_assignments or module content_id.", path: "/api/v1/courses/{courseid}/assignments/{assignmentid}", list_key: None, query: &[("include[]", "submission")] },
        Route { name: "assignment_status", description: "Own submission, attempts/history, submitted attachments, late/missing state, visible grade, rubric assessment and teacher comments. Does not submit work.", path: "/api/v1/courses/{courseid}/assignments/{assignmentid}/submissions/self", list_key: None, query: &[("include[]", "submission_history"), ("include[]", "submission_comments"), ("include[]", "rubric_assessment")] },
        Route { name: "quizzes", description: "Visible Classic Quizzes: instructions, dates, attempt limits and access state. New Quizzes may appear only as assignments; use canvas_assignments and canvas_assignment_status for those.", path: "/api/v1/courses/{courseid}/quizzes", list_key: Some("quizzes"), query: &[] },
        Route { name: "quiz_status", description: "Own current Classic Quiz submission, attempt number and visible results, with quiz metadata. Does not start a quiz. For previous assignment attempts use canvas_assignment_status with the quiz's assignment_id.", path: "/api/v1/courses/{courseid}/quizzes/{quizid}/submission", list_key: None, query: &[("include[]", "submission"), ("include[]", "quiz")] },
        Route { name: "discussions", description: "Course discussion topics and their full opening messages. Topic IDs also work with canvas_discussion and canvas_discussion_entries.", path: "/api/v1/courses/{courseid}/discussion_topics", list_key: Some("discussions"), query: &[("order_by", "recent_activity")] },
        Route { name: "discussion", description: "Read one discussion or announcement, including its full opening message and access restrictions.", path: "/api/v1/courses/{courseid}/discussion_topics/{topicid}", list_key: None, query: &[] },
        Route { name: "discussion_entries", description: "Top-level posts in a discussion. Embedded recent_replies is only a preview; call canvas_discussion_replies for each entry's replies.", path: "/api/v1/courses/{courseid}/discussion_topics/{topicid}/entries", list_key: Some("entries"), query: &[] },
        Route { name: "discussion_replies", description: "Replies to a discussion entry. Use entryid from canvas_discussion_entries. Canvas enforces initial-post and course access restrictions.", path: "/api/v1/courses/{courseid}/discussion_topics/{topicid}/entries/{entryid}/replies", list_key: Some("replies"), query: &[] },
        Route { name: "conversations", description: "Own Canvas inbox conversations (read and unread by default). Optional scope selects unread, starred, archived or sent.", path: "/api/v1/conversations", list_key: Some("conversations"), query: &[] },
        Route { name: "conversation", description: "Read a Canvas conversation and its messages without marking it read. conversationid comes from canvas_conversations.", path: "/api/v1/conversations/{conversationid}", list_key: None, query: &[("auto_mark_as_read", "false")] },
        Route { name: "todo", description: "Canvas's current to-do feed for the connected user. This is a dashboard feed, not a complete historical list of assignments. Prefer canvas_deadlines for due work.", path: "/api/v1/users/self/todo", list_key: Some("todo"), query: &[] },
    ]
}

fn parameters(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|s| s.split('}').next())
        .collect()
}

pub(super) fn add_paging(tool: &mut Value) {
    tool["inputSchema"]["properties"]["page"] = json!({
        "type":"string", "minLength":1, "maxLength":1024,
        "description":"Canvas page bookmark. Omit for the first page; copy next_page for subsequent pages."
    });
    tool["inputSchema"]["properties"]["per_page"] = json!({
        "type":"integer", "minimum":1, "maximum":100, "default":50
    });
    let description = tool["description"].as_str().unwrap_or_default();
    tool["description"] = json!(format!("{description} Returns one page; follow next_page until null, keeping other arguments unchanged. Content bodies are HTML where supplied by Canvas."));
}

pub(super) fn tools() -> Vec<Value> {
    routes().into_iter().map(|route| {
        let mut properties = serde_json::Map::new();
        let required = parameters(route.path);
        for key in &required {
            properties.insert((*key).into(), if *key == "page_url" {
                json!({"type":"string","minLength":1,"maxLength":255,"description":"Page slug (not a URL), or numeric page_id as a string."})
            } else {
                json!({"type":"integer","minimum":1})
            });
        }
        if route.name == "conversations" {
            properties.insert("scope".into(), json!({"type":"string","enum":["unread","starred","archived","sent"]}));
        }
        let mut tool = json!({"name":format!("canvas_{}",route.name), "description":route.description,
            "inputSchema":{"type":"object","properties":properties,"required":required},
            "annotations":{"readOnlyHint":true}});
        if route.list_key.is_some() { add_paging(&mut tool); }
        tool
    }).collect()
}

pub(crate) fn dispatch(client: &CanvasClient, name: &str, args: &Value) -> Result<Value, String> {
    let route = routes()
        .into_iter()
        .find(|r| name.strip_prefix("canvas_") == Some(r.name))
        .ok_or_else(|| format!("Unknown tool \"{name}\"."))?;
    let mut path = route.path.to_string();
    for key in parameters(route.path) {
        let value = if key == "page_url" {
            let value = args
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| {
                    !s.trim().is_empty() && s.len() <= 255 && !s.chars().any(char::is_control)
                })
                .ok_or(
                    "Provide page_url as a page slug or numeric page_id string (max 255 bytes).",
                )?;
            // A page slug is one path segment, never a caller-controlled path.
            if value == "." || value == ".." || value.contains(['/', '\\', '?', '#', '%']) {
                return Err("Provide a page slug, not a URL or encoded path.".into());
            }
            let mut url = url::Url::parse("https://canvas.invalid/").unwrap();
            url.path_segments_mut().unwrap().push(value);
            url.path().trim_start_matches('/').to_string()
        } else {
            args.get(key)
                .and_then(Value::as_i64)
                .filter(|id| *id > 0)
                .ok_or_else(|| format!("Provide integer {key} > 0."))?
                .to_string()
        };
        path = path.replace(&format!("{{{key}}}"), &value);
    }
    let mut query: Vec<_> = route
        .query
        .iter()
        .map(|(k, v)| (*k, (*v).to_string()))
        .collect();
    if route.name == "conversations" {
        if let Some(scope) = args.get("scope") {
            let scope = scope
                .as_str()
                .filter(|s| matches!(*s, "unread" | "starred" | "archived" | "sent"))
                .ok_or("Provide scope as unread, starred, archived or sent.")?;
            query.push(("scope", scope.into()));
        }
    }
    match route.list_key {
        Some(key) => client.list(&path, &query, &Page::from_args(args)?, key),
        None => client.get(&path, &query),
    }
}
