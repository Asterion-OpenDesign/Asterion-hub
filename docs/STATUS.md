# Status (2026-10-17)

What exists, what was run and what happened, what is missing. Nothing here claims more than was done.

## Done and verified on Windows 11 (the developer machine)

| Area | Evidence |
|---|---|
| Engine unit tests | `cargo test -p asterion-hub-core`: **60 passed**. Manifest validation (15+ invalid-metadata cases, schema/unknown values), selection (OS, architecture, minimum OS, minimum Hub, withdrawn, fallback), signatures (tampered byte, stranger key, wrong root, cross-document replay, revoked key, expired document and key window, wrong role, garbage signatures, signed-but-invalid), catalog (cache, offline, rollback, same sequence with other bytes, wrong channel, tampered cache, expired cache, revocation, key-set rollback), downloads against a scripted HTTP server (progress, hash mismatch, longer-than-declared body, interrupted + resumed with a Range request, server that ignores Range, poisoned partial file, 404, dead server, URLs outside the policy, redirect to a forbidden host, cancel, reuse of a verified file, swap detection), network policy, store (atomic writes, corrupt files, history bound, torn lines, orphan journal), process detection, free space. |
| Publisher tests | `cargo test -p asterion-publish`: **3 passed**. Keys, key set, release file, beta-only start, promotion, withdrawal, signed catalog, the Hub's verifier accepting it and rejecting a tampered catalog and a wrong root; secret keys refused inside a git repository or over another key; invalid sources refused before anything is signed. |
| **End-to-end with a real MSI** | `cargo test -p asterion-hub-core --test e2e_windows -- --ignored`: **passed**. Over real HTTP, with a real two-tier key set: catalog verified; install (per-user, no UAC, real progress events through every phase); the system reports the install; a newer release is detected; **update refused while the app is running** and nothing changed; update; repair (a deleted file comes back); a release whose hash does not match is refused and the installed version stays intact; an MSI whose properties differ from the catalog is refused; uninstall; history has the 7 entries. Uses a throw-away product ("ASTERION Hub Test App", own UpgradeCode) built by `tests/fixtures/build.ps1`, never a real application. |
| Interface gates | `pnpm --dir ui test`: 58 contrast checks across both themes meet WCAG AA, plus 11 tests of the formatters and of the catalog filtering/sorting/grouping (including 5,000 apps filtered in under 500 ms); `pnpm --dir ui build` typechecks and builds. |
| Interface v2 (see docs/UX.md) | Redesigned with the shared ASTERION identity (dark default, light available): command palette, catalog with filters, categories, sort, grid/list, hidden-unavailable switch, app pages with tabs, launcher, update slabs. Run for real against a local signed catalog of 22 entries (20 are clearly-labelled test entries in a git-ignored folder), photographed at 1180x760 and at the 956x640 minimum, dark and light. Install, update and uninstall of the test product were driven through the window again after the redesign. |
| The Hub window | Run for real (debug build) against a local signed catalog, driven through the WebView (DevTools protocol) and photographed: home, details drawer, tasks, history, light and dark themes, offline banner with saved data. Through the window: **install, update and uninstall of the test product worked**, history recorded them. |
| Detection of an existing install | The machine already had ASTERION BIM 0.1.0 installed by something other than the Hub. The Hub listed it as installed, found its folder through the catalog's `search_dirs` (the installer records no location), reported it **"En ejecución"** while it was open, and offered no action that could replace files in use. No real application was installed, updated, repaired or removed by any test. |
| Hub installer (Windows) | `tauri build` produced an NSIS `ASTERION Hub_0.1.0_x64-setup.exe` (2.0 MiB, per-user, no UAC). A silent install into a temporary folder, a run of the installed release build (it correctly shows the honest "no catalog yet" state, because the production catalog URL does not exist yet), and a silent uninstall (registry entry and Start-menu shortcut removed) were all executed. The release build used the GNU toolchain. |
| macOS code | `cargo check -p asterion-hub-core --target aarch64-apple-darwin` compiles. **It has not been run on a Mac.** |

## Not done / not verified

