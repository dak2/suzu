use std::fs;
use std::path::Path;

#[test]
fn examples_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "sz"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no examples found in {}", dir.display());

    let mut failures = String::new();
    for path in &paths {
        let source = fs::read_to_string(path).unwrap();
        let (file, diags) = suzu::parser::parse(&source);
        let name = path.file_name().unwrap().to_string_lossy();
        for d in &diags {
            failures.push_str(&d.render(&name, &source));
        }
        if diags.is_empty() && file.items.is_empty() {
            failures.push_str(&format!("{name}: parsed to an empty file\n"));
        }
    }
    assert!(failures.is_empty(), "\n{failures}");
}
