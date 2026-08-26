//! Linux AppImage: download the new image and rename it over the running one.
//! The running process keeps its open inode; the next launch gets the new file.

use std::path::Path;

use super::{fetch, InstallKind, Relaunch, Release, UpdateConfig, UpdateStage};

/// Download the release's AppImage and swap it in, returning the relaunch target
/// (the image path, handed to [`gpui::App::set_restart_path`]).
pub(crate) fn install(
  config: &UpdateConfig,
  release: &Release,
  target: &Path,
  on_stage: &dyn Fn(UpdateStage),
) -> Result<Relaunch, String> {
  // Resolved through `asset_for`, which matches the running architecture:
  // picking the AppImage by extension alone would hand an x86_64 machine the
  // aarch64 image and rename it over a working install.
  let asset = release
    .asset_for(&InstallKind::AppImage(target.to_path_buf()))
    .ok_or("this release hasn't published an AppImage for this architecture yet")?;
  // Stage *next to* the target, not in the temp dir: the final rename must not
  // cross filesystems (`/tmp` is often tmpfs), or it fails with EXDEV.
  let name = target
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or(&config.slug);
  let staged = target.with_file_name(format!(".{name}.update"));
  let total = asset.size;
  on_stage(UpdateStage::Downloading { done: 0, total });
  let fetched = fetch::file(&asset.url, &staged, total, &config.user_agent, &|done| {
    on_stage(UpdateStage::Downloading { done, total })
  });
  if let Err(e) = fetched {
    // A dead download must not strand a partial image next to the app.
    let _ = std::fs::remove_file(&staged);
    return Err(e);
  }

  // Verify before promoting, never after: `promote` renames the staged file
  // over the running binary, and there is no undo once the next launch is
  // pointed at it. macOS has `codesign` for this; here a published digest is
  // the only thing between a swapped asset and code that runs as the user.
  on_stage(UpdateStage::Verifying);
  if let Err(e) = config.verify_checksum(release, asset, &staged) {
    let _ = std::fs::remove_file(&staged);
    return Err(e);
  }

  on_stage(UpdateStage::Installing);
  promote(&staged, target)
}

/// Mark `staged` executable and rename it over `target`, dropping the staged file
/// if the rename fails.
fn promote(staged: &Path, target: &Path) -> Result<Relaunch, String> {
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(staged, std::fs::Permissions::from_mode(0o755));
  }
  if let Err(e) = std::fs::rename(staged, target) {
    let _ = std::fs::remove_file(staged);
    return Err(format!("replace AppImage: {e}"));
  }
  Ok(Relaunch::Binary(target.to_path_buf()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::update::{ReleaseAsset, UpdateSource};

  /// A scratch dir that cleans up after itself.
  fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
      std::env::temp_dir().join(format!("guise-update-image-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn config() -> UpdateConfig {
    UpdateConfig::new("Acme", "1.0.0", UpdateSource::github("acme/acme"))
  }

  #[test]
  fn promote_swaps_and_marks_executable() {
    let dir = scratch("promote");
    let target = dir.join("Acme.AppImage");
    let staged = dir.join(".Acme.AppImage.update");
    std::fs::write(&target, b"old").unwrap();
    std::fs::write(&staged, b"new").unwrap();

    let relaunch = promote(&staged, &target).unwrap();
    assert_eq!(relaunch, Relaunch::Binary(target.clone()));
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(!staged.exists());
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = std::fs::metadata(&target).unwrap().permissions().mode();
      assert_eq!(mode & 0o755, 0o755);
    }
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn promote_fails_cleanly_without_a_staged_file() {
    let dir = scratch("missing");
    let err = promote(&dir.join("absent"), &dir.join("target")).unwrap_err();
    assert!(err.contains("replace AppImage"));
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn promote_failure_drops_the_staged_file() {
    let dir = scratch("promotefail");
    let staged = dir.join(".Acme.AppImage.update");
    std::fs::write(&staged, b"new").unwrap();

    let err = promote(&staged, &dir.join("nosuchdir/Acme.AppImage")).unwrap_err();
    assert!(err.contains("replace AppImage"));
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn failed_download_leaves_no_staged_file() {
    let dir = scratch("download");
    let target = dir.join("Acme.AppImage");
    let staged = dir.join(".Acme.AppImage.update");
    // Simulate a dead download's partial output: the fetch is refused
    // (non-https) and any staged bytes must be swept up.
    std::fs::write(&staged, b"partial").unwrap();
    let release = Release {
      version: "9.9.9".to_string(),
      url: String::new(),
      assets: vec![ReleaseAsset {
        name: format!("Acme-9.9.9-{}.AppImage", std::env::consts::ARCH),
        url: "http://127.0.0.1/x".to_string(),
        size: 0,
      }],
    };

    assert!(install(&config(), &release, &target, &|_| {}).is_err());
    assert!(!staged.exists());
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn an_appimage_for_another_architecture_is_not_installed() {
    // A release that has only uploaded the other arch's image must be refused
    // outright, never renamed over the running install.
    let dir = scratch("otherarch");
    let target = dir.join("Acme.AppImage");
    std::fs::write(&target, b"working").unwrap();
    let other = if std::env::consts::ARCH == "aarch64" {
      "x86_64"
    } else {
      "aarch64"
    };
    let release = Release {
      version: "9.9.9".to_string(),
      url: String::new(),
      assets: vec![ReleaseAsset {
        name: format!("Acme-9.9.9-{other}.AppImage"),
        url: "https://d/x".to_string(),
        size: 0,
      }],
    };

    assert!(install(&config(), &release, &target, &|_| {}).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"working");
    let _ = std::fs::remove_dir_all(&dir);
  }
}
