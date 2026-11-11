use super::*;
use crate::{
    config::MoodleConfig,
    services::moodle::{tests::server, MoodleClient},
};

fn course() -> Value {
    json!({"id":42,"shortname":"PHS0102","fullname":"Électricité","visible":true})
}

fn contents(description: &str) -> Value {
    json!([{"id":2,"section":1,"name":"Semaine 5", "summary":"Consignes générales",
        "modules":[{"id":7,"name":"TP 5", "modname":"resource", "description":description,
            "contents":[{"type":"file","filename":"support.pdf", "fileurl":"https://school.test/pluginfile.php/42/support.pdf?token=private-token"}]}]}])
}

#[test]
fn finds_late_description_terms_accents_and_explains_which_field_matched() {
    let raw = snapshot(&contents(&format!(
        "{} Conserver le résultat quantique final.",
        "Introduction. ".repeat(3000)
    )))
    .unwrap();
    let hits = search(&course(), &raw, "PHS0102 résultat quantique");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["filename"], "support.pdf");
    assert_eq!(hits[0]["moduleid"], 7);
    assert!(hits[0]["snippet"].as_str().unwrap().contains("quantique"));
    assert!(hits[0]["matchedIn"]
        .as_array()
        .unwrap()
        .contains(&json!("description")));
    assert_eq!(search(&course(), &raw, "resultat quantique").len(), 1);
    assert!(!raw.to_string().contains("private-token"));
}

#[test]
fn title_filters_keep_related_files_and_overview_explicitly_reports_omissions() {
    let raw = snapshot(&contents(&"Consigne complète. ".repeat(100))).unwrap();
    let view = overview(42, &raw, Some("TP5"));
    assert_eq!(
        view["sections"][0]["modules"][0]["files"][0]["filename"],
        "support.pdf"
    );
    assert_eq!(
        view["sections"][0]["modules"][0]["descriptionTruncated"],
        true
    );
    assert_eq!(view["coverage"]["fileContentsRead"], false);
    assert_eq!(view["coverage"]["descriptionsComplete"], false);
}

#[test]
fn full_detail_recovers_every_character_including_a_late_requirement() {
    let description = format!(
        "{} EXIGENCE FINALE : joindre le corrigé.",
        "Énergie λ et équations. ".repeat(2500)
    );
    let raw = snapshot(&contents(&description)).unwrap();
    let mut reconstructed = String::new();
    let mut page = 0;
    loop {
        let response = detail(42, &raw, Some(7), None, page).unwrap();
        for block in response["blocks"].as_array().unwrap() {
            let text = block["text"].as_str().unwrap();
            assert!(text.chars().count() <= 8000);
            reconstructed.push_str(text);
        }
        if let Some(next) = response["nextPage"].as_u64() {
            page = next as usize;
        } else {
            break;
        }
    }
    assert!(page > 0);
    assert_eq!(
        reconstructed,
        format!("{}\n", crate::services::moodle::plain_text(&description))
    );
    assert!(detail(42, &raw, Some(999), None, 0).is_err());
    assert!(detail(42, &raw, Some(7), None, 999).is_err());
}

#[test]
fn td_number_does_not_override_other_query_terms() {
    let raw = snapshot(&json!([{"id":1,"name":"Travaux", "modules":[
        {"id":1,"name":"TD Chapitre 5", "contents":[{"filename":"TD5.pdf","type":"file"}]},
        {"id":2,"name":"Corrigé TD Chapitre 5", "contents":[{"filename":"corrige.pdf","type":"file"}]}]}])).unwrap();
    let found = search(&course(), &raw, "TD5 corrigé");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["moduleid"], 2);
}

#[test]
fn inline_html_and_decoded_math_are_preserved_without_double_html_parsing() {
    let raw = snapshot(&json!([{"id":1,"name":"Cours", "summary":"x &lt; y",
        "modules":[{"id":7,"name":"Analyse", "contents":[{"type":"content","content":"<p>Condition x &lt; y et unicité</p>"}]}]}])).unwrap();
    let view = overview(42, &raw, None);
    assert_eq!(view["sections"][0]["summary"], "x < y");
    assert_eq!(
        view["sections"][0]["modules"][0]["inlineContentAvailable"],
        true
    );
    assert_eq!(search(&course(), &raw, "unicite")[0]["moduleid"], 7);
    assert!(
        detail(42, &raw, Some(7), None, 0).unwrap()["blocks"][0]["text"]
            .as_str()
            .unwrap()
            .contains("x < y")
    );
}

