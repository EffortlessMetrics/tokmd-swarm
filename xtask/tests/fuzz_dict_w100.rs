use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("workspace parent")
        .to_path_buf()
}

#[test]
fn fuzz_dictionaries_do_not_define_empty_tokens() {
    let dict_dir = workspace_root().join("fuzz").join("dict");
    let mut checked = 0usize;

    for entry in fs::read_dir(&dict_dir).expect("read fuzz/dict") {
        let entry = entry.expect("dictionary entry");
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("dict") {
            continue;
        }

        let body = fs::read_to_string(&path).expect("dictionary body");
        for (line_index, raw_line) in body.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let token = if line.starts_with('"') {
                line
            } else {
                line.split_once('=')
                    .map(|(_, value)| value.trim())
                    .unwrap_or(line)
            };

            assert!(
                token.len() >= 2 && token.starts_with('"') && token.ends_with('"'),
                "{}:{}: libFuzzer dictionary entries must be quoted",
                path.display(),
                line_index + 1
            );

            let payload = &token[1..token.len() - 1];
            assert!(
                !payload.is_empty(),
                "{}:{}: empty libFuzzer dictionary entries are rejected; keep empty input coverage in corpus or unit tests",
                path.display(),
                line_index + 1
            );

            checked += 1;
        }
    }

    assert!(checked > 0, "expected at least one fuzz dictionary entry");
}
