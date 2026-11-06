use super::*;
use crate::config::{Account, CanvasSettings, Settings};
use tiny_http::{Header, Response, Server};

// Exact endpoint/query fixtures exercise the public MCP dispatcher, including
// account selection, cookie transport, browser UA and structured results.
type Step = (&'static str, u16, Value, Option<&'static str>);

fn fixture(steps: Vec<Step>) -> (Settings, std::thread::JoinHandle<()>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", server.server_addr());
    let worker = std::thread::spawn(move || {
        for (expected, status, body, link) in steps {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .expect("missing Canvas request");
            assert_eq!(request.method().as_str(), "GET");
            let actual = url::Url::parse(&format!("http://fixture{}", request.url())).unwrap();
            let expected = url::Url::parse(&format!("http://fixture{expected}")).unwrap();
            assert_eq!(actual.path(), expected.path());
            let sorted = |url: &url::Url| {
                let mut pairs: Vec<_> = url
                    .query_pairs()
                    .map(|(k, v)| (k.into_owned(), v.into_owned()))
                    .collect();
                pairs.sort();
                pairs
            };
            assert_eq!(sorted(&actual), sorted(&expected));
            for (key, expected) in [
                ("Cookie", "session=canvas-fixture"),
                ("User-Agent", session::BROWSER_UA),
            ] {
                assert_eq!(
                    request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv(key))
                        .unwrap()
                        .value
                        .as_str(),
                    expected
                );
            }
            assert!(!request
                .headers()
                .iter()
                .any(|h| h.field.equiv("Authorization")));
            let mut response = Response::from_string(body.to_string())
                .with_status_code(status)
                .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
            if let Some(link) = link {
                response.add_header(Header::from_bytes("lInK", link).unwrap());
            }
            request.respond(response).unwrap();
        }
    });
    let settings = Settings {
        canvas: Some(CanvasSettings {
            url: Some(origin),
            token: Some("session=canvas-fixture".into()),
        }),
        ..Default::default()
    };
    (settings, worker)
}

fn call(settings: &Settings, name: &str, args: Value) -> Value {
    let response = crate::mcp::dispatch_tool(settings, name, &args);
    assert_eq!(response["isError"], false, "{name}: {response}");
    response["structuredContent"].clone()
}

