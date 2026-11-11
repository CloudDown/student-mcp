//! Clean-install v0.8.0 regressions, using local HTTP fixtures only.
use super::*;

#[test]
fn book_chapter_download_and_study_preserve_html_without_marking_it_viewed() {
    let html = b"<h3>Chapitre 1</h3><p>Lire cette consigne.</p><img src=\"figure.svg\">";
    let (url, worker) = tests::raw_server(vec![(200, "text/html".into(), vec![], html.to_vec())]);
    let client = MoodleClient::new(&MoodleConfig {
        base_url: url.clone(),
        token: "fixture-secret".into(),
        allow_write: false,
    });
    let fileurl = format!("{url}/pluginfile.php/44/mod_book/chapter/7/index.html");
    let original = client
        .download_file(&fileurl, &FileQuery::default())
        .unwrap();
    assert_eq!(original["mimeType"], "text/html");
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(original["file"]["data"].as_str().unwrap())
            .unwrap(),
        html
    );
    let study = client
        .download_file(
            &fileurl,
            &FileQuery {
                format: FileFormat::Text,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(study["text"]
        .as_str()
        .unwrap()
        .contains("Lire cette consigne."));
    assert_eq!(study["textSource"], "html-source");
    assert_eq!(study["coverage"]["faithfulPageImages"], false);
    assert!(!study.to_string().contains("fixture-secret"));
    let requests = worker.join().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "No API view/write call, repeated study reuses download"
    );
    assert!(requests[0].contains(
        "/webservice/pluginfile.php/44/mod_book/chapter/7/index.html?token=fixture-secret"
    ));
}

#[test]
fn file_download_distinguishes_login_unexpected_html_and_challenges() {
    for (status, mime, body, path, expected) in [
        (
            200,
            "text/html",
            "<html><form><input name='password' type='password'></form></html>",
            "/mod_book/chapter/7/index.html",
            "sign-in form",
        ),
        (
            200,
            "text/html",
            "<html><p>School unavailable</p></html>",
            "/mod_resource/content/1/course.pdf",
            "unexpected HTML",
        ),
        (
            202,
            "text/html",
            "",
            "/mod_book/chapter/7/index.html",
            "HTTP 202",
        ),
        (
            302,
            "text/html",
            "",
            "/mod_book/chapter/7/index.html",
            "redirected the file request",
        ),
    ] {
        let (url, worker) = tests::raw_server(vec![(
            status,
            mime.into(),
            vec![],
            body.as_bytes().to_vec(),
        )]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url.clone(),
            token: "secret".into(),
            allow_write: false,
        });
        let error = client
            .download_file(
                &format!("{url}/pluginfile.php/44{path}"),
                &FileQuery::default(),
            )
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(
            !error.contains("Use a fileurl"),
            "The input already came from course content"
        );
        worker.join().unwrap();
    }
}

#[test]
fn assignments_summary_omits_large_bodies_and_preserves_dates_warnings_and_ids() {
    let now = unix_now();
    let (url, worker) = tests::server(vec![
        json!({"courses":[{"id":42,"fullname":"<b>Course</b>","assignments":[
        {"id":1,"cmid":11,"name":"<p>Title</p>","duedate":now + 10,"cutoffdate":now + 20,
            "allowsubmissionsfromdate":now - 1,"intro":"x".repeat(140_000),"configs":[{"value":"noise"}]},
        {"id":2,"cmid":12,"name":"Future","duedate":0,"allowsubmissionsfromdate":now + 99},
        {"id":3,"name":"Closed","cutoffdate":now - 1,"allowsubmissionsfromdate":0},
        {"id":4,"name":"Unknown"}]}],"warnings":[{"warningcode":"courseignored","message":"Some course unavailable"}]}),
    ]);
    let client = MoodleClient::new(&MoodleConfig {
        base_url: url,
        token: "secret".into(),
        allow_write: false,
    });
    let result = client.assignments(Some(&[42])).unwrap();
    assert_eq!(result["count"], 4);
    assert_eq!(result["assignments"][0]["title"], "Title");
    assert_eq!(result["assignments"][0]["course"], "Course");
    assert_eq!(result["assignments"][0]["cmid"], 11);
    assert_eq!(result["assignments"][0]["dueDate"], now + 10);
    assert_eq!(result["assignments"][0]["status"], "open");
    assert!(result["assignments"][0]["submissionStatus"].is_null());
    assert_eq!(result["assignments"][1]["status"], "not-open");
    assert!(result["assignments"][1]["dueDate"].is_null());
    assert_eq!(result["assignments"][2]["status"], "closed");
    assert_eq!(result["assignments"][3]["status"], "unknown");
    assert_eq!(result["partial"], true);
    assert!(result.to_string().len() < 2500);
    let requests = worker.join().unwrap();
    assert_eq!(
        requests.len(),
        1,
        "Summary does not query individual submissions"
    );
    assert!(requests[0].contains("courseids%5B0%5D=42"));
}

#[test]
fn assignments_malformed_reply_is_not_an_empty_success() {
    let (url, worker) = tests::server(vec![json!({"courses":[{"id":42}]})]);
    let client = MoodleClient::new(&MoodleConfig {
        base_url: url,
        token: "fixture".into(),
        allow_write: false,
    });
    assert!(client.assignments(None).is_err());
    worker.join().unwrap();
}
