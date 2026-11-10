use super::*;
use pklr::{EvaluatorBuilder, ModuleSource};
use std::path::{Path, PathBuf};

fn eval_with(builder: EvaluatorBuilder, src: &str) -> Result<serde_json::Value, String> {
    let mut ev = builder.build();
    ev.eval_source(src, Path::new("/workspace/main.pkl"))
        .map(|value| value.to_json())
        .map_err(|error| error.to_string())
}

#[test]
fn module_path_sources_serve_imports_and_their_relative_imports() {
    let builder = EvaluatorBuilder::new()
        .module_path_source(
            "/acme/Schema.pkl",
            "import \"Base.pkl\"\nanswer = Base.value + 1\n",
        )
        .module_path_source("/acme/Base.pkl", "value = 41\n");
    let json = eval_with(
        builder,
        "import \"modulepath:/acme/Schema.pkl\" as S\nresult = S.answer\n",
    )
    .unwrap();
    assert_eq!(json["result"], 42);
}

#[test]
fn module_path_supports_amends_and_typed_classes() {
    let builder = EvaluatorBuilder::new().module_path_source(
        "/acme/Schema.pkl",
        "module Schema\nclass Box { size: Int(this > 0) }\nbox: Box?\n",
    );
    let json = eval_with(
        builder,
        "amends \"modulepath:/acme/Schema.pkl\"\nbox = new Box { size = 3 }\n",
    )
    .unwrap();
    assert_eq!(json["box"]["size"], 3);
    let builder = EvaluatorBuilder::new().module_path_source(
        "/acme/Schema.pkl",
        "module Schema\nclass Box { size: Int(this > 0) }\nbox: Box?\n",
    );
    let error = eval_with(
        builder,
        "amends \"modulepath:/acme/Schema.pkl\"\nbox = new Box { size = 0 }\n",
    )
    .unwrap_err();
    assert!(error.contains("property 'size' expected Int"), "{error}");
}

#[test]
fn module_path_directories_are_searched_in_order() {
    let first = TestTempDir::new("pklr_module_path_first");
    let second = TestTempDir::new("pklr_module_path_second");
    std::fs::create_dir_all(first.path().join("lib")).unwrap();
    std::fs::create_dir_all(second.path().join("lib")).unwrap();
    std::fs::write(first.path().join("lib/A.pkl"), "name = \"first\"\n").unwrap();
    std::fs::write(second.path().join("lib/A.pkl"), "name = \"second\"\n").unwrap();
    std::fs::write(
        second.path().join("lib/B.pkl"),
        "import \"A.pkl\"\nname = A.name\n",
    )
    .unwrap();
    let builder = EvaluatorBuilder::new()
        .module_path_directory(first.path())
        .module_path_directory(second.path());
    let json = eval_with(
        builder,
        "import \"modulepath:/lib/A.pkl\" as A\nimport \"modulepath:/lib/B.pkl\" as B\na = A.name\nb = B.name\n",
    )
    .unwrap();
    // B lives only in the second directory; its relative import of A still
    // resolves through the whole module path, where the first entry wins.
    assert_eq!(json["a"], "first");
    assert_eq!(json["b"], "first");
}

#[test]
fn earlier_module_path_sources_shadow_later_directories() {
    let dir = TestTempDir::new("pklr_module_path_shadow");
    std::fs::create_dir_all(dir.path().join("acme")).unwrap();
    std::fs::write(
        dir.path().join("acme/Schema.pkl"),
        "origin = \"directory\"\n",
    )
    .unwrap();
    let builder = EvaluatorBuilder::new()
        .module_path_source("/acme/Schema.pkl", "origin = \"embedded\"\n")
        .module_path_directory(dir.path());
    let json = eval_with(
        builder,
        "import \"modulepath:/acme/Schema.pkl\" as S\no = S.origin\n",
    )
    .unwrap();
    assert_eq!(json["o"], "embedded");
}

#[test]
fn missing_module_path_modules_are_reported_by_uri() {
    let error = eval_with(
        EvaluatorBuilder::new(),
        "import \"modulepath:/acme/Missing.pkl\" as M\nx = M.y\n",
    )
    .unwrap_err();
    assert!(error.contains("modulepath:/acme/Missing.pkl"), "{error}");
}

