# Infrastructure audit and decisions

Audited on 2026-10-17 from the developer machine with read-only commands. Nothing was created, changed or
published in any external service. No credentials appear here or anywhere in the repository.

## What exists

| Service | Finding | Decision |
|---|---|---|
| **GitHub** | The CLI is signed in with `repo`, `workflow`, `read:org` scopes. The `Asterion-OpenDesign` organisation exists on the **free plan**. At audit time it held one private application repository; the public `Asterion-hub` repository was created empty afterwards. | **Use.** Hosts the code, the static catalog (GitHub Pages) and the packages (Releases). Public repositories get unlimited Actions minutes and free Pages; private repositories on the free plan have a monthly minute quota in which macOS minutes count ten times and Windows minutes twice (check the current figures in GitHub's billing documentation). |
| **AWS** | The only credentials on the machine belong to an IAM user that belongs to **another product's backend**. They can read and write one bucket that holds that product's customer files and backups. They cannot list buckets or CloudFront distributions (access denied). Whether the bucket is public is unknown. | **Do not use these credentials or this bucket** for ASTERION: it would mix a different product's customer data with public installers and give the publishing pipeline credentials that can touch backups. If S3/CloudFront is wanted later: a *new* bucket and a *new* IAM user whose policy is `s3:PutObject` on that bucket only, kept in CI secrets. |
| **Railway** | Signed in; one project, belonging to the other product's backend. | **Not needed.** The catalog is a static signed file; a server would add cost and an attack surface for no benefit. |
| **Vercel** | CLI installed; the account check timed out, so nothing was audited. | **Not needed** for the MVP. A public site or docs can go on Vercel or Pages later. |
| Local toolchain | Rust (GNU), Node 24, pnpm 11, WebView2 runtime, WiX 5. No MSVC. No macOS machine. | Release builds in CI (MSVC on Windows, a macOS runner). |

## Chosen architecture and cost

```
GitHub Pages       static signed catalog + keys   $0
GitHub Releases    packages (public, CDN-backed)   $0   (2 GB per file limit)
GitHub Actions     build, verify, publish          $0 on a public repository
```

Total recurring cost: **$0**.

Potential future costs, none required now:

* Windows code-signing certificate or a managed signing service; Apple Developer Program (annual fee) for Developer ID
  signing and notarization. These remove OS warnings; nothing else does.
* S3 + CloudFront if bandwidth or file sizes outgrow GitHub Releases (CloudFront has an always-free monthly allowance;
  S3 storage and requests are billed). Needs a new bucket and a dedicated least-privilege user.
* A small API (Railway) only if a dynamic feature appears (per-licence catalogs, download statistics).

## What still has to be created by a person with the right access

1. Enable **GitHub Pages** (source: GitHub Actions) on `Asterion-OpenDesign/Asterion-hub`.
2. Create the **`release` environment** with a required reviewer, and the secret `ASTERION_RELEASE_KEY`.
3. Generate the production keys offline (see PUBLISHING.md) and commit the **root public key** to `trust/roots.json`.
   The key currently in `trust/roots.json` was generated during development on the developer machine: replace it
   with one generated on a trusted machine before the first public release, and move its private half offline.
4. Upload the first package as a release asset, add its release file and let the workflow publish.
5. Decide about code signing (see SECURITY.md).
