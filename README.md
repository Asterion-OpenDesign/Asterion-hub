# ASTERION Hub

A small desktop application to discover, install, update, repair and uninstall the applications of the
ASTERION ecosystem, from one place, without visiting a download page.

* **Windows** is implemented and has been run on a real machine (see [docs/STATUS.md](docs/STATUS.md)).
* **macOS** is implemented but **has not been run on a Mac yet**. Do not rely on it until the checklist
  in `docs/STATUS.md` has been executed on real hardware.
* The applications stay independent: they run without the Hub, and the Hub never runs in the background
  once it is closed.

## How it is built

| Layer | Technology | Why |
|---|---|---|
| Window | **Tauri 2** (Rust) with the system WebView: WebView2 on Windows, WKWebView on macOS | One small native shell for both systems, no bundled browser. |
| Interface | **Vite + React + TypeScript** in `ui/` | Presentation only. It never decides anything and shows no invented data. |
| Engine | Rust, crate `crates/hub-core` | Catalog, signatures, resumable verified downloads, install/detect/uninstall, state. Fully unit-tested without a window. |
| Publisher | Rust, crate `crates/hub-publish` (`asterion-publish`) | Keys, release files, signed catalog, verification gate, local test server. Never shipped inside the Hub. |

The catalog is a **static, signed JSON document**. There is no API server to run or pay for, and adding an
application or a version means publishing files, not rebuilding the Hub. See
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/SECURITY.md](docs/SECURITY.md),
[docs/CATALOG_FORMAT.md](docs/CATALOG_FORMAT.md), [docs/PUBLISHING.md](docs/PUBLISHING.md) and
[docs/INFRASTRUCTURE.md](docs/INFRASTRUCTURE.md).

## Honest limits

* Hashes and signatures here prove **integrity** and **who published the catalog**. They are **not** Windows
  Authenticode signatures or Apple notarization, which cost money. Installers distributed this way make
  Windows SmartScreen and macOS Gatekeeper show their own warnings. The Hub never disables or works around
  either (see [docs/SECURITY.md](docs/SECURITY.md)).
* Nothing is published yet: the first catalog, the hosting and the first Hub installer are still to be created
  (see [docs/STATUS.md](docs/STATUS.md)).

## Build and run

Requirements: Rust (stable), Node 22+, pnpm, and on Windows the Edge WebView2 runtime (already part of
Windows 10/11). Tauri officially builds on Windows with the MSVC toolchain; CI uses it. Local builds with the
`x86_64-pc-windows-gnu` toolchain were used during development and work.

```powershell
cd ui; pnpm install; cd ..
cargo test --workspace                 # engine + publisher tests
pnpm --dir ui test                     # contrast gate (WCAG) for the design tokens
ui/node_modules/.bin/tauri dev      # run the Hub (starts Vite); the CLI must run from this folder
ui/node_modules/.bin/tauri build    # installer bundle (use tauri.cmd on Windows)
```

To try the whole pipeline against a local catalog, follow "Local end-to-end" in
[docs/PUBLISHING.md](docs/PUBLISHING.md). A debug build can read a local catalog through
`ASTERION_HUB_DEV_CATALOG_URL`; release builds ignore that variable.

## Licence

Apache-2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE).
