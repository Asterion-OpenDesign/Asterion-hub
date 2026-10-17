# Architecture

## What was found before deciding (2026-10-17)

* **The first application, ASTERION BIM**, is a Qt 6 desktop shell (C++, CMake) over Rust libraries, built
  with MinGW. It runs on Windows today. There is **no macOS build** and no macOS packaging. Its existing
  package is a WiX 5 `.msi` (about 19 MB, unsigned, per-machine or per-user, with a major-upgrade rule), produced by
  a script that also installs it silently, runs it from the installed copy and uninstalls it.
  So the packaging pipeline the Hub needs already exists for Windows; nothing had to be invented.
* **The developer machine** has the Rust GNU toolchain, Node, pnpm, the WebView2 runtime, WiX 5. It has no
  MSVC build tools. This matters for Tauri (below).
* **Existing automation**: none that builds or publishes releases. See [INFRASTRUCTURE.md](INFRASTRUCTURE.md).

## Why Tauri 2 with a WebView

The request was a desktop app for Windows and macOS with a Vite interface in a WebView. Options considered:

| Option | Verdict |
|---|---|
| **Tauri 2 (Rust + system WebView)** | Chosen. One small shell for both systems, the interface is plain Vite/React, the engine is Rust (the language the ASTERION kernels already use), updater and bundlers exist for both systems. |
| WPF/.NET + WebView2 (the pattern of an existing setup wizard) | Works well but Windows only; a macOS Hub would need a second codebase. |
| Electron | Cross-platform, but ships a whole browser (~100 MB) for a tool that mostly downloads files. |
| Qt (the ASTERION BIM shell) | Would reuse the stack but not the Vite interface that was asked for, and the Qt runtime would be duplicated. |

Nothing that already works was migrated: the Hub is a new application and ASTERION BIM is untouched.

Tauri's documentation lists the Microsoft C++ Build Tools (MSVC) as the Windows prerequisite. This machine has
only the GNU toolchain, and the Hub builds and runs with it during development. **Release builds should use
the MSVC toolchain** (the CI workflow does); that is the supported configuration.

## Parts

```
ui/                    Vite + React + TypeScript. Paints views, sends commands.
src-tauri/             Tauri shell: wires commands and events to the engine. No logic.
crates/hub-core/       Engine (library). Everything testable lives here.
crates/hub-publish/    asterion-publish CLI. Publisher side only.
catalog-src/           Source of truth for the catalog: one app.json per app, one JSON per release.
trust/roots.json       The public root key compiled into the Hub.
```

The web layer asks, the engine decides. The UI calls commands (`list_apps`, `start_task`, ...) and
listens to one event stream (`hub://event`). Every number on screen (progress, speed, versions, states) is
produced by the engine at that moment; in a plain browser, with no engine, the UI shows an explanation and no data.

### Engine modules (`hub-core`)

* `manifest`: catalog types, strict validation, choosing the newest compatible release for this machine.
* `trust`: Ed25519 verification of the key set and the catalog (two tiers, see SECURITY.md).
* `catalog`: fetch, verify, rollback protection, cache, offline fallback. The cache is re-verified on every load.
* `net`: URL policy (https, allow-listed hosts, redirects checked hop by hop) and the HTTP client.
* `download`: resumable download to `.part`, size and SHA-256 check, atomic rename.
* `msi_info`: reads the MSI's own properties and compares them with what the catalog promised.
* `platform`: Windows (registry detection, `msiexec`, elevation only when asked) and macOS (`.app` bundles).
* `store`: settings, install records, history, journal, with atomic writes.
* `engine`: the orchestration and the views the UI paints.

## Distribution

```
 publisher (CI, release key)           static hosting (GitHub Pages)         the Hub on the user's machine
 ───────────────────────────           ─────────────────────────────         ─────────────────────────────
 catalog-src/*.json  ──build+sign──►   keys.json(+.sig)                ◄──GET──  verify with embedded root key
 packages ──────────upload────────►    stable|beta/catalog.json(+.sig)           rollback check, cache
                                       packages (GitHub Releases)    ◄──GET──  size + SHA-256, MSI facts, run
```

* **No API server.** A signed static file does the job of a catalog API, costs nothing, cannot be "down" in
  interesting ways (the Hub keeps the last verified copy), and has nothing to attack. Railway is not needed.
  If dynamic features appear later (download counts, per-licence catalogs) a small API can sit beside it.
* **Packages are stored apart from the Hub and from the catalog.** The catalog only holds URLs, sizes and hashes.
  Moving packages to S3/CloudFront later is a change of URL in the catalog, not of the Hub (add the host to the
  Hub's allow-list in a Hub release first).
* **Channels** `stable` and `beta` are separate catalogs. A release enters `beta` and is promoted to `stable`
  by a reviewed change; a failed or incomplete package is caught by the verification gate before either.
* **If hosting is down**, installed applications are unaffected (they never depend on the Hub), and the Hub shows
  the last verified catalog with a banner saying so. A catalog past its expiry date is shown but cannot be used to
  install.

## What happens on "Install"

1. Preconditions: usable (non-expired) catalog; newest release this machine can run (OS, architecture, minimum
   OS version, minimum Hub version); dependencies on other apps; free disk space; app not running (for update,
   repair, uninstall: the Hub does not replace files in use).
2. Download with resume; progress, speed and ETA are real. Cancel keeps the partial file; retry resumes it.
3. SHA-256 and size are verified; for MSI, `ProductName`, `ProductVersion` and `UpgradeCode` are read from the
   file and must match the catalog. Verified again after waiting for the installer lock.
4. A journal entry is written, `msiexec` runs (per-user, no elevation, by default), the journal is removed.
   If the Hub dies in between, the next start records "interrupted" in the history.
5. The system is asked again what is installed; only that answer is stored. History records the outcome.

User data is outside the install directory and is never touched. A failed MSI update rolls back to the previous
version by Windows Installer's own transaction.
