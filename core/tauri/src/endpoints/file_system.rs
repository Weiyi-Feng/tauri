// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use crate::{
  api::{dir, file, path::BaseDirectory},
  scope::Scopes,
  Config, Env, Manager, PackageInfo, Runtime, Window,
};

use super::InvokeContext;
#[allow(unused_imports)]
use anyhow::Context;
use serde::{
  de::{Deserializer, Error as DeError},
  Deserialize, Serialize,
};
use tauri_macros::{module_command_handler, CommandModule};

use std::fmt::{Debug, Formatter};
use std::{
  fs,
  fs::File,
  io::Write,
  path::{Component, Path},
  sync::Arc,
};

#[derive(Clone, Debug)]
pub struct SafePathBuf(std::path::PathBuf);

impl AsRef<Path> for SafePathBuf {
  fn as_ref(&self) -> &Path {
    self.0.as_ref()
  }
}

impl<'de> Deserialize<'de> for SafePathBuf {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let path = std::path::PathBuf::deserialize(deserializer)?;
    Ok(SafePathBuf(path))
  }
}

/// The options for the directory functions on the file system API.
#[derive(Debug, Clone, Deserialize)]
pub struct DirOperationOptions {
  /// Whether the API should recursively perform the operation on the directory.
  #[serde(default)]
  pub recursive: bool,
  /// The base directory of the operation.
  /// The directory path of the BaseDirectory will be the prefix of the defined directory path.
  pub dir: Option<BaseDirectory>,
}

/// The options for the file functions on the file system API.
#[derive(Debug, Clone, Deserialize)]
pub struct FileOperationOptions {
  /// The base directory of the operation.
  /// The directory path of the BaseDirectory will be the prefix of the defined file path.
  pub dir: Option<BaseDirectory>,
}

/// The API descriptor.
#[derive(Deserialize, CommandModule)]
#[serde(tag = "cmd", rename_all = "camelCase")]
pub enum Cmd {
  /// The read text file API.
  ReadFile {
    path: SafePathBuf,
    options: Option<FileOperationOptions>,
  },
  /// The write file API.
  WriteFile {
    path: SafePathBuf,
    contents: Vec<u8>,
    options: Option<FileOperationOptions>,
  },
  /// The read dir API.
  ReadDir {
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  },
  /// The copy file API.
  CopyFile {
    source: SafePathBuf,
    destination: SafePathBuf,
    options: Option<FileOperationOptions>,
  },
  /// The create dir API.
  CreateDir {
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  },
  /// The remove dir API.
  RemoveDir {
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  },
  /// The remove file API.
  RemoveFile {
    path: SafePathBuf,
    options: Option<FileOperationOptions>,
  },
  /// The rename file API.
  #[serde(rename_all = "camelCase")]
  RenameFile {
    old_path: SafePathBuf,
    new_path: SafePathBuf,
    options: Option<FileOperationOptions>,
  },
}

