//! `modulepath:` modules and resources, served from the host's module path.
//!
//! This follows `pkl --module-path`: a `modulepath:/a/B.pkl` URI names a
//! module or resource by an absolute path inside one namespace, and the
//! module path's entries are searched in order. An entry is a module or
//! resource compiled into the host (the counterpart of a library on the
//! JVM class path). Serving `modulepath:` imports from module path
//! *directories* is upstream since jdx/pklr@d0b0a05 (`#415`,
//! `EvaluatorBuilder::module_path`), so this fork no longer carries its
//! own directory entries. Relative and triple-dot URIs inside a
//! `modulepath:` module stay inside the namespace, as with the JVM
//! evaluator. Globs are refused: Pkl does not consider the scheme
//! globbable.
//!
//! The namespace is independent from [`crate::ModuleSource`], which governs
//! local file modules only.

use super::*;

pub(crate) const SCHEME: &str = "modulepath:";

#[derive(Clone)]
pub(crate) enum ModulePathEntry {
    Source { path: String, source: Arc<str> },
}

/// The namespace path (`/a/B.pkl`) of a `modulepath:` identity.
pub(crate) fn namespace_path(path: &Path) -> Option<&str> {
    path.to_str()?.strip_prefix(SCHEME)
}

/// The `modulepath:` identity of the absolute namespace path `path`.
pub(crate) fn identity(path: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(format!("{SCHEME}{}", normalize(path)?)))
}

/// Remove `.` and `..` segments like `java.net.URI.normalize`, which keeps a
/// `..` that would climb above the root. Such a path names nothing.
fn normalize(path: &str) -> Result<String> {
    let Some(rest) = path.strip_prefix('/') else {
        return Err(Error::Eval(format!(
            "Module path URI `{SCHEME}{path}` has invalid syntax: the path must be absolute."
        )));
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

/// Resolve the relative `uri` found in the `modulepath:` module or resource
/// whose namespace path is `base`.
pub(crate) fn resolve_relative(base: &str, uri: &str) -> Result<PathBuf> {
    if uri.starts_with('/') {
        return identity(uri);
    }
    let directory = base.rsplit_once('/').map_or("", |(directory, _)| directory);
    identity(&format!("{directory}/{uri}"))
}

/// The namespace path of a host source, checked when it is registered.
pub(crate) fn source_path(path: &str) -> Result<String> {
    let normalized = normalize(path)?;
    if normalized == "/" || normalized.starts_with("/..") {
        return Err(Error::Eval(format!(
            "module path source path `{path}` names no module"
        )));
    }
    Ok(normalized)
}

impl Evaluator {
    #[cfg(feature = "native-io")]
    pub(crate) fn set_module_path(&mut self, entries: Vec<ModulePathEntry>) {
        self.module_path = entries;
    }

    /// Append a module or resource compiled into the host to the module path
    /// at the absolute namespace path `path`, for example `/acme/Schema.pkl`,
    /// which Pkl code imports as `modulepath:/acme/Schema.pkl`.
    ///
    /// # Errors
    /// Rejects a path that is relative or names no module.
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

    /// Resolve `uri`, a triple-dot or relative URI found in the `modulepath:`
    /// module or resource at namespace path `base`.
    pub(super) fn resolve_module_path_relative(
        &mut self,
        base: &str,
        uri: &str,
    ) -> Result<PathBuf> {
        let Some(triple_dot) = parse_triple_dot_path(uri)? else {
            return resolve_relative(base, uri);
        };
        // Search the namespace the same way a file module searches its
        // ancestor directories, with the namespace root as the last stop.
        let base = normalize(base)?;
        let found = resolve_triple_dot(
            Path::new(&base),
            triple_dot,
            Some(Path::new("/")),
            |candidate| {
                Ok(candidate.to_str().is_some_and(|candidate| {
                    self.find_module_path_entry(candidate)
                        .ok()
                        .flatten()
                        .is_some()
                }))
            },
        )?;
        match found {
            Some(path) => identity(&path.to_string_lossy()),
            None => resolve_relative(&base, uri),
        }
    }

    /// The first module path entry that has the module or resource at
    /// namespace path `path`.
    fn find_module_path_entry(&mut self, path: &str) -> Result<Option<Arc<str>>> {
        let path = normalize(path)?;
        if path == "/" || path.starts_with("/..") {
            return Ok(None);
        }
        for index in 0..self.module_path.len() {
            match &self.module_path[index] {
                ModulePathEntry::Source {
                    path: entry,
                    source,
                } => {
                    if *entry == path {
                        return Ok(Some(Arc::clone(source)));
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
            Some(source) => Ok(source.to_string()),
            None => Err(Error::ImportNotFound(format!("{SCHEME}{path}"))),
        }
    }

    /// The bytes of the resource at namespace path `path`, or `None`.
    pub(super) fn module_path_read_bytes(&mut self, path: &str) -> Result<Option<Vec<u8>>> {
        match self.find_module_path_entry(path)? {
            Some(source) => Ok(Some(source.as_bytes().to_vec())),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_normalized_like_java_uris() {
        assert_eq!(
            identity("/a/./b/../C.pkl").unwrap(),
            PathBuf::from("modulepath:/a/C.pkl")
        );
        assert_eq!(
            identity("/../C.pkl").unwrap(),
            PathBuf::from("modulepath:/../C.pkl")
        );
        assert!(identity("a/C.pkl").is_err());
        assert_eq!(
            resolve_relative("/a/B.pkl", "../C.pkl").unwrap(),
            PathBuf::from("modulepath:/C.pkl")
        );
        assert_eq!(
            resolve_relative("/a/B.pkl", "/x/C.pkl").unwrap(),
            PathBuf::from("modulepath:/x/C.pkl")
        );
        assert_eq!(
            resolve_relative("/B.pkl", "../C.pkl").unwrap(),
            PathBuf::from("modulepath:/../C.pkl")
        );
        assert!(source_path("acme/A.pkl").is_err());
        assert!(source_path("/../A.pkl").is_err());
        assert!(source_path("/").is_err());
    }
}
