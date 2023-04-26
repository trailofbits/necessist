use super::TryInsert;
use anyhow::{anyhow, Error, Result};
use cargo_metadata::{MetadataCommand, Package};
use necessist_core::{util, Span};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use syn::{ExprMethodCall, File, Ident, Stmt};

/// Structures needed during parsing but not after.
pub struct Storage<'ast> {
    pub module_path: Vec<&'ast Ident>,
    pub last_statement_visited: Option<&'ast Stmt>,
    pub last_method_call_visited: Option<&'ast ExprMethodCall>,
    pub test_file_fs_module_path_cache: BTreeMap<PathBuf, Vec<String>>,
    pub test_file_package_cache: BTreeMap<PathBuf, Package>,
    pub error: Option<Error>,
}

impl<'ast> Storage<'ast> {
    pub fn new(_file: &'ast File) -> Self {
        Self {
            module_path: Vec::new(),
            last_statement_visited: None,
            last_method_call_visited: None,
            test_file_fs_module_path_cache: BTreeMap::new(),
            test_file_package_cache: BTreeMap::new(),
            error: None,
        }
    }

    pub fn test_path(&mut self, span: &Span, name: &str) -> Result<Vec<String>> {
        let mut test_path = cached_test_file_fs_module_path(
            &mut self.test_file_fs_module_path_cache,
            &mut self.test_file_package_cache,
            &span.source_file,
        )
        .cloned()?;
        test_path.extend(self.module_path.iter().map(ToString::to_string));
        test_path.push(name.to_string());
        Ok(test_path)
    }
}

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
pub(super) fn cached_test_file_fs_module_path<'a>(
    test_file_fs_module_path_map: &'a mut BTreeMap<PathBuf, Vec<String>>,
    test_file_package_map: &mut BTreeMap<PathBuf, Package>,
    test_file: &Path,
) -> Result<&'a Vec<String>> {
    test_file_fs_module_path_map
        .entry(test_file.to_path_buf())
        .or_try_insert_with(|| {
            let package = cached_test_file_package(test_file_package_map, test_file)?;

            let manifest_dir = package
                .manifest_path
                .parent()
                .ok_or_else(|| anyhow!("Failed to get parent directory"))?;

            let src_dir = manifest_dir.join("src");
            let (test_file_relative_path, is_integration_test) =
                util::strip_prefix(test_file, src_dir.as_std_path())
                    .map(|path| (path, false))
                    .or_else(|_| {
                        let test_dir = manifest_dir.join("tests");
                        util::strip_prefix(test_file, test_dir.as_std_path())
                            .map(|path| (path, true))
                    })?;

            if (test_file_relative_path == Path::new("lib.rs")
                || test_file_relative_path == Path::new("main.rs"))
                && !is_integration_test
            {
                return Ok(Vec::new());
            }

            let test_file_relative_path_parent = test_file_relative_path
                .parent()
                .ok_or_else(|| anyhow!("Failed to get parent directory"))?;
            let test_file_relative_path_stem = test_file_relative_path
                .file_stem()
                .ok_or_else(|| anyhow!("Failed to get file stem"))?;

            let mut fs_module_path = test_file_relative_path_parent
                .components()
                .map(|component| component.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>();
            if test_file_relative_path_stem != "mod" && !is_integration_test {
                fs_module_path.push(test_file_relative_path_stem.to_string_lossy().to_string());
            }

            Ok(fs_module_path)
        })
        .map(|value| value as &_)
}

#[cfg_attr(
    dylint_lib = "non_local_effect_before_error_return",
    allow(non_local_effect_before_error_return)
)]
pub(super) fn cached_test_file_package<'a>(
    test_file_package_map: &'a mut BTreeMap<PathBuf, Package>,
    test_file: &Path,
) -> Result<&'a Package> {
    test_file_package_map
        .entry(test_file.to_path_buf())
        .or_try_insert_with(|| {
            let parent = test_file
                .parent()
                .ok_or_else(|| anyhow!("Failed to get parent directory"))?;

            let metadata = MetadataCommand::new()
                .current_dir(parent)
                .no_deps()
                .exec()?;

            // smoelius: Use the package whose manifest directory is nearest to the test file.
            let mut package_near: Option<Package> = None;
            for package_curr in metadata.packages {
                let manifest_dir = package_curr
                    .manifest_path
                    .parent()
                    .ok_or_else(|| anyhow!("Failed to get parent directory"))?;
                if !test_file.starts_with(manifest_dir) {
                    continue;
                }
                if let Some(package_prev) = &package_near {
                    if package_prev.manifest_path.components().count()
                        < package_curr.manifest_path.components().count()
                    {
                        package_near = Some(package_curr);
                    }
                } else {
                    package_near = Some(package_curr);
                }
            }

            package_near.ok_or_else(|| anyhow!("Failed to determine package"))
        })
        .map(|value| value as &_)
}
