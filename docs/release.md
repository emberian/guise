# Releasing

One repository, two things to ship: the **`guise-ui`** library, which goes to
crates.io, and **Tailor**, which goes out as a macOS app. Both take the same
version — the one in the root `[workspace.package]` — and both come off the same
tag.

## Cutting one

1. Write the CHANGELOG section. `## <version> — <date>`, and make it the notes
   you would want to read: `release.yml` lifts that section verbatim, so a list
   of commit subjects is not an option.
2. Bump `version` in the root `Cargo.toml`, then `cargo build --workspace` to
   regenerate `Cargo.lock` — it is committed, and CI builds `--locked`, so a
   bump without the lockfile fails.
3. Commit, tag `v<version>`, push both.

```sh
git tag -a v1.1.0 -m "Version 1.1.0 — …"
git push origin main v1.1.0
```

Pushing to `main` also deploys the site (`pages.yml`, on any change under
`site/` or `docs/`). Pushing the tag runs `release.yml`.

## What the workflow does

1. **`github-release`** — opens the release as a **draft**, with notes from the
   CHANGELOG. Draft, because a published release is what `releases/latest`
   reports: publishing first would advertise a version for the length of a
   notarization run with none of its assets attached.
2. **`tailor-macos`** — builds `dist/Tailor.app` (`scripts/bundle.sh`), signs
   it, notarizes and staples it, packages `Tailor.dmg` (`scripts/dmg.sh`),
   notarizes that too, and uploads it to the draft.
3. **`publish`** — flips the draft live, once the DMG is attached.

`workflow_dispatch` runs the same thing for a tag that already exists — the
release job sees it and skips, the build re-uploads with `--clobber`. That is
the way to re-cut a release after fixing signing, without moving a tag.

## crates.io

Manual, on purpose:

```sh
cargo publish -p guise-ui
```

The library builds against plain crates.io `gpui`, with no patch section, so
this works from a clean checkout. It stays a human step because publishing
cannot be undone — a version can be yanked, never replaced. Nothing about
Tailor reaches crates.io: its six crates are `publish = false`, and
`cargo package -p guise-ui --list` is the proof.

## Signing & notarization

Optional, and gated in two halves, because they need different credentials and
one is usually set up before the other:

| Secret | What it is | Gates |
|--------|------------|-------|
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` | signing |
| `APPLE_CERT_P12` | base64 of the exported Developer ID `.p12` | signing |
| `APPLE_CERT_PASSWORD` | password for that `.p12` | signing |
| `KEYCHAIN_PASSWORD` | any password, for the throwaway CI keychain | signing |
| `APPLE_ID` | Apple ID email for `notarytool` | notarization |
| `APPLE_TEAM_ID` | Apple Developer Team ID | notarization |
| `APPLE_APP_PASSWORD` | app-specific password for that Apple ID | notarization |

Three outcomes, and the `Verify bundle` step says which one you got:

- **Notarized** — all seven. Gatekeeper opens it without a word.
- **Signed, not notarized** — the first four. Gatekeeper still refuses a
  *downloaded* copy: notarization, not the signature, is what clears that.
- **Ad-hoc** — none. It runs on the machine that built it and nowhere else.

The app is signed with a hardened runtime and `assets/tailor.entitlements`;
gpui renders through Metal and JITs shaders, so it needs the JIT and
unsigned-executable-memory entitlements to run under notarization at all.

Setting a secret without putting it in a shell history or a scrollback:

```sh
gh secret set APPLE_APP_PASSWORD --repo wess/guise   # reads from stdin
base64 < cert.p12 | gh secret set APPLE_CERT_P12 --repo wess/guise
```

Exporting the certificate, if you need a fresh `.p12`:

```sh
security export -t identities -f pkcs12 -k login.keychain-db -P "$PW" -o cert.p12
```

That exports every identity in the keychain, which is fine — CI signs with the
name in `APPLE_SIGNING_IDENTITY` and ignores the rest. Delete the file
afterwards; it carries a private key.

## Building the app locally

```sh
scripts/icon.sh      # assets/icon.png + icon.icns, from scripts/icon.swift
scripts/bundle.sh    # dist/Tailor.app   (CODESIGN_IDENTITY to sign it)
scripts/dmg.sh       # dist/Tailor.dmg
```

`bundle.sh` reads the version from `Cargo.toml`, renames the `tailordev` binary
to `tailor`, and puts `tailor-mcp` beside it in the bundle. Regenerate the icon
only when the design changes — the `.icns` is committed.