impl Cmd {
  #[module_command_handler(fs_read_file, "fs > readFile")]
  fn read_file<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) -> super::Result<Vec<u8>> {
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      options.and_then(|o| o.dir),
    )?;
    file::read_binary(&resolved_path)
      .with_context(|| format!("path: {}", resolved_path.0.display()))
      .map_err(Into::into)
  }

  #[module_command_handler(fs_write_file, "fs > writeFile")]
  fn write_file<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    contents: Vec<u8>,
    options: Option<FileOperationOptions>,
  ) -> super::Result<()> {
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      options.and_then(|o| o.dir),
    )?;
    File::create(&resolved_path)
      .with_context(|| format!("path: {}", resolved_path.0.display()))
      .map_err(Into::into)
      .and_then(|mut f| f.write_all(&contents).map_err(|err| err.into()))
  }

  #[module_command_handler(fs_read_dir, "fs > readDir")]
  fn read_dir<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  ) -> super::Result<Vec<dir::DiskEntry>> {
    let (recursive, dir) = if let Some(options_value) = options {
      (options_value.recursive, options_value.dir)
    } else {
      (false, None)
    };
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      dir,
    )?;
    dir::read_dir(&resolved_path, recursive)
      .with_context(|| format!("path: {}", resolved_path.0.display()))
      .map_err(Into::into)
  }

  #[module_command_handler(fs_copy_file, "fs > copyFile")]
  fn copy_file<R: Runtime>(
    context: InvokeContext<R>,
    source: SafePathBuf,
    destination: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) -> super::Result<()> {
    let (src, dest) = match options.and_then(|o| o.dir) {
      Some(dir) => (
        resolve_path(
          &context.config,
          &context.package_info,
          &context.window,
          source,
          Some(dir),
        )?,
        resolve_path(
          &context.config,
          &context.package_info,
          &context.window,
          destination,
          Some(dir),
        )?,
      ),
      None => (source, destination),
    };
    fs::copy(src.clone(), dest.clone())
      .with_context(|| format!("source: {}, dest: {}", src.0.display(), dest.0.display()))?;
    Ok(())
  }

  #[module_command_handler(fs_create_dir, "fs > createDir")]
  fn create_dir<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  ) -> super::Result<()> {
    let (recursive, dir) = if let Some(options_value) = options {
      (options_value.recursive, options_value.dir)
    } else {
      (false, None)
    };
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      dir,
    )?;
    if recursive {
      fs::create_dir_all(&resolved_path)
        .with_context(|| format!("path: {}", resolved_path.0.display()))?;
    } else {
      fs::create_dir(&resolved_path)
        .with_context(|| format!("path: {} (non recursive)", resolved_path.0.display()))?;
    }

    Ok(())
  }

  #[module_command_handler(fs_remove_dir, "fs > removeDir")]
  fn remove_dir<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    options: Option<DirOperationOptions>,
  ) -> super::Result<()> {
    let (recursive, dir) = if let Some(options_value) = options {
      (options_value.recursive, options_value.dir)
    } else {
      (false, None)
    };
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      dir,
    )?;
    if recursive {
      fs::remove_dir_all(&resolved_path)
        .with_context(|| format!("path: {}", resolved_path.0.display()))?;
    } else {
      fs::remove_dir(&resolved_path)
        .with_context(|| format!("path: {} (non recursive)", resolved_path.0.display()))?;
    }

    Ok(())
  }

  #[module_command_handler(fs_remove_file, "fs > removeFile")]
  fn remove_file<R: Runtime>(
    context: InvokeContext<R>,
    path: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) -> super::Result<()> {
    let resolved_path = resolve_path(
      &context.config,
      &context.package_info,
      &context.window,
      path,
      options.and_then(|o| o.dir),
    )?;
    fs::remove_file(&resolved_path)
      .with_context(|| format!("path: {}", resolved_path.0.display()))?;
    Ok(())
  }

  #[module_command_handler(fs_rename_file, "fs > renameFile")]
  fn rename_file<R: Runtime>(
    context: InvokeContext<R>,
    old_path: SafePathBuf,
    new_path: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) -> super::Result<()> {
    let (old, new) = match options.and_then(|o| o.dir) {
      Some(dir) => (
        resolve_path(
          &context.config,
          &context.package_info,
          &context.window,
          old_path,
          Some(dir),
        )?,
        resolve_path(
          &context.config,
          &context.package_info,
          &context.window,
          new_path,
          Some(dir),
        )?,
      ),
      None => (old_path, new_path),
    };
    fs::rename(&old, &new)
      .with_context(|| format!("old: {}, new: {}", old.0.display(), new.0.display()))
      .map_err(Into::into)
  }
}

#[allow(dead_code)]
fn resolve_path<R: Runtime>(
  config: &Config,
  package_info: &PackageInfo,
  window: &Window<R>,
  path: SafePathBuf,
  dir: Option<BaseDirectory>,
) -> super::Result<SafePathBuf> {
  let env = window.state::<Env>().inner();
  match crate::api::path::resolve_path(config, package_info, env, &path, dir) {
    Ok(path) => {
      Ok(SafePathBuf(path))
    }
    Err(e) => super::Result::<SafePathBuf>::Err(e.into())
      .with_context(|| format!("path: {}, base dir: {:?}", path.0.display(), dir)),
  }
}

