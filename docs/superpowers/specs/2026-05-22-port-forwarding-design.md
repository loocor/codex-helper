# Codex Helper Port Forwarding Design

## Goal

Codex Helper should provide lightweight, VS Code-style port forwarding for Codex remote SSH sessions while staying small, local, and auditable. The helper should stabilize its injected Settings surface first, then add a bottom-panel Ports view for daily port forwarding operations.

## Product Shape

Settings remains a policy surface. It should not become a port management table.

The bottom panel gets a new `Ports` entry alongside Codex's native panel entries. The Ports panel shows detected remote web ports, active tunnels, failures, and stale tunnels. Users can open forwarded URLs, copy local URLs, stop tunnels, and manually forward a port.

The default interaction follows the familiar VS Code remote development model:

- A clear web URL detected in a remote Terminal is auto-forwarded.
- The local port defaults to the same number as the remote port.
- A successful forward is shown in the Ports panel and via a small notification.
- If the local port is unavailable, Codex Helper asks for a local port instead of silently choosing a random one.
- Ambiguous port mentions are listed as detected candidates but are not auto-forwarded.

## Scope

In scope:

- Stabilize the existing injected Settings page before adding the Ports UI.
- Add Settings policies for port forwarding.
- Add a bottom-panel `Ports` view.
- Detect web ports from Terminal output.
- Resolve SSH target metadata from Codex's remote context and global state.
- Manage local SSH tunnel child processes from the helper backend.
- Stop helper-managed tunnels when users click Stop or when the helper exits.

Out of scope for this version:

- A persistent user-maintained port mapping table.
- A global random-port forwarding policy.
- Reading SSH private keys.
- Storing SSH credentials or passwords.
- Installing remote helper scripts.
- Scanning remote processes independently of Codex or Terminal context.
- Implementing an SSH protocol client inside Codex Helper.

## Settings Policies

The Settings page should add a `Port Forwarding` section with a small policy set:

- `Enable port forwarding`: controls all port detection and tunnel creation.
- `Auto-forward detected web ports`: defaults to enabled when port forwarding is enabled.
- `Use the same local port by default`: defaults to enabled.

The Settings UI should not expose a port mapping table. When users need a different local port, they choose it at forwarding time. Codex Helper may remember the most recent local port choice for the same remote host, project, and remote port, but this is a recent choice, not a user-managed rule table.

## Ports Panel

The Ports panel should be the operational surface. It should show rows with:

- Remote port.
- Local URL when forwarded.
- Remote host or project context when available.
- Status: `detected`, `forwarding`, `active`, `failed`, or `stale`.
- Actions: `Open`, `Copy URL`, `Forward`, and `Stop` when applicable.

Panel rows should be deduplicated by remote host id, remote project path, remote port, and local port. Repeated Terminal output should refresh the existing row instead of creating duplicate rows.

## Detection Rules

Auto-forward only clear web service URLs from Terminal output:

- `http://localhost:<port>`
- `http://127.0.0.1:<port>`
- `http://0.0.0.0:<port>`
- `http://[::1]:<port>`

The detector should accept optional URL paths after the port.

Ambiguous text such as `listening on port 5432` or `bound to 6379` should create a detected candidate only if the implementation can do so without false positives. It must not auto-forward ambiguous ports.

Ports must be validated as integers from `1` through `65535`.

## SSH Context and Security Boundary

Codex Helper should reuse the same security posture as the existing Zed remote open support:

- Read the current remote `hostId` and remote project path from Codex renderer context when available.
- Resolve `sshHost`, `sshUser`, and `sshPort` from Codex global state.
- Do not read SSH private keys.
- Do not persist SSH credentials.
- Do not prompt for or store SSH passwords.
- Do not parse `~/.ssh/config` as a primary data source.

The backend should launch the system SSH client and let the user's existing SSH agent, keychain, known hosts, and SSH configuration handle authentication:

```text
ssh -N -L 127.0.0.1:<local_port>:127.0.0.1:<remote_port> <ssh_target>
```

Codex Helper owns only the tunnel process lifecycle and UI state.

## Tunnel Lifecycle

The backend tunnel manager should keep an in-memory registry of helper-managed tunnels.

Lifecycle states:

- `detected`: the renderer found a clear or candidate port.
- `forwarding`: the backend is starting the SSH child process.
- `active`: the local port is listening for this tunnel.
- `failed`: validation, port binding, SSH startup, or context resolution failed.
- `stale`: the source Terminal or remote service appears to have stopped.

Helper-managed tunnels stop when:

- The user clicks Stop.
- The helper exits.
- An auto-forwarded tunnel becomes stale according to conservative detection.

Manual tunnels should remain active until the user stops them or the helper exits. This avoids stopping a user-requested tunnel just because Terminal output is no longer visible.

## Settings Injection Stability

Port forwarding depends on reliable injected UI surfaces. Before adding the Ports panel, Codex Helper should harden the Settings injection:

- Stop treating arbitrary `div` nodes as Settings sidebar items.
- Separate sidebar detection, insertion parent selection, and content root selection.
- Add an explicit helper active state.
- Clear the helper page when users click native Settings entries.
- Log explicit diagnostics when a stable insertion point or content root is unavailable.
- Never render the helper page into a guessed General content subgroup.

The hardened Settings behavior should be covered by DOM fixture tests before adding more runtime UI.

## Error Handling

Failures should be visible and explicit:

- Missing Codex remote context: show a failed row with an explanation.
- Missing SSH target metadata: show a failed row with an explanation.
- Local port already in use: ask the user to choose another local port.
- SSH startup failure: show stderr or a concise startup error in the row.
- Bridge route failure: show the backend message and log diagnostics.

Codex Helper must not silently fall back to a different local port, different SSH host, or unrelated UI surface.

## Testing Strategy

Tests should cover:

- Settings sidebar and content root fixture detection.
- Terminal URL port parsing.
- Port validation.
- SSH target reuse from Codex global state.
- Tunnel registry state transitions.
- Duplicate detection updates an existing row.
- Local port conflict returns a visible failure.

Manual validation should cover:

- Codex Settings does not mix with General after repeated navigation.
- A remote Terminal web URL creates a Ports row.
- Auto-forward uses the same local port by default.
- `Open`, `Copy URL`, and `Stop` work from the Ports panel.
- Helper exit stops active helper-managed tunnels.
