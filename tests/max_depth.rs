//! `EvaluatorBuilder::max_depth` raises the expression nesting limit.

use std::io::Write;

fn nested_lets(depth: usize) -> String {
    let mut source = String::from("x = ");
    for level in 0..depth {
        source.push_str(&format!("let (v{level} = {level}) "));
    }
    source.push_str("v0\n");
    source
}

fn module(name: &str, depth: usize) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("pklr-max-depth-{}-{name}.pkl", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create module");
    file.write_all(nested_lets(depth).as_bytes())
        .expect("write module");
    path
}

#[test]
fn default_limit_rejects_deep_nesting() {
    let path = module("default", 40);
    let error = pklr::EvaluatorBuilder::new()
        .eval_to_json(&path)
        .expect_err("depth 40 exceeds the default");
    std::fs::remove_file(&path).ok();
    assert!(
        error
            .to_string()
            .contains("maximum recursion depth exceeded"),
        "{error}"
    );
}

#[test]
fn raised_limit_accepts_it() {
    let path = module("raised", 40);
    let value = pklr::EvaluatorBuilder::new()
        .max_depth(64)
        .eval_to_json(&path)
        .expect("depth 40 fits in 64");
    std::fs::remove_file(&path).ok();
    assert_eq!(value["x"], 0);
}
