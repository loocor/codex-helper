# Release

## Local build

Unsigned DMG (default for development):

```bash
bun run build:app
```

## GitHub Release

Push a tag such as `v0.1.0`. The **Release** workflow builds both architectures, signs, notarizes, and uploads DMGs to GitHub Releases.
The release entry uses GitHub-generated release notes based on the tag comparison range selected by GitHub.

Configure the repository secrets used by signed and notarized macOS releases.
Secret names must match exactly:

| Secret                               | Purpose                                     |
| ------------------------------------ | ------------------------------------------- |
| `APPLE_CERTIFICATE_P12_BASE64`       | Developer ID `.p12` (base64)                |
| `APPLE_CERTIFICATE_PASSWORD`         | `.p12` password                             |
| `KEYCHAIN_PASSWORD`                  | Temporary CI keychain password              |
| `APPLE_SIGNING_IDENTITY`             | Codesign identity string                    |
| `APPLE_TEAM_ID`                      | Apple Developer Team ID                     |
| `APPLE_API_KEY`                      | Notarization (App Store Connect API key ID) |
| `APPLE_API_ISSUER`                   | API issuer UUID                             |
| `APPLE_API_PRIVATE_KEY`              | Contents of `AuthKey_*.p8`                  |
| `TAURI_SIGNING_PRIVATE_KEY`          | Tauri updater private signing key           |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Tauri updater signing key password          |

The App Store Connect API key trio (`APPLE_API_KEY`, `APPLE_API_ISSUER`,
`APPLE_API_PRIVATE_KEY`) is the notarization path used by the release workflow.
`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are only
needed when updater bundle signing is enabled.

Before pushing a release tag, confirm the repository has these secrets configured:

```bash
gh secret list --repo loocor/codex-helper
```

The release workflow fails before keychain import when any signing secret is missing, so an empty or incomplete secret set must be fixed in GitHub before a signed release can build.
