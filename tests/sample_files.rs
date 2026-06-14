#[test]
fn includes_initial_sample_log_files() {
    let plain = include_str!("../samples/plain.log");
    let json = include_str!("../samples/json.log");

    assert!(plain.contains("INFO"));
    assert!(plain.contains("ERROR"));
    assert!(json.contains("\"level\":\"INFO\""));
    assert!(json.contains("\"source\":\"api\""));
}
