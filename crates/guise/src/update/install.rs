//! Install detection and the vocabulary the installer reports back with.

#[cfg(any(target_os = "macos", test))]
use std::path::Path;
use std::path::PathBuf;

/// How this copy of the app was installed, which decides the update path. An
/// app self-updates where it can rewrite its own install; anything else opens
/// the download page. (Some variants are only ever constructed on their
/// platform.)
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallKind {
  /// A macOS `.app` bundle at this path — rewrite its contents in place.
  /// Covers every macOS install, Homebrew casks included; how it got there
  /// doesn't matter.
  MacApp(PathBuf),
  /// A running AppImage at this path (replace the file).
  AppImage(PathBuf),
  /// An install that can't be rewritten from inside the app — a root-owned
  /// distro package (`.deb`/`.rpm`), a Windows install, or a dev build. Falls
  /// back to opening the release page.
  Unknown,
}

impl InstallKind {
  /// Whether this install can be updated in place (vs. opening the page).
  pub fn is_in_place(&self) -> bool {
    matches!(self, InstallKind::MacApp(_) | InstallKind::AppImage(_))
  }
}

/// What the installer is doing, reported as it happens so the UI can show real
/// progress. Without this the whole install is one opaque blocking call, and a
/// failure that arrives in microseconds — a missing asset, say — never renders a
/// single frame of feedback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStage {
  /// Fetching the asset: bytes written of the total (0 total = unknown).
  Downloading { done: u64, total: u64 },
  /// Opening what was downloaded (macOS mounts the `.dmg`).
  Preparing,
  /// Writing the new version over the install.
  Installing,
  /// Checking the result before relaunching into it.
  Verifying,
}

impl UpdateStage {
  /// Short present-tense label for the UI. Lives here so the stages and the
  /// words describing them can't drift apart.
  pub fn label(&self) -> &'static str {
    match self {
      UpdateStage::Downloading { .. } => "Downloading update…",
      UpdateStage::Preparing => "Preparing…",
      UpdateStage::Installing => "Installing…",
      UpdateStage::Verifying => "Verifying…",
    }
  }
}

/// How to relaunch after a successful install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relaunch {
  /// The install was rewritten in place at its existing path: restart with
  /// **no** explicit binary path, so gpui reopens the running bundle via
  /// `NSBundle`. Never hand the restart an explicit path here — `open` on a
  /// path whose LaunchServices registration is stale can fall back to running
  /// the inner Mach-O inside Terminal.app.
  Current,
  /// Restart by launching this binary ([`gpui::App::set_restart_path`]).
  Binary(PathBuf),
}

/// The `.app` bundle three levels above a macOS executable
/// (`…/Acme.app/Contents/MacOS/acme`), if there is one.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn bundle_of(exe: &Path) -> Option<PathBuf> {
  exe
    .ancestors()
    .nth(3)
    .filter(|p| p.extension().is_some_and(|e| e == "app"))
    .map(|p| p.to_path_buf())
}

/// Detect the install method from the running executable and environment.
///
/// This only decides *how* to install an update — whether one exists is
/// [`super::UpdateConfig::check`], which asks the release feed. No package
/// manager is consulted.
pub fn detect() -> InstallKind {
  // A Linux AppImage exports APPIMAGE pointing at the running image.
  if let Some(image) = std::env::var_os("APPIMAGE") {
    return InstallKind::AppImage(PathBuf::from(image));
  }
  #[cfg(target_os = "macos")]
  {
    // Any macOS .app self-updates; Homebrew is never asked whether it owns it.
    let exe = std::env::current_exe().unwrap_or_default();
    if let Some(app) = bundle_of(&exe) {
      return InstallKind::MacApp(app);
    }
  }
  // A Linux distro package under a system prefix is root-owned, and Windows
  // installs update through their own package flow; neither can be swapped in
  // place, so both fall through to Unknown (open the download page).
  InstallKind::Unknown
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_swappable_installs_update_in_place() {
    // A macOS .app and a Linux AppImage are rewritten in place; everything
    // else (a root-owned distro package, Windows, a dev build) opens the page.
    assert!(InstallKind::MacApp(PathBuf::from("/Applications/Acme.app")).is_in_place());
    assert!(InstallKind::AppImage(PathBuf::from("/x/Acme.AppImage")).is_in_place());
    assert!(!InstallKind::Unknown.is_in_place());
  }

  #[test]
  fn bundle_is_three_levels_above_the_executable() {
    assert_eq!(
      bundle_of(Path::new("/Applications/Acme.app/Contents/MacOS/acme")),
      Some(PathBuf::from("/Applications/Acme.app"))
    );
  }

  #[test]
  fn unbundled_executables_have_no_bundle() {
    // A dev build under target/ must not be mistaken for an installable .app.
    assert_eq!(bundle_of(Path::new("/dev/acme/target/release/acme")), None);
    assert_eq!(bundle_of(Path::new("/usr/local/bin/acme")), None);
    assert_eq!(bundle_of(Path::new("acme")), None);
  }

  #[test]
  fn every_stage_has_a_label() {
    // The UI renders these verbatim, so an empty one is a blank status line.
    for stage in [
      UpdateStage::Downloading { done: 0, total: 0 },
      UpdateStage::Preparing,
      UpdateStage::Installing,
      UpdateStage::Verifying,
    ] {
      assert!(!stage.label().is_empty());
    }
  }
}
