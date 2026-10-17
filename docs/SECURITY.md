# Security model

There is no budget for code-signing certificates, so this design says exactly what it protects and what it
does not. Four different things are easy to confuse:

| # | What | Answers | Provided here? |
|---|---|---|---|
| 1 | **File integrity** (SHA-256) | "Are these bytes the ones the publisher listed?" | Yes. Detects corruption and substitution. |
| 2 | **Publication authenticity** (Ed25519 signature on the catalog) | "Was this list produced by someone holding the ASTERION publishing key?" | Yes, with keys we control. |
| 3 | **Code signing** (Windows Authenticode, Apple Developer ID) | "Does the operating system recognise the publisher?" | **No.** Needs paid certificates. |
| 4 | **Notarization and reputation** (Apple notarization, SmartScreen reputation) | "Has the platform scanned/seen this before?" | **No.** |

A hash published next to a file detects corruption; on its own it does **not** show who published the file,
because whoever can replace the file can replace the hash. That is why the hashes live inside a catalog that is
signed (2), and why the hashes are not presented as a substitute for (3) or (4).

## What a user will still see, and why we do not hide it

* **Windows:** installers without Authenticode can trigger SmartScreen ("Windows protected your PC"). An MSI run by
  the Hub per-user shows no UAC prompt, but SmartScreen/Defender may still evaluate the file. Reputation builds
  with volume; it cannot be bought around without a certificate.
* **macOS:** an unsigned, un-notarized app downloaded in a browser is quarantined, and Gatekeeper blocks it until
  the user opens *System Settings → Privacy & Security → Open Anyway* (macOS 15 removed the Control-click shortcut).
  The Hub itself, downloaded in a browser, will be quarantined the same way. Apple Silicon also requires at least an
  ad-hoc signature on arm64 binaries; the linker adds one by default.
* The Hub **does not** disable antivirus, SmartScreen or Gatekeeper, does not call `spctl`, and does not add or strip
  the `com.apple.quarantine` attribute. Whatever the system decides is the system's decision, and the interface says
  so on the application details panel and in Settings.
* Costs that would remove the warnings: an Apple Developer Program membership (paid, annual) for signing and
  notarization; a Windows code-signing certificate (paid) or a managed signing service. Free alternatives exist for
  open-source projects (for example SignPath Foundation), with their own eligibility rules; evaluate them before
  the first public release. Until then the limitation stands and is documented here.

## Trust chain

```
 root key (Ed25519, OFFLINE)      public half compiled into the Hub: trust/roots.json
    └─ signs keys.json            list of delegated release keys, validity window, revoked ids, sequence
         └─ release key (CI)      signs <channel>/catalog.json
              └─ catalog          every artifact: https URL, size, SHA-256 (+ MSI facts)
                   └─ artifact    downloaded, size and SHA-256 checked, MSI facts re-read from the file
```

* Signatures are over `DOMAIN || exact bytes`, with a different `DOMAIN` for key sets and catalogs, so a signature
  for one document type cannot be replayed as another. The signature is checked **before** the JSON is parsed.
* **Rollback and replay:** both documents carry a strictly increasing `sequence`; the Hub stores the highest it has
  accepted and refuses older ones. The same sequence with different bytes is refused. Documents expire (the catalog
  after 30 days by default, the key set after one year): a stale cached catalog is shown but cannot install.
* **Channel binding:** the channel is inside the signed catalog; a beta catalog served at the stable URL is refused.
* **Invalid metadata:** strict validation (app ids, semver, RFC 3339, 64-hex hashes, size limits, relative-path
  rules, package kind per OS, no credentials in URLs). The publisher runs the same parser before signing.
* **URLs:** https only, to hosts on a compiled-in allow-list, standard port, no credentials, no IP literals;
  redirects are checked at each hop. Logs and errors never contain a query string.
* **Downloaded files:** the file name comes from the SHA-256, never from the URL or the server; bytes are written to
  `.part` and renamed only after size and hash match; a poisoned partial file is deleted, never resumed or reused;
  verified again right before the installer runs.
* **Unexpected packages:** the MSI's own `ProductName`, `ProductVersion` and `UpgradeCode` must equal what the signed
  catalog says. An MSI property name taken from the catalog must be an upper-case identifier (no argument injection).
* **Least privilege:** installs are per-user by default (no administrator rights). Elevation is requested only if the
  user chooses "all users", or to update/remove an existing all-users install. The elevated process is `msiexec` with
  a fixed command line built by the Hub.
* **Web layer:** the WebView has a strict CSP (`default-src 'self'`) and no filesystem, shell or network
  capability; it can only call the Hub's own commands. External links are limited to https, and to homepages that
  appear in the signed catalog.
* **Separation of publishing and consuming:** the Hub binary contains no signing code (it is behind a Cargo feature
  that only `asterion-publish` enables) and no private key. The private release key exists only in the CI secret store.
* **Secrets** are never in the repository; `*.key` is git-ignored and `asterion-publish keygen` refuses to write a
  secret key inside a git working tree or over an existing file.
* **Logs** (`logs/hub.log`) record events, codes and versions, not URLs with queries, tokens or file contents.

## Keys: protection, rotation, compromise

| Key | Where it lives | Rotation |
|---|---|---|
| Root | Offline: an encrypted removable drive or password-manager vault, with a second copy somewhere else. Never on CI, never in the repo. Used about once a year. | Ship a Hub that trusts both the old and the new root in `trust/roots.json`; sign the next `keys.json` with the new one; later drop the old root in another Hub release. |
| Release | CI secret `ASTERION_RELEASE_KEY`, available only to the protected `release` environment (required reviewer). | Generate a new one, add it to `keys.json`, re-sign `keys.json` with the root, update the secret. Old key stays valid until its `not_after` or until revoked. No Hub release needed. |

* **Release key compromised:** with the root key, publish a new `keys.json` that lists the new key and puts the old
  key's id in `revoked`, with a higher `sequence`. Every Hub applies it on its next refresh and rejects catalogs
  signed by the old key. Then re-publish a clean catalog, and review what was published during the exposure window.
* **Root key compromised (or lost):** this is the one case that needs a new Hub. A lost key cannot sign a new
  key set, so users on old Hubs stop accepting catalogs after `keys.json` expires (one year). Mitigation: keep two
  backups, and the key-set expiry is shown in the Hub's Settings with a warning 30 days before it runs out.
  Users must install the new Hub by hand (documented download), which is the same level of trust as the first install.
* The first install of the Hub itself is trusted by the channel it came from (HTTPS to the project's page). From then
  on, everything the Hub does is checked against the key compiled into it.

## Out of scope today

* Hub self-update. Design: the Hub appears in the catalog as an application and is replaced through the same verified
  path, by launching its own installer and exiting; it is not implemented yet.
* Reproducible builds and provenance attestations for the packages (planned with the CI pipeline).
* Protection against an attacker who already controls the user's account (they can edit the Hub's own files).