#[test]
fn warm_search_makes_no_school_requests_and_explicit_refresh_replaces_the_snapshot() {
    crate::config::tests::with_temp_config(|| {
        let (url, worker) = server(vec![
            json!({"userid":9}),
            json!([course()]),
            contents("Exigence quantique originale"),
            contents("Exigence quantique actualisée"),
        ]);
        let config = MoodleConfig {
            base_url: url,
            token: "private-token".into(),
            allow_write: false,
        };
        let cold = MoodleClient::new(&config)
            .with_index("a", false)
            .search("PHS0102 quantique", 0, None)
            .unwrap();
        assert_eq!(
            cold["coverage"]["coursesExamined"][0]["index"]["status"],
            "fresh"
        );
        let warm = MoodleClient::new(&config)
            .with_index("a", false)
            .search("PHS0102 quantique", 0, None)
            .unwrap();
        assert_eq!(
            warm["coverage"]["coursesExamined"][0]["index"]["status"],
            "cached"
        );
        assert_eq!(warm["results"], cold["results"]);
        let fresh = MoodleClient::new(&config)
            .with_index("a", true)
            .course_content(42, None)
            .unwrap();
        assert!(fresh["sections"][0]["modules"][0]["description"]
            .as_str()
            .unwrap()
            .contains("actualisée"));
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 4); // cold 3, warm 0, refresh 1
        let after_restart = MoodleClient::new(&config)
            .with_index("a", false)
            .search("PHS0102 quantique", 0, None)
            .unwrap();
        assert!(after_restart["results"][0]["snippet"]
            .as_str()
            .unwrap()
            .contains("actualisée"));
        assert!(MoodleClient::new(&config)
            .with_index("a", true)
            .course_content(42, None)
            .is_err()); // no stale success
    });
}

#[test]
fn a_course_name_does_not_suppress_the_global_fallback_or_its_next_page() {
    crate::config::tests::with_temp_config(|| {
        let (url, worker) = server(vec![
            json!({"userid":9}),
            json!([course()]),
            contents("Introduction"),
            json!({"results":[{"title":"Consigne recherchée","content":"Unique aiguille"}]}),
            json!({"results":[]}),
        ]);
        let config = MoodleConfig {
            base_url: url,
            token: "fixture".into(),
            allow_write: false,
        };
        let client = MoodleClient::new(&config).with_index("a", false);
        let found = client.search("PHS0102 aiguille", 0, None).unwrap();
        assert_eq!(found["results"][0]["title"], "Consigne recherchée");
        assert_eq!(found["coverage"]["globalSearch"]["attempted"], true);
        assert_eq!(found["continuation"]["source"], "global");
        assert_eq!(found["continuation"]["page"], 1);
        let rest = client
            .search_page("PHS0102 aiguille", 1, None, 0, true)
            .unwrap();
        assert_eq!(rest["returned"], 0);
        assert!(rest["continuation"].is_null());
        let requests = worker.join().unwrap();
        assert!(requests[3].contains("wsfunction=core_search_get_results"));
        assert!(requests[4].contains("page=1"));
    });
}

#[test]
fn four_equally_named_courses_are_not_silently_reduced_to_three() {
    let courses = (1..=4)
        .map(|id| json!({"id":id,"shortname":format!("INF{id}"),"fullname":"Informatique"}))
        .collect::<Vec<_>>();
    assert_eq!(
        crate::services::moodle::resolve_search_courses(&courses, "informatique", None).len(),
        4
    );
}

