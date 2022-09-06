use crate::TryInsert;
use anyhow::{anyhow, Result};
use cargo_metadata::{MetadataCommand, Package};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Structures needed during parsing but not after.
#[derive(Default)]
pub(super) struct Parsing {
    pub test_file_fs_module_path_cache: BTreeMap<PathBuf, Vec<String>>,
    pub test_file_package_cache: BTreeMap<PathBuf, Package>,
}

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
                .ok_or_else(|| anyhow!("Could not get parent directory"))?;

            let src_dir = manifest_dir.join("src");
            let (test_file_relative_path, is_integration_test) = test_file
                .strip_prefix(src_dir)
                .map(|path| (path, false))
                .or_else(|_| {
                    let test_dir = manifest_dir.join("tests");
                    test_file.strip_prefix(test_dir).map(|path| (path, true))
                })?;

            if (test_file_relative_path == Path::new("lib.rs")
                || test_file_relative_path == Path::new("main.rs"))
                && !is_integration_test
            {
                return Ok(Vec::new());
            }

            let test_file_relative_path_parent = test_file_relative_path
                .parent()
                .ok_or_else(|| anyhow!("Could not get parent directory"))?;
            let test_file_relative_path_stem = test_file_relative_path
                .file_stem()
                .ok_or_else(|| anyhow!("Could not get file stem"))?;

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

pub(super) fn cached_test_file_package<'a>(
    test_file_package_map: &'a mut BTreeMap<PathBuf, Package>,
    test_file: &Path,
) -> Result<&'a Package> {
    test_file_package_map
        .entry(test_file.to_path_buf())
        .or_try_insert_with(|| {
            let parent = test_file
                .parent()
                .ok_or_else(|| anyhow!("Could not get parent directory"))?;

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
                    .ok_or_else(|| anyhow!("Could not get parent directory"))?;
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

            package_near.ok_or_else(|| anyhow!("Could not determine package"))
        })
        .map(|value| value as &_)
}
