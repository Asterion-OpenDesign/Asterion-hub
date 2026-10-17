# Publishing

## The flow

1. **Prepare** a version in the application's own repository.
2. **Build** it for each supported system (the application's CI; for ASTERION BIM today: its WiX script, Windows only).
3. **Generate artifacts and hashes**: `asterion-publish release-add` computes size and SHA-256 and reads the MSI facts.
4. **Checks**: the publisher's parser validates the catalog; `asterion-publish verify --online` downloads every
   artifact from its public URL and re-checks size, SHA-256 and MSI facts. A missing, incomplete or wrong upload
   fails here.
5. **Publish the package** (GitHub Release asset, or S3/CloudFront later).
6. **Publish the manifest** (signed `catalog.json` for `beta`, then for `stable` after promotion).
7. The catalog lists the version; 8. the Hub detects the update on its next check; 9. the user installs it.

Nothing reaches `stable` automatically: a release is added to `beta`, verified, and **promoted** by a separate,
reviewed change. A build that failed never gets a source file, so it never appears in any catalog.

## One-time setup

```powershell
# keys live OUTSIDE the repository (keygen refuses to write inside a git working tree)
$k = "$env:USERPROFILE\.asterion\publishing-keys"
asterion-publish keygen --id root-2026-1    --secret-out $k\root.key    --public-out $k\root.pub.json
asterion-publish keygen --id release-2026-1 --secret-out $k\release.key --public-out $k\release.pub.json
asterion-publish keyset --root-key $k\root.key --delegate $k\release.pub.json --out-dir site --valid-days 365
# put the ROOT public key in trust/roots.json (committed), move root.key OFFLINE,
# store release.key's content as the GitHub secret ASTERION_RELEASE_KEY (environment "release").
```

Then enable GitHub Pages (source: GitHub Actions) on the repository and create the `release` environment with a
required reviewer. Publish `keys.json` and `keys.json.sig` at the root of the Pages site.

## Adding a release

```powershell
asterion-publish release-add --src catalog-src --app asterion-bim --version 0.2.0 `
  --file ASTERION-BIM-0.2.0.msi `
  --url https://github.com/Asterion-OpenDesign/Asterion-hub/releases/download/asterion-bim-v0.2.0/ASTERION-BIM-0.2.0.msi `
  --os-min 10.0.17763 --launch asterion-shell.exe --installed-size-mb 63 --install-dir-property INSTALLFOLDER `
  --note "What changed" --channel beta
asterion-publish promote --src catalog-src --app asterion-bim --version 0.2.0 --to stable   # later, after beta
asterion-publish withdraw --src catalog-src --app asterion-bim --version 0.2.0 --reason "..." # pull a bad release
asterion-publish build  --src catalog-src --channel beta --release-key <path> --out site
asterion-publish verify --site site --roots trust/roots.json --channel beta --hub-config src-tauri/hub.config.json --online
```

A new **application** is a new `catalog-src/apps/<id>/app.json` plus its first release; the Hub is not rebuilt.

## Automation (GitHub Actions)

* `.github/workflows/ci.yml`: tests on Windows, macOS and Linux for every push and pull request.
* `.github/workflows/publish-catalog.yml`: on changes to `catalog-src/**`, builds and signs both catalogs (the key is
  only available to the protected `release` environment), runs the online verification gate, and only then deploys to
  Pages.
* `.github/workflows/release-hub.yml`: builds the Hub installer for Windows and macOS and attaches it to a release.

These workflows were written but **have not been executed** (nothing has been published and the repository settings
above do not exist yet). Expect to fix small things on the first run.

## Local end-to-end (no hosting needed)

```powershell
cargo build -p asterion-publish
$x = "target\debug\asterion-publish.exe"
mkdir .dev\src\apps\asterion-bim; copy catalog-src\apps\asterion-bim\app.json .dev\src\apps\asterion-bim\
& $x keyset --root-key <root.key> --delegate <release.pub.json> --out-dir .dev\site
& $x release-add --src .dev\src --app asterion-bim --version 0.1.0 --file <pkg.msi> `
    --url http://127.0.0.1:8787/files/<pkg.msi> --channel stable --launch asterion-shell.exe --install-dir-property INSTALLFOLDER
& $x build --src .dev\src --channel stable --release-key <release.key> --out .dev\site
& $x serve --dir .dev\site            # http://127.0.0.1:8787
$env:ASTERION_HUB_DEV_CATALOG_URL = "http://127.0.0.1:8787/"; ui/node_modules/.bin/tauri dev
```

`ASTERION_HUB_DEV_CATALOG_URL` and loopback `http://` exist only in debug builds.