#[test]
fn a_broad_search_visits_all_course_windows() {
    crate::config::tests::with_temp_config(|| {
        let courses = (1..=10)
            .map(|id| json!({"id":id,"shortname":format!("ABC{id}"),"fullname":"Cours"}))
            .collect::<Vec<_>>();
        // Identical course contents make response ordering irrelevant for the two workers.
        let mut responses = vec![json!({"userid":9}), json!(courses)];
        responses.extend((0..10).map(|_| contents("Quantique")));
        let (url, worker) = server(responses);
        let config = MoodleConfig {
            base_url: url,
            token: "fixture".into(),
            allow_write: false,
        };
        let client = MoodleClient::new(&config).with_index("a", false);
        let first = client.search("quantique", 0, None).unwrap();
        assert_eq!(
            first["coverage"]["coursesExamined"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(first["continuation"]["page"], 1);
        let last = client.search("quantique", 1, None).unwrap();
        assert_eq!(
            last["coverage"]["coursesExamined"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(last["continuation"].is_null());
        assert_eq!(worker.join().unwrap().len(), 12);
    });
}

#[test]
fn malformed_lists_fail_instead_of_proving_an_empty_course() {
    assert!(snapshot(&json!({"oops":true})).is_err());
    assert!(snapshot(&json!([{"id":1,"modules":null}])).is_err());
}

#[test]
fn numeric_exercise_numbers_do_not_select_or_disappear_into_a_course_code() {
    let courses = vec![json!({"id":5,"shortname":"INF5","fullname":"Algorithmique"})];
    assert!(crate::services::moodle::resolve_search_courses(&courses, "TP5", None).is_empty());
    let raw = snapshot(&json!([{"id":1,"name":"Travaux","modules":[
        {"id":5,"name":"TP 5"}, {"id":6,"name":"TP 6"}]}]))
    .unwrap();
    let found = search(&courses[0], &raw, "TP5");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["moduleid"], 5);
}

#[test]
fn metadata_result_pagination_does_not_drop_hits_after_forty() {
    crate::config::tests::with_temp_config(|| {
        let modules = (1..=45)
            .map(|id| json!({"id":id,"name":format!("Quantique {id}")}))
            .collect::<Vec<_>>();
        let (url, worker) = server(vec![
            json!({"userid":9}),
            json!([course()]),
            json!([{"id":1,"name":"Cours","modules":modules}]),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "fixture".into(),
            allow_write: false,
        })
        .with_index("a", false);
        let first = client.search("quantique", 0, Some(42)).unwrap();
        assert_eq!(first["returned"], 40);
        assert_eq!(first["continuation"]["offset"], 40);
        let last = client
            .search_page("quantique", 0, Some(42), 40, false)
            .unwrap();
        assert_eq!(last["returned"], 5);
        assert!(last["continuation"].is_null());
        assert_eq!(worker.join().unwrap().len(), 3);
    });
}

#[test]
fn a_failed_course_and_malformed_global_reply_are_reported_as_partial_not_absent() {
    crate::config::tests::with_temp_config(|| {
        let (url, worker) = server(vec![
            json!({"userid":9}),
            json!([course()]),
            json!({"exception":"denied","message":"Not available"}),
            json!({"unexpected":true}),
        ]);
        let client = MoodleClient::new(&MoodleConfig {
            base_url: url,
            token: "fixture".into(),
            allow_write: false,
        })
        .with_index("a", false);
        let found = client.search("quantique", 0, Some(42)).unwrap();
        assert_eq!(found["returned"], 0);
        assert_eq!(found["partial"], true);
        assert_eq!(found["errors"].as_array().unwrap().len(), 2);
        assert_eq!(worker.join().unwrap().len(), 4);
    });
}

#[test]
fn html_lists_and_hexadecimal_equations_do_not_lose_boundaries_or_symbols() {
    assert_eq!(
        crate::services::moodle::plain_text(
            "<p>Étape 1</p><p>Étape 2 : &#x3bb; &lt; 3 &times; 2</p>"
        ),
        "Étape 1 Étape 2 : λ < 3 &times; 2"
    );
}

#[test]
fn mcp_overview_then_full_detail_reuses_the_same_account_snapshot() {
    crate::config::tests::with_temp_config(|| {
        let (url, worker) = server(vec![contents(&format!(
            "{} EXIGENCE FINALE",
            "Énergie. ".repeat(5000)
        ))]);
        let settings = crate::config::Settings {
            moodle: Some(crate::config::MoodleSettings {
                url: Some(url),
                token: Some("private-token".into()),
                allow_write: None,
            }),
            ..Default::default()
        };
        let invoke = |args| {
            let response = crate::mcp::dispatch_tool(&settings, "moodle_course_content", &args);
            assert_eq!(response["isError"], false, "{response}");
            serde_json::from_str::<Value>(response["content"][0]["text"].as_str().unwrap()).unwrap()
        };
        let view = invoke(json!({"courseid":42}));
        assert_eq!(
            view["sections"][0]["modules"][0]["descriptionTruncated"],
            true
        );
        let mut page = 0;
        let mut read = String::new();
        loop {
            let result = invoke(json!({"courseid":42,"detail":"full","moduleid":7,"page":page}));
            assert_eq!(result["index"]["status"], "cached");
            for block in result["blocks"].as_array().unwrap() {
                read.push_str(block["text"].as_str().unwrap());
            }
            if let Some(next) = result["nextPage"].as_u64() {
                page = next;
            } else {
                break;
            }
        }
        assert!(read.contains("EXIGENCE FINALE"));
        assert!(page > 0);
        assert_eq!(worker.join().unwrap().len(), 1);
    });
}

#[test]
fn new_mcp_selectors_are_validated_before_school_requests() {
    let settings = crate::config::Settings {
        moodle: Some(crate::config::MoodleSettings {
            url: Some("http://127.0.0.1:0".into()),
            token: Some("fixture".into()),
            allow_write: None,
        }),
        ..Default::default()
    };
    for args in [
        json!({"courseid":1,"detail":true}),
        json!({"courseid":1,"refresh":"yes"}),
        json!({"courseid":1,"detail":"full","query":"partiel"}),
        json!({"courseid":1,"moduleid":7}),
        json!({"courseid":1,"detail":"full","page":-1}),
    ] {
        let response = crate::mcp::dispatch_tool(&settings, "moodle_course_content", &args);
        assert_eq!(response["isError"], true);
        assert!(!response.to_string().contains("Cannot reach"), "{response}");
    }
}
