# Interface and UX

The Hub has to stay usable with two applications and with two hundred. This is what that implies, and
what was built.

## Visual language

The shared ASTERION identity: one variable family (Archivo, the "machine voice" is the same font stretched
with tabular figures), a dark "noche" floor as the default and a light "papel" theme on the same token names,
elevation made of translucent planes (`lavado-1/2/3`), selection as a "realce", and the blue spent only where it
means something: the primary action, the current place, an update waiting. Large tight display type for the
page's one statement, small tracked uppercase "rótulos" for orientation. Colour carries state:

| Colour | Meaning |
|---|---|
| Blue slab (`losa`) | An update is waiting and wants a decision. |
| Green tile / pill | Installed. |
| Blue-tinted button | Install (quiet, so a list of fifty is not fifty blue blocks). |
| Solid blue button | The one primary action of a screen (Open, Update, Install on an app's own page). |
| Amber | Offline / saved data / blocked version. |
| Red | An error, or Uninstall. |

`ui/scripts/check-contrast.mjs` composites the planes and checks 58 foreground/background pairs across both themes
(WCAG 2.x). Disabled is a colour pair, never opacity on text. `forced-colors` (Windows High Contrast) is handled.

## Structure

* **Rail** (7 sections, with live counts for installed, updates, active tasks) and a sticky **top bar** with the
  route (and Back, `Alt + ←`), a search entry (`Ctrl K` or `/`) and the catalog sync state (click to refresh).
* **Command palette** (`Ctrl K`): every app and every action from the keyboard. `Enter` opens an app, `Ctrl + Enter`
  installs/updates/opens it, plus "Check for updates", "Update all", theme switch and the seven sections.
* **Home**: the one statement (updates waiting / all up to date), updates as slabs, a launcher for installed apps
  (the Hub is also where you start them), a few apps to discover, recent activity.
* **Catalog** (built for a long list): text filter (accents and case ignored, every word must match name, summary,
  publisher, category or tag), status filter with counts, category chips with counts, sort (name, newest, status,
  category with group headings), grid or list view. Apps this machine cannot install are **hidden by default**, with
  a single switch that says how many. The view, sort and that switch are remembered. Filtering 5,000 apps takes
  well under a second (tested).
* **App page** instead of a drawer: hero, tabs (Summary, What's new, Requirements and security), a "ficha" with the
  facts, the active task, and Repair/Uninstall tucked behind "···" so they never sit next to the thing most people want.
* **Installed**, **Updates** (with "can't be installed here" listed, not silently ignored), **Tasks** (queue with real
  progress), **History** (day groups, filter by result and app, search), **Settings**.
* Every state is real: loading skeletons, empty states with a next step, offline with saved data, expired catalog,
  errors with a plain explanation and technical details one click away.

## What a large catalog needed from the engine and the format

* `category` and `tags` on an app (additive, schema 1; older Hubs ignore them).
* The installed-apps registry is read **once** per refresh (`SystemApps`) and matched in memory, not once per app;
  the process list is read once. The list is built off the thread that paints the window.
* Event bursts (a running task emits several per second) are coalesced into one reload.

## Not done yet

* Virtualised lists (not needed until the catalog is in the hundreds; filtering is already instant).
* Per-app icons (tiles show letters; an optional icon would have to be fetched and cached under the same signature rules).
* A screen-reader audit.
