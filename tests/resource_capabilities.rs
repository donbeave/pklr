#![cfg(feature = "native-io")]

use pklr::{EvalCapabilities, Evaluator, NativeCapabilities, Result};
use std::path::{Path, PathBuf};

struct ResourceDenied {
    native: NativeCapabilities,
    denied: &'static str,
}
impl EvalCapabilities for ResourceDenied {
    fn is_native_filesystem(&self) -> bool {
        true
    }
    fn read_to_string(&mut self, path: &Path) -> Result<String> {
        self.native.read_to_string(path)
    }
    fn path_exists(&mut self, path: &Path) -> Result<bool> {
        self.native.path_exists(path)
    }
    fn canonicalize(&mut self, path: &Path) -> Result<PathBuf> {
        self.native.canonicalize(path)
    }
    fn read_env(&mut self, name: &str) -> Result<Option<String>> {
        self.native.read_env(name)
    }
    fn fetch_text(&mut self, url: &str) -> Result<String> {
        self.native.fetch_text(url)
    }
    fn fetch_bytes(&mut self, url: &str) -> Result<Vec<u8>> {
        self.native.fetch_bytes(url)
    }
    fn temp_dir(&mut self, prefix: &str) -> Result<PathBuf> {
        self.native.temp_dir(prefix)
    }
    fn glob(&mut self, base: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
        self.native.glob(base, pattern)
    }
    fn resource_path_exists(&mut self, path: &Path) -> Result<bool> {
        if self.denied == "existence" {
            Err(pklr::Error::Eval("resource existence denied".into()))
        } else {
            self.native.path_exists(path)
        }
    }
    fn resource_read_bytes(&mut self, path: &Path) -> Result<Vec<u8>> {
        if self.denied == "read" {
            Err(pklr::Error::Eval("resource read denied".into()))
        } else {
            self.native.read_bytes(path)
        }
    }
    fn resource_glob(&mut self, base: &Path, pattern: &str) -> Result<Vec<PathBuf>> {
        if self.denied == "glob" {
            Err(pklr::Error::Eval("resource glob denied".into()))
        } else {
            self.native.glob(base, pattern)
        }
    }
}

#[test]
fn resource_hooks_are_distinct_from_module_imports()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(format!("pkl-resource-hooks-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("module.pkl"), "value = 42\n")?;
    let root = dir.join("root.pkl");
    let mut evaluator = Evaluator::with_capabilities(ResourceDenied {
        native: NativeCapabilities::new(),
        denied: "existence",
    });
    evaluator.set_base_path(&dir);
    std::fs::write(
        &root,
        "import \"module.pkl\" as other\nvalue = other.value\n",
    )?;
    assert_eq!(evaluator.eval_file(&root)?.to_json()["value"], 42);
    std::fs::write(dir.join("input.txt"), "synthetic")?;
    for (source, denied, expected) in [
        (
            "value = read(\"input.txt\")\n",
            "existence",
            "resource existence denied",
        ),
        (
            "value = read(\"input.txt\")\n",
            "read",
            "resource read denied",
        ),
        ("value = read*(\"*.txt\")\n", "glob", "resource glob denied"),
    ] {
        std::fs::write(&root, source)?;
        let mut evaluator = Evaluator::with_capabilities(ResourceDenied {
            native: NativeCapabilities::new(),
            denied,
        });
        evaluator.set_base_path(&dir);
        assert!(
            evaluator
                .eval_file(&root)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
