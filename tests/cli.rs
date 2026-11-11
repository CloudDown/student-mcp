use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

#[test]
fn help_lists_canvas_and_settings_discovery() {
    let output = Command::new(env!("CARGO_BIN_EXE_student-mcp"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stderr);
    assert!(help.contains("Canvas"));
    assert!(help.contains("settings-url"));
}

#[test]
fn settings_url_is_read_only_and_checks_the_running_instance() {
    let dir = std::env::temp_dir().join(format!(
        "student-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    let config = dir.join("settings.json");
    let preserved =
        r#"{"accounts":[{"id":"keep","enabled":false}],"gateway":{"public_enabled":false}}"#;
    std::fs::write(&config, preserved).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_student-mcp"))
            .arg("settings-url")
            .env("STUDENT_MCP_CONFIG_FILE", &config)
            .output()
            .unwrap()
    };
    assert!(!run().status.success());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let key = "a".repeat(64);
    std::fs::write(
        config.with_extension("ui.json"),
        serde_json::to_vec(&(port, &key)).unwrap(),
    )
    .unwrap();
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        String::from_utf8(request).unwrap()
    });
    let output = run();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("http://127.0.0.1:{port}/#{key}")
    );
    assert!(worker
        .join()
        .unwrap()
        .contains(&format!("Authorization: Bearer {key}")));
    assert!(
        !run().status.success(),
        "Stale discovery is not a working URL"
    );
    assert_eq!(std::fs::read_to_string(config).unwrap(), preserved);
    std::fs::remove_dir_all(dir).unwrap();
}
