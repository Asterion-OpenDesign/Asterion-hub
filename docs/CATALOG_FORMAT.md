# Catalog format (schema 1)

Published as static files:

```
<base>/keys.json            <base>/keys.json.sig          delegated release keys, signed by the root key
<base>/stable/catalog.json  <base>/stable/catalog.json.sig
<base>/beta/catalog.json    <base>/beta/catalog.json.sig
```

`*.sig` is `{ "keyid": "...", "alg": "ed25519", "sig": "<base64 of 64 bytes>" }`, computed over
`DOMAIN || exact file bytes` (see [SECURITY.md](SECURITY.md)). Never re-serialise a signed file.

Unknown **fields** are ignored (additive changes inside schema 1 do not need a new Hub). Unknown **enum values**
(a new OS, architecture or package kind) make that artifact invisible to older Hubs without failing the document.
Anything structurally wrong rejects the whole document.

## catalog.json

```jsonc
{
  "schema": 1,
  "channel": "stable",                  // must equal the channel asked for
  "sequence": 1792265556,               // strictly increasing; the Hub refuses to go back
  "generated_at": "2026-10-17T19:32:36Z",
  "expires_at":   "2026-11-16T19:32:36Z",
  "min_hub_version": null,              // optional semver for the whole catalog
  "apps": [ { /* app */ } ]
}
```

### app

| Field | Notes |
|---|---|
| `id` | `[a-z0-9][a-z0-9-]{0,39}`, stable forever. |
| `name`, `summary`, `description`, `publisher` | Length limited. `summary` ≤ 280, `description` ≤ 4000. |
| `homepage` | Optional, https. The only external link the Hub will open for the app. |
| `license` | Optional text. |
| `category`, `tags` | Optional. `category` (1-40 characters) groups the app in the Hub's catalog; up to 8 `tags` of up to 24 characters feed the search. Older Hubs ignore both. |
| `detect.windows` | `{ "display_name": "...", "publisher": "..." }`: how to recognise an install in *Apps & features* (exact `DisplayName`, optional `Publisher`). |
| `detect.macos` | `{ "bundle_id": "..." }`. |
| `releases[]` | See below. |

### release

| Field | Notes |
|---|---|
| `version` | Semver. Unique per app. |
| `released_at` | RFC 3339. |
| `notes[]` | Short strings shown as "Novedades" (≤ 50 entries, ≤ 500 characters). |
| `withdrawn` | A reason string pulls the release: it is never offered to install or as an update target. |
| `min_hub_version` | Optional semver. A release needing a newer Hub is skipped; the app shows why. |
| `requires_apps[]` | `{ "id": "...", "min_version": "..." }` dependencies on other catalog apps. |
| `artifacts[]` | One per OS + architecture. |

### artifact

| Field | Notes |
|---|---|
| `os` | `windows`, `macos`. |
| `arch` | `x86_64`, `aarch64`. |
| `os_min` | Optional dotted version. Windows: `10.0.<build>`; macOS: `14.0`. |
| `kind` | `msi` (Windows), `dmg` or `zip` (macOS; one `.app` inside). |
| `url` | https; host must be on the Hub's allow-list. No credentials. |
| `size`, `sha256` | Exact byte size (≤ 4 GiB); 64 lowercase hex characters. |
| `installed_size_mb` | Optional, for the free-space check. |
| `launch` | Optional relative path of the executable inside the install directory (cannot escape it). |
| `msi` | Required for `msi`: `product_name`, `upgrade_code` (braced GUID), `product_version`, and optionally `install_dir_property` (upper-case MSI property such as `INSTALLFOLDER`). The Hub re-reads these from the downloaded file and refuses a mismatch. |

## keys.json

```jsonc
{
  "schema": 1,
  "sequence": 1792265493,
  "generated_at": "...", "expires_at": "...",     // one year by default
  "keys": [ { "id": "release-2026-1", "alg": "ed25519", "public": "<base64 32 bytes>",
              "roles": ["catalog"], "not_before": "...", "not_after": "..." } ],
  "revoked": []                                     // ids that must never be accepted again
}
```

## Source tree (what is committed)

```
catalog-src/apps/<id>/app.json
catalog-src/apps/<id>/releases/<version>.json    // adds "channels": ["beta"] or ["beta","stable"]
```

`asterion-publish build --channel stable` selects the releases whose `channels` contain `stable`, validates the
result with the Hub's own parser and signs it.
