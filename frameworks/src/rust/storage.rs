use super::TryInsert;
use anyhow::{anyhow, Error, Result};
use cargo_metadata::{MetadataCommand, Package};
use necessist_core::{util, Span};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    path::{Path, PathBuf},
};
use syn::{File, Ident};

/// Structures needed during parsing but not after.
pub struct Storage<'ast> {
    pub module_path: Vec<&'ast Ident>,
    pub test_file_fs_module_path_cache: BTreeMap<PathBuf, Vec<String>>,
    pub test_file_package_cache: BTreeMap<PathBuf, Package>,
    pub error: Option<Error>,
}

impl<'ast> Storage<'ast> {
    pub fn new(_file: &'ast File) -> Self {
        Self {
            module_path: Vec::new(),
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

            let test_file_relative_path = (|| {
                const PREFIXES: [(&str, bool); 3] =
                    [("src/bin", true), ("src", false), ("tests", true)];
                for (dir, path_includes_crate_name) in PREFIXES {
                    if let Ok(suffix) =
                        util::strip_prefix(test_file, manifest_dir.join(dir).as_std_path())
                    {
                        return if path_includes_crate_name {
                            let mut components = suffix.components();
                            components.next().map(|_| components.as_path())
                        } else {
                            Some(suffix)
                        };
                    }
                }
                None
            })()
            .ok_or(anyhow!(
                "Failed to determine relative path of test file {test_file:?}"
            ))?;

            fs_module_path(test_file_relative_path)
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

fn fs_module_path(path: &Path) -> Result<Vec<String>> {
    let Some(path_parent) = path.parent() else {
        return Ok(Vec::new());
    };

    let path_stem = path
        .file_stem()
        .ok_or_else(|| anyhow!("Failed to get file stem"))?;

    let mut fs_module_path = path_parent
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if !["lib", "main", "mod"].map(OsStr::new).contains(&path_stem) {
        fs_module_path.push(path_stem.to_string_lossy().to_string());
    }

    Ok(fs_module_path)
}