// Expected results in this file were produced by `pkl eval --module-path`
// (Pkl 0.32.2) on the same modules.

#[test]
fn module_path_uris_that_climb_above_the_root_name_no_module() {
    let builder = EvaluatorBuilder::new()
        .module_path_source("/Top.pkl", "import \"../Out.pkl\"\nx = Out.y\n")
        .module_path_source("/Out.pkl", "y = 1\n");
    let error = eval_with(builder, "import \"modulepath:/Top.pkl\" as T\nx = T.x\n").unwrap_err();
    assert!(error.contains("modulepath:/../Out.pkl"), "{error}");
}

#[test]
fn triple_dot_imports_search_the_module_path_namespace() {
    let builder = EvaluatorBuilder::new()
        .module_path_source("/Top.pkl", "name = \"root\"\n")
        .module_path_source("/acme/sub/Top.pkl", "name = \"sub\"\n")
        .module_path_source(
            "/acme/sub/Deep.pkl",
            "import \".../Top.pkl\"\nn = Top.name\n",
        );
    let json = eval_with(
        builder,
        "import \"modulepath:/acme/sub/Deep.pkl\" as D\nr = D.n\n",
    )
    .unwrap();
    assert_eq!(json["r"], "root");
}

#[test]
fn module_path_resources_are_readable_absolutely_and_relatively() {
    let builder = EvaluatorBuilder::new()
        .module_path_source("/acme/data.txt", "hello\n")
        .module_path_source(
            "/acme/Res.pkl",
            "a = read(\"data.txt\").text\nb = read(\"/acme/data.txt\").text\nc = read?(\"none.txt\") == null\n",
        );
    let json = eval_with(
        builder,
        "import \"modulepath:/acme/Res.pkl\" as R\nr = List(R.a, R.b, R.c, read(\"modulepath:/acme/./x/../data.txt\").text)\n",
    )
    .unwrap();
    assert_eq!(
        json["r"],
        serde_json::json!(["hello\n", "hello\n", true, "hello\n"])
    );
}

#[test]
fn module_path_is_not_globbable() {
    let builder = || {
        EvaluatorBuilder::new()
            .module_path_source("/acme/A.pkl", "x = 1\n")
            .module_path_source(
                "/acme/Glob.pkl",
                "import* \"*.pkl\" as all\nkeys = all.keys.toList()\n",
            )
    };
    for source in [
        "import \"modulepath:/acme/Glob.pkl\" as G\nr = G.keys\n",
        "import* \"modulepath:/acme/*.pkl\" as all\nr = all.keys.toList()\n",
        "r = read*(\"modulepath:/acme/*.pkl\").keys.toList()\n",
    ] {
        let error = eval_with(builder(), source).unwrap_err();
        assert!(
            error.contains("Cannot expand glob pattern `modulepath:/acme/*.pkl` because scheme `modulepath` is not globbable."),
            "{error}"
        );
    }
}

struct LocalOnly;
impl ModuleSource for LocalOnly {
    fn identity(&mut self, path: &Path) -> pklr::Result<PathBuf> {
        assert!(
            !path.to_string_lossy().contains("modulepath:"),
            "{}",
            path.display()
        );
        Ok(path.into())
    }
    fn exists(&mut self, path: &Path) -> pklr::Result<bool> {
        assert!(
            !path.to_string_lossy().contains("modulepath:"),
            "{}",
            path.display()
        );
        Ok(false)
    }
    fn read(&mut self, path: &Path) -> pklr::Result<String> {
        panic!("unexpected local module read: {}", path.display())
    }
}

#[test]
fn module_sources_never_see_module_path_modules() {
    let mut ev = EvaluatorBuilder::new()
        .module_path_source("/acme/Schema.pkl", "answer = 42\n")
        .build();
    ev.set_module_source(LocalOnly);
    let json = ev
        .eval_source(
            "import \"modulepath:/acme/Schema.pkl\" as S\nresult = S.answer\n",
            Path::new("/workspace/main.pkl"),
        )
        .unwrap()
        .to_json();
    assert_eq!(json["result"], 42);
}

#[test]
#[should_panic(expected = "invalid module path source path")]
fn relative_module_path_source_paths_are_rejected() {
    let _ = EvaluatorBuilder::new().module_path_source("acme/Schema.pkl", "");
}