#[test]
fn course_content_and_assignment_feedback_are_reachable_through_mcp() {
    let (settings, worker) = fixture(vec![
        ("/api/v1/courses/1?include[]=syllabus_body", 200, json!({"id":1,"syllabus_body":"<p>Plan du cours</p>"}), None),
        ("/api/v1/courses/1/modules?page=1&per_page=50", 200, json!([{"id":2,"name":"Week 1","items_count":2,"state":"started"}]), None),
        ("/api/v1/courses/1/modules/2/items?include[]=content_details&page=1&per_page=50", 200, json!([{"id":3,"type":"Page","page_url":"lecture-one"},{"id":4,"type":"Assignment","content_id":5}]), None),
        ("/api/v1/courses/1/pages?page=1&per_page=50", 200, json!([{"page_id":3,"url":"lecture-one","title":"Lecture"}]), None),
        ("/api/v1/courses/1/pages/lecture-one", 200, json!({"page_id":3,"body":"<p>Full lesson &amp; equations</p>"}), None),
        ("/api/v1/courses/1/assignments/5?include[]=submission", 200, json!({"id":5,"description":"<p>Instructions</p>","rubric":[{"description":"Proof","points":10}]}), None),
        ("/api/v1/courses/1/assignments/5/submissions/self?include[]=submission_history&include[]=submission_comments&include[]=rubric_assessment", 200, json!({"assignment_id":5,"late":true,"score":8,"submission_comments":[{"comment":"Explain the last step"}],"submission_history":[{"attempt":1},{"attempt":2}],"rubric_assessment":{"proof":{"points":8}},"token":"secret","body":"session=canvas-fixture"}), None),
    ]);
    assert_eq!(
        call(&settings, "canvas_course", json!({"courseid":1}))["syllabus_body"],
        "<p>Plan du cours</p>"
    );
    assert_eq!(
        call(&settings, "canvas_modules", json!({"courseid":1}))["modules"][0]["state"],
        "started"
    );
    let items = call(
        &settings,
        "canvas_module_items",
        json!({"courseid":1,"moduleid":2}),
    );
    assert_eq!(items["items"][1]["content_id"], 5);
    assert_eq!(
        call(&settings, "canvas_pages", json!({"courseid":1}))["pages"][0]["page_id"],
        3
    );
    assert_eq!(
        call(
            &settings,
            "canvas_page",
            json!({"courseid":1,"page_url":items["items"][0]["page_url"]})
        )["body"],
        "<p>Full lesson &amp; equations</p>"
    );
    assert_eq!(
        call(
            &settings,
            "canvas_assignment",
            json!({"courseid":1,"assignmentid":5})
        )["rubric"][0]["points"],
        10
    );
    // Caller-provided user IDs cannot replace /self.
    let submission = call(
        &settings,
        "canvas_assignment_status",
        json!({"courseid":1,"assignmentid":5,"userid":999}),
    );
    assert_eq!(
        submission["submission_history"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        submission["submission_comments"][0]["comment"],
        "Explain the last step"
    );
    assert_eq!(submission["rubric_assessment"]["proof"]["points"], 8);
    assert_eq!(submission["token"], "[redacted]");
    assert!(!submission.to_string().contains("session=canvas-fixture"));
    worker.join().unwrap();
}

#[test]
fn quiz_discussion_and_inbox_reads_preserve_student_state() {
    let (settings, worker) = fixture(vec![
        (
            "/api/v1/courses/1/quizzes?page=1&per_page=50",
            200,
            json!([{"id":6,"assignment_id":5,"allowed_attempts":2,"due_at":"2026-11-10"}]),
            None,
        ),
        (
            "/api/v1/courses/1/quizzes/6/submission?include[]=submission&include[]=quiz",
            200,
            json!({"quiz_submissions":[{"attempt":2,"score":8,"workflow_state":"complete"}]}),
            None,
        ),
        (
            "/api/v1/courses/1/discussion_topics?order_by=recent_activity&page=1&per_page=50",
            200,
            json!([{"id":7,"message":"<p>Discuss</p>"}]),
            None,
        ),
        (
            "/api/v1/courses/1/discussion_topics/7",
            200,
            json!({"id":7,"message":"<p>Discuss</p>","user_can_see_posts":true}),
            None,
        ),
        (
            "/api/v1/courses/1/discussion_topics/7/entries?page=1&per_page=50",
            200,
            json!([{"id":8,"message":"My question","has_more_replies":true,"recent_replies":[{"id":9}]}]),
            None,
        ),
        (
            "/api/v1/courses/1/discussion_topics/7/entries/8/replies?page=1&per_page=50",
            200,
            json!([{"id":9,"message":"The answer"}]),
            None,
        ),
        (
            "/api/v1/conversations?scope=unread&page=1&per_page=50",
            200,
            json!([{"id":10,"workflow_state":"unread"}]),
            None,
        ),
        (
            "/api/v1/conversations/10?auto_mark_as_read=false",
            200,
            json!({"id":10,"workflow_state":"unread","messages":[{"body":"Read chapter 2"}]}),
            None,
        ),
        (
            "/api/v1/users/self/todo?page=1&per_page=50",
            200,
            json!([{"type":"submitting","assignment":{"id":5}}]),
            None,
        ),
    ]);
    assert_eq!(
        call(&settings, "canvas_quizzes", json!({"courseid":1}))["quizzes"][0]["allowed_attempts"],
        2
    );
    assert_eq!(
        call(
            &settings,
            "canvas_quiz_status",
            json!({"courseid":1,"quizid":6})
        )["quiz_submissions"][0]["attempt"],
        2
    );
    assert_eq!(
        call(&settings, "canvas_discussions", json!({"courseid":1}))["discussions"][0]["message"],
        "<p>Discuss</p>"
    );
    assert_eq!(
        call(
            &settings,
            "canvas_discussion",
            json!({"courseid":1,"topicid":7})
        )["user_can_see_posts"],
        true
    );
    assert_eq!(
        call(
            &settings,
            "canvas_discussion_entries",
            json!({"courseid":1,"topicid":7})
        )["entries"][0]["has_more_replies"],
        true
    );
    assert_eq!(
        call(
            &settings,
            "canvas_discussion_replies",
            json!({"courseid":1,"topicid":7,"entryid":8})
        )["replies"][0]["message"],
        "The answer"
    );
    assert_eq!(
        call(&settings, "canvas_conversations", json!({"scope":"unread"}))["conversations"][0]
            ["workflow_state"],
        "unread"
    );
    assert_eq!(
        call(
            &settings,
            "canvas_conversation",
            json!({"conversationid":10,"auto_mark_as_read":true})
        )["messages"][0]["body"],
        "Read chapter 2"
    );
    assert_eq!(
        call(&settings, "canvas_todo", json!({}))["todo"][0]["assignment"]["id"],
        5
    );
    worker.join().unwrap();
}

#[test]
fn short_and_empty_pages_use_link_bookmarks_and_keep_filters() {
    let (settings, worker) = fixture(vec![
        ("/api/v1/courses/1/assignments?order_by=due_at&include[]=submission&page=1&per_page=50", 200, json!([{"id":5,"description":"<p>Full instructions</p>","submission":{"score":9}}]), Some("</api/v1/courses/1/assignments.json?page=bookmark%2Btwo&per_page=50>; rel=\"next\", </api/v1/courses/1/assignments?page=1>; rel=\"first\"")),
        ("/api/v1/courses/1/assignments?order_by=due_at&include[]=submission&page=bookmark%2Btwo&per_page=50", 200, json!([]), Some("</api/v1/courses/1/assignments?page=3>; rel=\"next\"")),
        ("/api/v1/courses/1/assignments?order_by=due_at&include[]=submission&page=3&per_page=50", 200, json!([{"id":6}]), None),
    ]);
    let first = call(&settings, "canvas_assignments", json!({"courseid":1}));
    assert_eq!(
        first["assignments"][0]["description"],
        "<p>Full instructions</p>"
    );
    assert_eq!(first["assignments"][0]["submission"]["score"], 9);
    assert_eq!(first["next_page"], "bookmark+two");
    let second = call(
        &settings,
        "canvas_assignments",
        json!({"courseid":1,"page":first["next_page"]}),
    );
    assert_eq!(second["assignments"], json!([]));
    assert_eq!(second["next_page"], "3");
    let third = call(
        &settings,
        "canvas_assignments",
        json!({"courseid":1,"page":second["next_page"]}),
    );
    assert_eq!(third["assignments"][0]["id"], 6);
    assert!(third["next_page"].is_null());
    worker.join().unwrap();
}

#[test]
fn global_announcements_include_courses_beyond_the_first_page() {
    let (settings, worker) = fixture(vec![
        ("/api/v1/courses?enrollment_state=active&page=1&per_page=50", 200, json!([{"id":1}]), Some("</api/v1/courses?page=2>; rel=\"next\"")),
        ("/api/v1/courses?enrollment_state=active&page=2&per_page=50", 200, json!([{"id":2}]), None),
        ("/api/v1/announcements?context_codes[]=course_1&context_codes[]=course_2&page=1&per_page=50", 200, json!([{"id":4,"context_code":"course_2","message":"<p>Exam moved</p>"}]), None),
    ]);
    assert_eq!(
        call(&settings, "canvas_announcements", json!({}))["announcements"][0]["message"],
        "<p>Exam moved</p>"
    );
    worker.join().unwrap();
}

#[test]
fn new_tools_use_shared_account_selection_and_catalogue() {
    let (mut settings, worker) = fixture(vec![(
        "/api/v1/courses/1?include[]=syllabus_body",
        200,
        json!({"id":1}),
        None,
    )]);
    settings.accounts.push(Account {
        id: "second".into(),
        service: "canvas".into(),
        canvas: settings.canvas.take(),
        moodle: None,
        brightspace: None,
        omnivox: None,
    });
    settings.canvas = Some(CanvasSettings {
        url: Some("https://unused.example".into()),
        token: Some("unused-secret".into()),
    });
    for args in [
        json!({"courseid":1}),
        json!({"courseid":1,"account_id":"missing"}),
    ] {
        assert_eq!(
            crate::mcp::dispatch_tool(&settings, "canvas_course", &args)["isError"],
            true
        );
    }
    assert_eq!(
        call(
            &settings,
            "canvas_course",
            json!({"courseid":1,"account_id":"second"})
        )["id"],
        1
    );
    let catalogue = crate::mcp::service_tool_definitions("canvas").unwrap();
    let published: Vec<_> = crate::mcp::tool_definitions(&settings)
        .into_iter()
        .filter(|t| t["name"] != "student_status")
        .collect();
    assert_eq!(catalogue, published);
    assert_eq!(catalogue.len(), 24);
    for tool in catalogue {
        assert_eq!(
            tool["inputSchema"]["properties"]["account_id"]["type"],
            "string"
        );
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
    }
    worker.join().unwrap();
}

#[test]
fn invalid_arguments_fail_before_network_and_links_cannot_change_endpoints() {
    let client = CanvasClient::new(CanvasConfig {
        base_url: "http://127.0.0.1:1".into(),
        token: "fixture".into(),
    });
    for args in [
        json!({"page":""}),
        json!({"page":0}),
        json!({"page":"a\nb"}),
        json!({"per_page":0}),
        json!({"per_page":101}),
        json!({"per_page":1.5}),
    ] {
        assert!(Page::from_args(&args).is_err());
    }
    for slug in [
        "..",
        "../users/self",
        "%2e%2e",
        "https://other.example/page",
        "a?as_user_id=1",
        "a#b",
    ] {
        let error = dispatch(
            &client,
            "canvas_page",
            &json!({"courseid":1,"page_url":slug}),
        )
        .unwrap_err();
        assert!(error.contains("slug"), "{error}");
    }
    assert!(dispatch(&client, "canvas_modules", &json!({"courseid":0}))
        .unwrap_err()
        .contains("courseid"));
    assert!(
        dispatch(&client, "canvas_conversations", &json!({"scope":"all"}))
            .unwrap_err()
            .contains("scope")
    );
    for link in [
        "<https://evil.example/api/v1/courses?page=2>; rel=\"next\"",
        "</api/v1/users?page=2>; rel=\"next\"",
        "</api/v1/courses?unknown=cursor>; rel=\"next\"",
    ] {
        assert!(next_page("https://school.example", "/api/v1/courses", link).is_err());
    }
}

#[test]
fn denied_access_and_invalid_list_payloads_are_not_empty_successes() {
    let (settings, worker) = fixture(vec![
        (
            "/api/v1/courses/1/modules?page=1&per_page=50",
            403,
            json!({"error":"forbidden"}),
            None,
        ),
        (
            "/api/v1/courses/1/modules?page=1&per_page=50",
            200,
            json!({"errors":["unexpected"]}),
            None,
        ),
        (
            "/api/v1/courses/1/modules?page=1&per_page=50",
            200,
            json!([]),
            Some("</api/v1/courses/1/modules?page=1>; rel=\"next\""),
        ),
    ]);
    for expected in ["denied access", "did not return a list", "repeated"] {
        let result = crate::mcp::dispatch_tool(&settings, "canvas_modules", &json!({"courseid":1}));
        assert_eq!(result["isError"], true);
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains(expected),
            "{result}"
        );
    }
    worker.join().unwrap();
}