#[cfg(test)]
mod tests {
  use super::{BaseDirectory, DirOperationOptions, FileOperationOptions, SafePathBuf};

  use quickcheck::{Arbitrary, Gen};

  use std::path::PathBuf;

  impl Arbitrary for super::SafePathBuf {
    fn arbitrary(g: &mut Gen) -> Self {
      Self(PathBuf::arbitrary(g))
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
      Box::new(self.0.shrink().map(SafePathBuf))
    }
  }

  impl Arbitrary for BaseDirectory {
    fn arbitrary(g: &mut Gen) -> Self {
      if bool::arbitrary(g) {
        BaseDirectory::App
      } else {
        BaseDirectory::Resource
      }
    }
  }

  impl Arbitrary for FileOperationOptions {
    fn arbitrary(g: &mut Gen) -> Self {
      Self {
        dir: Option::arbitrary(g),
      }
    }
  }

  impl Arbitrary for DirOperationOptions {
    fn arbitrary(g: &mut Gen) -> Self {
      Self {
        recursive: bool::arbitrary(g),
        dir: Option::arbitrary(g),
      }
    }
  }

  #[tauri_macros::module_command_test(fs_read_file, "fs > readFile")]
  #[quickcheck_macros::quickcheck]
  fn read_file(path: SafePathBuf, options: Option<FileOperationOptions>) {
    let res = super::Cmd::read_file(crate::test::mock_invoke_context(), path, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_write_file, "fs > writeFile")]
  #[quickcheck_macros::quickcheck]
  fn write_file(path: SafePathBuf, contents: Vec<u8>, options: Option<FileOperationOptions>) {
    let res = super::Cmd::write_file(crate::test::mock_invoke_context(), path, contents, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_read_dir, "fs > readDir")]
  #[quickcheck_macros::quickcheck]
  fn read_dir(path: SafePathBuf, options: Option<DirOperationOptions>) {
    let res = super::Cmd::read_dir(crate::test::mock_invoke_context(), path, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_copy_file, "fs > copyFile")]
  #[quickcheck_macros::quickcheck]
  fn copy_file(
    source: SafePathBuf,
    destination: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) {
    let res = super::Cmd::copy_file(
      crate::test::mock_invoke_context(),
      source,
      destination,
      options,
    );
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_create_dir, "fs > createDir")]
  #[quickcheck_macros::quickcheck]
  fn create_dir(path: SafePathBuf, options: Option<DirOperationOptions>) {
    let res = super::Cmd::create_dir(crate::test::mock_invoke_context(), path, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_remove_dir, "fs > removeDir")]
  #[quickcheck_macros::quickcheck]
  fn remove_dir(path: SafePathBuf, options: Option<DirOperationOptions>) {
    let res = super::Cmd::remove_dir(crate::test::mock_invoke_context(), path, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_remove_file, "fs > removeFile")]
  #[quickcheck_macros::quickcheck]
  fn remove_file(path: SafePathBuf, options: Option<FileOperationOptions>) {
    let res = super::Cmd::remove_file(crate::test::mock_invoke_context(), path, options);
    crate::test_utils::assert_not_allowlist_error(res);
  }

  #[tauri_macros::module_command_test(fs_rename_file, "fs > renameFile")]
  #[quickcheck_macros::quickcheck]
  fn rename_file(
    old_path: SafePathBuf,
    new_path: SafePathBuf,
    options: Option<FileOperationOptions>,
  ) {
    let res = super::Cmd::rename_file(
      crate::test::mock_invoke_context(),
      old_path,
      new_path,
      options,
    );
    crate::test_utils::assert_not_allowlist_error(res);
  }
}
