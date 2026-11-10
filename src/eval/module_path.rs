//! `modulepath:` modules, served from the host's module path.
//!
//! As with `pkl --module-path`, a `modulepath:/a/B.pkl` URI names a module by
//! an absolute path inside one namespace. The host supplies the namespace as
//! an ordered list of entries: directories, or sources compiled into the host
//! (the counterpart of a Pkl library on the JVM class path). The first entry
//! that has the module wins. A relative import inside a `modulepath:` module
//! stays inside the namespace.
//!
//! The namespace is independent from [`crate::ModuleSource`], which governs
//! local file modules only.

use super::*;

pub(crate) const SCHEME: &str = "modulepath:";

#[derive(Clone)]
pub(crate) enum ModulePathEntry {
    Directory(PathBuf),
    Source { path: String, source: Arc<str> },
}

/// The namespace path (`/a/B.pkl`) of a `modulepath:` module identity.
pub(crate) fn namespace_path(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix(SCHEME)
}

/// Normalize the namespace path `path` to a `modulepath:` identity. The path
/// must be absolute and must not climb above the namespace root.
pub(crate) fn identity(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(format!("{SCHEME}{}", normalize(path)?)))
}

fn normalize(path: &str) -> Result<String> {
    let Some(rest) = path.strip_prefix('/') else {
        return Err(Error::Eval(format!(
            "module path URI must be absolute: {SCHEME}{path}"
        )));
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(Error::Eval(format!(
                        "module path URI escapes the module path: {SCHEME}{path}"
                    )));
                }
            }
            part => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(Error::Eval(format!(
            "module path URI names no module: {SCHEME}{path}"
        )));
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Resolve `uri`, found in the `modulepath:` module whose namespace path is
/// `base`, to a `modulepath:` identity.
pub(crate) fn resolve_relative(base: &str, uri: &str) -> Result<PathBuf> {
    if uri.starts_with('/') {
        return identity(uri);
    }
    let directory = base.rsplit_once('/').map_or("", |(directory, _)| directory);
    identity(&format!("{directory}/{uri}"))
}

/// The namespace path of an embedded source, checked when it is registered.
pub(crate) fn source_path(path: &str) -> Result<String> {
    normalize(path)
}

impl Evaluator {
    #[cfg(feature = "native-io")]
    pub(crate) fn set_module_path(&mut self, entries: Vec<ModulePathEntry>) {
        self.module_path = entries;
    }

    /// Append a directory to the module path.
    pub fn add_module_path_directory(&mut self, directory: impl Into<PathBuf>) {
        self.module_path
            .push(ModulePathEntry::Directory(directory.into()));
    }

    /// Append a module source to the module path at the absolute namespace
    /// path `path`, for example `/acme/Schema.pkl`, which Pkl code imports
    /// as `modulepath:/acme/Schema.pkl`.
    ///
    /// # Errors
    /// Rejects a path that is relative, empty or climbs above the root.
    pub fn add_module_path_source(
        &mut self,
        path: &str,
        source: impl Into<Arc<str>>,
    ) -> Result<()> {
        let path = source_path(path)?;
        self.module_path.push(ModulePathEntry::Source {
            path,
            source: source.into(),
        });
        Ok(())
    }

    /// The first module path entry that has the module at namespace path
    /// `path`, as its source or its file.
    fn find_module_path_entry(&mut self, path: &str) -> Result<Option<ModulePathHit>> {
        let path = normalize(path)?;
        for index in 0..self.module_path.len() {
            match &self.module_path[index] {
                ModulePathEntry::Source {
                    path: entry,
                    source,
                } => {
                    if *entry == path {
                        return Ok(Some(ModulePathHit::Source(Arc::clone(source))));
                    }
                }
                ModulePathEntry::Directory(directory) => {
                    let file = directory.join(&path[1..]);
                    if self.capabilities.path_exists(&file)? {
                        return Ok(Some(ModulePathHit::File(file)));
                    }
                }
            }
        }
        Ok(None)
    }

    pub(super) fn module_path_exists(&mut self, path: &str) -> Result<bool> {
        Ok(self.find_module_path_entry(path)?.is_some())
    }

    pub(super) fn module_path_read(&mut self, path: &str) -> Result<String> {
        match self.find_module_path_entry(path)? {
            Some(ModulePathHit::Source(source)) => Ok(source.to_string()),
            Some(ModulePathHit::File(file)) => self.capabilities.read_to_string(&file),
            None => Err(Error::ImportNotFound(format!("{SCHEME}{path}"))),
        }
    }
}

enum ModulePathHit {
    Source(Arc<str>),
    File(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_normalized_and_confined() {
        assert_eq!(
            identity("/a/./b/../C.pkl").unwrap(),
            PathBuf::from("modulepath:/a/C.pkl")
        );
        assert!(identity("a/C.pkl").is_err());
        assert!(identity("/../C.pkl").is_err());
        assert!(identity("/").is_err());
        assert_eq!(
            resolve_relative("/a/B.pkl", "../C.pkl").unwrap(),
            PathBuf::from("modulepath:/C.pkl")
        );
        assert_eq!(
            resolve_relative("/a/B.pkl", "/x/C.pkl").unwrap(),
            PathBuf::from("modulepath:/x/C.pkl")
        );
        assert!(resolve_relative("/B.pkl", "../C.pkl").is_err());
    }
}