* **macOS**: nothing run. Checklist for a real Mac: (1) build and open the Hub; (2) publish a `zip` or `dmg` containing one `.app` for `macos/aarch64`; (3) install, check `~/Applications`; (4) open, update, uninstall; (5) note exactly what Gatekeeper says for the Hub and for the installed app. ASTERION BIM has **no macOS build**, so the catalog correctly lists it as unavailable there.
* **Nothing is published.** No GitHub Pages site, no release asset, no catalog on the internet. The production Hub points at `https://asterion-opendesign.github.io/Asterion-hub/`, which does not exist yet, so a release build shows "sin conexión" until it is created (docs/INFRASTRUCTURE.md, "what still has to be created").
* **Hub installer**: not code-signed (no certificate), so SmartScreen may warn. Hub self-update is not implemented, so a new Hub version is installed by hand for now.
* **CI workflows** (`.github/workflows`) were written but **never executed**.
* **Hub self-update**: designed (SECURITY.md), not implemented.
* **All-users installs** (elevated through UAC): implemented, **not exercised** (a UAC prompt cannot be answered by an automated test). Per-user is the default and is what was tested.
* **Dependencies between apps** (`requires_apps`) and **disk-space checks** are implemented; only the "satisfied" path is exercised indirectly. No unit test for the unmet case yet.
* **Production keys**: the root and release keys in use were generated on the developer machine during this work. Before the first public release, generate production keys on a trusted machine, keep the root offline, and replace `trust/roots.json` and `keys/` (see SECURITY.md). Until then the catalog is only trustworthy as a development catalog.
* The Windows `.exe` Hub was built with the **GNU** Rust toolchain (the only one installed). Tauri documents MSVC as the supported Windows toolchain, and the linker reported a manifest-merge warning. The Hub ran correctly; for releases build with MSVC in CI.
* Accessibility was designed in (contrast gate, focus rings, `aria` roles, forced-colors) but not audited with a screen reader.

## Decisions

| Decision | Reason |
|---|---|
| Tauri 2 + Vite/React in the system WebView; engine in Rust | One small shell for Windows and macOS; the interface stays plain web; the logic is testable without a window. |
| Static **signed** catalog, no API server | Costs nothing, nothing to attack, works offline from cache, and a new app is a file, not a deploy. |
| Two-tier Ed25519 keys (offline root, CI release key) | A leaked CI key is recoverable without shipping a new Hub. |
| GitHub Pages + Releases for hosting | Free; the AWS credentials found belong to another product and must not be reused. |
| Per-user installs by default | No administrator rights unless the user asks. |
| Scope read from Windows Installer, not from the registry hive | A per-user MSI is listed under HKLM too; the test caught this. |
| Downloaded files named by hash, never by URL | A hostile name cannot steer the file. |
| `start_task` is an async command | The first GUI click crashed the Hub because the task was spawned outside the runtime; the engine now returns an error instead of panicking and the command runs inside the runtime. |

## Costs and services

Recurring: **$0** (GitHub free tier, public repository). Optional later: code-signing certificate or Apple Developer Program (the only things that remove OS warnings), S3/CloudFront if GitHub's limits are outgrown, a small API only if dynamic features appear. Details in INFRASTRUCTURE.md.

## Exact steps

```powershell
# one-time on this machine (Windows): Rust, Node 22+, pnpm, WebView2 runtime (part of Windows)
cd ui; pnpm install; cd ..
cargo test -p asterion-hub-core -p asterion-publish          # 60 + 3 tests
pnpm --dir ui test                                           # contrast gate + formatters
# end-to-end with a real MSI (installs and removes a throw-away product, per-user):
powershell -File tests/fixtures/build.ps1 -Wix <path\to\wix.exe>
cargo test -p asterion-hub-core --test e2e_windows -- --ignored --nocapture
# run the Hub against a local signed catalog: see "Local end-to-end" in docs/PUBLISHING.md
```

## Files

Created in this repository: everything. Key paths: `crates/hub-core/src/{manifest,trust,catalog,download,net,msi_info,store,engine}.rs`,
`crates/hub-core/src/platform/{windows,macos,other}.rs`, `crates/hub-core/tests/e2e_windows.rs`,
`crates/hub-publish/src/main.rs`, `crates/hub-publish/tests/cli.rs`, `src-tauri/`, `ui/`, `catalog-src/`, `trust/roots.json`,
`keys/`, `tests/fixtures/`, `docs/`, `.github/workflows/`.
