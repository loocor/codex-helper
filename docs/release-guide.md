# Release

## Local build

Unsigned DMG (default for development):

```bash
bun run build:app
```

## GitHub Release

Push a tag such as `v0.1.0`. The **Release** workflow builds both architectures, signs, notarizes, and uploads DMGs to GitHub Releases.

Configure the same repository secrets as MCPMate (names must match exactly):

| Secret                         | Purpose                                     |
| ------------------------------ | ------------------------------------------- |
| `APPLE_CERTIFICATE_P12_BASE64` | Developer ID `.p12` (base64)                |
| `APPLE_CERTIFICATE_PASSWORD`   | `.p12` password                             |
| `KEYCHAIN_PASSWORD`            | Temporary CI keychain password              |
| `APPLE_SIGNING_IDENTITY`       | Codesign identity string                    |
| `APPLE_ID`                     | Notarization (Apple ID)                     |
| `APPLE_PASSWORD`               | App-specific password or keychain profile   |
| `APPLE_TEAM_ID`                | Apple Developer Team ID                     |
| `APPLE_API_KEY`                | Notarization (App Store Connect API key ID) |
| `APPLE_API_ISSUER`             | API issuer UUID                             |
| `APPLE_API_PRIVATE_KEY`        | Contents of `AuthKey_*.p8`                  |

Provide either the Apple ID trio (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`) or the API key trio (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_PRIVATE_KEY`) for notarization.
