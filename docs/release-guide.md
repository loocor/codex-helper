# Release

## Local build

Unsigned DMG (default for development):

```bash
bun run build:app
```

## GitHub Release

Push a tag such as `v0.1.0`. The **Release** workflow builds both architectures, signs, notarizes, and uploads DMGs to GitHub Releases.

Configure these repository secrets for signed and notarized macOS releases.
Secret names must match exactly:

| Secret                         | Purpose                                     |
| ------------------------------ | ------------------------------------------- |
| `APPLE_CERTIFICATE_P12_BASE64` | Developer ID `.p12` (base64)                |
| `APPLE_CERTIFICATE_PASSWORD`   | `.p12` password                             |
| `KEYCHAIN_PASSWORD`            | Temporary CI keychain password              |
| `APPLE_SIGNING_IDENTITY`       | Codesign identity string                    |
| `APPLE_API_KEY`                | Notarization (App Store Connect API key ID) |
| `APPLE_API_ISSUER`             | API issuer UUID                             |
| `APPLE_API_PRIVATE_KEY`        | Contents of `AuthKey_*.p8`                  |

The App Store Connect API key trio (`APPLE_API_KEY`, `APPLE_API_ISSUER`,
`APPLE_API_PRIVATE_KEY`) is the preferred notarization path. The workflow also
supports Apple ID notarization with `APPLE_ID`, `APPLE_PASSWORD`, and
`APPLE_TEAM_ID`; configure one complete notarization set, not a partial mix.

Before pushing a release tag, confirm the repository has these secrets configured:

```bash
gh secret list --repo loocor/codex-helper
```

The release workflow fails before keychain import when any signing secret is missing, so an empty or incomplete secret set must be fixed in GitHub before a signed release can build.
