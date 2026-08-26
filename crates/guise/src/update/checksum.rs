//! SHA-256 verification of a downloaded asset.
//!
//! macOS installs are gated on `codesign` against a pinned requirement, which
//! answers "did the people who ship this app produce this bundle". Linux has no
//! equivalent: [`appimage`](super::appimage) downloads a file, marks it
//! executable, and renames it over the running binary, and the only thing
//! standing between a wrong file and arbitrary code execution on the next
//! launch is that the release feed said so. A byte count is not an integrity
//! check.
//!
//! A published digest closes most of that gap. It doesn't prove authorship the
//! way a signature does — an attacker who can rewrite the release can rewrite
//! the digest beside it — but it does defeat a swapped or corrupted asset, a
//! poisoned mirror or CDN, and a truncated download that happens to match the
//! advertised size. [`UpdateConfig::require_checksum`](super::UpdateConfig::require_checksum)
//! turns "no digest published" from a silent pass into a refusal.
//!
//! The hash comes from the system's own tool, the same way the rest of this
//! module shells out rather than growing a dependency.

use std::path::Path;
use std::process::Command;

/// A hex SHA-256 digest is 64 characters.
const DIGEST_LEN: usize = 64;

/// Hash `path` with whichever of the platform's digest tools is present.
///
/// `shasum` ships with macOS (and with perl on most Linux images),
/// `sha256sum` with GNU coreutils, and `openssl` with almost everything else.
/// Returns the lowercase hex digest.
pub(crate) fn of_file(path: &Path) -> Result<String, String> {
  let attempts: [(&str, &[&str]); 3] = [
    ("shasum", &["-a", "256"]),
    ("sha256sum", &[]),
    ("openssl", &["dgst", "-sha256", "-r"]),
  ];
  let mut last = String::from("no sha256 tool is available");
  for (tool, args) in attempts {
    // `--` first, so a path starting with `-` is an operand and not a flag.
    let out = match Command::new(tool).args(args).arg("--").arg(path).output() {
      Ok(out) => out,
      Err(e) => {
        last = format!("{tool}: {e}");
        continue;
      }
    };
    if !out.status.success() {
      last = format!("{tool} failed ({})", out.status);
      continue;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if let Some(digest) = first_digest(&text) {
      return Ok(digest);
    }
    last = format!("{tool} produced no digest");
  }
  Err(last)
}

/// The first 64-hex-character token in `text`, lowercased.
///
/// Every one of these tools prints `<digest>  <path>`, but the path is
/// attacker-adjacent (it's a filename), so this looks for the digest shape
/// rather than splitting on whitespace and trusting the first field.
pub(crate) fn first_digest(text: &str) -> Option<String> {
  text
    .split(|c: char| c.is_whitespace() || c == '*')
    .find(|token| token.len() == DIGEST_LEN && token.bytes().all(|b| b.is_ascii_hexdigit()))
    .map(|token| token.to_ascii_lowercase())
}

/// The digest recorded for `name` in a checksum file's contents.
///
/// Handles both shapes a release publishes: a lone digest in `asset.sha256`,
/// and a `SHA256SUMS`-style listing of `<digest>  <name>` lines. A listing that
/// doesn't mention `name` is not a match — falling back to "the only digest in
/// the file" would happily verify the wrong asset.
pub(crate) fn find(body: &str, name: &str) -> Option<String> {
  let lines: Vec<&str> = body
    .lines()
    .filter(|line| !line.trim().is_empty())
    .collect();
  for line in &lines {
    let mentions = line
      .split(|c: char| c.is_whitespace() || c == '*')
      .any(|token| token.trim_start_matches("./") == name);
    if mentions {
      if let Some(digest) = first_digest(line) {
        return Some(digest);
      }
    }
  }
  // A file holding nothing but one digest is the `asset.sha256` convention;
  // there is no ambiguity about which asset it belongs to.
  if lines.len() == 1 {
    let only = lines[0].trim();
    if only.len() == DIGEST_LEN {
      return first_digest(only);
    }
  }
  None
}

/// Whether two hex digests match, compared case-insensitively. Anything that
/// isn't digest-shaped fails, so an empty string can never verify anything.
pub(crate) fn matches(a: &str, b: &str) -> bool {
  a.len() == DIGEST_LEN && b.len() == DIGEST_LEN && a.eq_ignore_ascii_case(b)
}

#[cfg(test)]
mod tests {
  use super::*;

  const A: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
  const B: &str = "0000000000000000000000000000000000000000000000000000000000000000";

  #[test]
  fn reads_the_digest_out_of_each_tool_format() {
    // shasum / sha256sum
    assert_eq!(
      first_digest(&format!("{A}  update.AppImage\n")).as_deref(),
      Some(A)
    );
    // sha256sum in binary mode
    assert_eq!(
      first_digest(&format!("{A} *update.AppImage\n")).as_deref(),
      Some(A)
    );
    // openssl -r
    assert_eq!(
      first_digest(&format!("{A} *./update.AppImage\n")).as_deref(),
      Some(A)
    );
    // openssl without -r puts the label first, which must not be mistaken
    // for the digest.
    assert_eq!(
      first_digest(&format!("SHA256(update.AppImage)= {A}")).as_deref(),
      Some(A)
    );
  }

  #[test]
  fn rejects_anything_that_is_not_a_digest() {
    assert_eq!(first_digest(""), None);
    assert_eq!(first_digest("not a digest at all"), None);
    // 63 and 65 characters are not sha256.
    assert_eq!(first_digest(&A[..63]), None);
    assert_eq!(first_digest(&format!("{A}f")), None);
    // Right length, wrong alphabet.
    assert_eq!(first_digest(&"z".repeat(64)), None);
  }

  #[test]
  fn a_listing_matches_the_named_asset_only() {
    let body = format!("{A}  Acme-x86_64.AppImage\n{B}  Acme-aarch64.AppImage\n");
    assert_eq!(find(&body, "Acme-x86_64.AppImage").as_deref(), Some(A));
    assert_eq!(find(&body, "Acme-aarch64.AppImage").as_deref(), Some(B));
    // An asset the listing never mentions must not borrow another's digest.
    assert_eq!(find(&body, "Acme.dmg"), None);
  }

  #[test]
  fn a_lone_digest_file_belongs_to_its_asset() {
    assert_eq!(find(&format!("{A}\n"), "Acme.AppImage").as_deref(), Some(A));
    // …but a lone digest that is malformed is not a pass.
    assert_eq!(find("not-a-digest\n", "Acme.AppImage"), None);
  }

  #[test]
  fn a_listing_with_one_entry_still_has_to_name_the_asset() {
    // One line, but it is a listing rather than a bare digest — the name
    // has to match or there is nothing tying it to what was downloaded.
    let body = format!("{A}  something-else.AppImage\n");
    assert_eq!(find(&body, "Acme.AppImage"), None);
  }

  #[test]
  fn digests_compare_case_insensitively_and_by_length() {
    assert!(matches(A, &A.to_ascii_uppercase()));
    assert!(!matches(A, B));
    assert!(!matches(A, &A[..63]));
    assert!(!matches("", ""));
  }
}
