//! Compatibility fingerprints captured before the internal performance refactor.
//! These cover the complete JSON, including order, scores, snippets and coverage.
use super::*;
use sha2::{Digest, Sha256};

fn course() -> Value {
    json!({"id":42,"shortname":"ABC105","fullname":"Cours de démonstration"})
}

fn fingerprint(value: &impl serde::Serialize) -> String {
    Sha256::digest(serde_json::to_vec(value).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn search_fixture() -> Value {
    let texts = [
        "",
        "TD Chapitre 5 corrigé",
        "TD 6 puis TD 5",
        "TD 6 chapitre 5",
        "Équations résultats quantiques",
        "résultat\nλ 中文",
        "TD",
        "5 corrigés",
        "sans correspondance",
        "TD 5 et TD 6",
        "mots répétés répétés",
    ];
    let modules = (0..texts.len())
        .map(|i| {
            let files = (0..i % 4)
                .map(|j| {
                    json!({"type":"file",
            "filename":format!("{}.pdf", texts[(i + j + 3) % texts.len()]),
            "fileurl":format!("https://school.test/{i}/{j}")})
                })
                .collect::<Vec<_>>();
            json!({"id":i + 1,"name":texts[i],"description":texts[(i + 2) % texts.len()],
            "contentText":texts[(i + 5) % texts.len()],"contents":files})
        })
        .collect::<Vec<_>>();
    json!([{"id":1,"name":"TD","summary":"5 consignes", "modules":modules},
        {"id":2,"name":"Équations", "summary":"résultats", "modules":modules}])
}

#[test]
fn search_complete_outputs_match_pre_optimization_fingerprint() {
    let raw = search_fixture();
    let queries = [
        "",
        "le de",
        "ABC105",
        "ABC105 TD5",
        "TD5",
        "TD6",
        "TD5 corrigé",
        "TD5 TD6",
        "corrigés",
        "resultat quantiques",
        "λ",
        "中文",
        "répétés",
        "introuvable",
    ];
    let results = queries
        .iter()
        .map(|query| search(&course(), &raw, query))
        .collect::<Vec<_>>();
    assert_eq!(
        fingerprint(&results),
        "a5c7bf258d2b7d30d05553496277d4286058fb65f02b236e3f6c3fc20fbad77e"
    );
}

#[test]
fn detail_complete_outputs_and_errors_match_pre_optimization_fingerprint() {
    let modules = [0, 1, 7999, 8000, 8001, 31999, 32000, 32001]
        .into_iter()
        .enumerate()
        .map(|(i, length)| {
            json!({"id":i + 1,"name":format!("Activité {i}"),
            "description":"éλ🙂中".chars().cycle().take(length).collect::<String>(),
            "contentText":if i % 2 == 0 { "" } else { "fin" },
            "contents":[{"filename":"document.pdf"}],"links":[],"richContentOmitted":true})
        })
        .collect::<Vec<_>>();
    let raw = json!([{"id":1,"name":"Vide","summary":"","modules":[]},
        {"id":2,"name":"Cours","summary":"Sommaire","modules":modules}]);
    let mut results = Vec::new();
    for section in [None, Some(1), Some(2), Some(99)] {
        for module in [None, Some(1), Some(5), Some(8), Some(99)] {
            for page in [0, 1, 2, 5, 999, usize::MAX] {
                results.push(detail(42, &raw, module, section, page));
            }
        }
    }
    results.push(detail(42, &json!([]), None, None, 0));
    assert_eq!(
        fingerprint(&results),
        "9d78e04dcf56bf08e1edadf4e90ef1af810aa50da56c8801e2e7e1e72f37871b"
    );
}

fn workload(modules: usize, files: usize) -> Value {
    let description = format!(
        "{} résultat quantique",
        "Introduction énergie. ".repeat(800)
    );
    let modules = (0..modules)
        .map(|id| {
            json!({"id":id + 1,"name":"TD Chapitre 5",
        "description":description,"contentText":"Suite des consignes",
        "contents":(0..files).map(|file| json!({"type":"file","filename":format!("TD5-{file}.pdf"),
            "fileurl":format!("https://school.test/{id}/{file}")})).collect::<Vec<_>>()})
        })
        .collect::<Vec<_>>();
    json!([{"id":1,"name":"Cours","summary":"Consignes","modules":modules}])
}

#[test]
#[ignore = "local CPU benchmark; run explicitly with --ignored --nocapture --test-threads=1"]
fn benchmark_metadata_cpu() {
    use std::{hint::black_box, time::Instant};
    fn measure(label: &str, mut run: impl FnMut()) {
        run();
        let mut samples = Vec::new();
        for _ in 0..5 {
            let start = Instant::now();
            for _ in 0..3 {
                run();
            }
            samples.push(start.elapsed().as_secs_f64() * 1000.0 / 3.0);
        }
        samples.sort_by(f64::total_cmp);
        eprintln!(
            "{label}: median {:.3} ms/call (5 samples of 3, no network)",
            samples[2]
        );
    }
    let raw = workload(8, 16);
    measure("search_many_attachments", || {
        black_box(search(&course(), black_box(&raw), "TD5 résultat quantique"));
    });
    let raw = workload(120, 16);
    measure("detail_first_window", || {
        black_box(detail(42, black_box(&raw), None, None, 0).unwrap());
    });
    measure("detail_late_window", || {
        black_box(detail(42, black_box(&raw), None, None, 70).unwrap());
    });
}
