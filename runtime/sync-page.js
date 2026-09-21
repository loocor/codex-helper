let syncPeerDialogRoot = null;

const SYNC_MASKED_PASSWORD = "********";

function nativeSettingsSyncPageContent() {
  return `
    ${nativeSettingsGroupSection(
      "This Mac",
      `
        <div class="codex-helper-settings-row">
          <div class="codex-helper-settings-row-copy">
            <div class="codex-helper-settings-row-title" data-codex-helper-sync-self-name>This Mac</div>
            <div class="codex-helper-settings-row-description" data-codex-helper-sync-self-detail>Loading</div>
          </div>
        </div>
        <div class="codex-helper-settings-row">
          <div class="codex-helper-settings-row-copy">
            <div class="codex-helper-settings-row-title">Role</div>
            <div class="codex-helper-settings-row-description">Primary pushes to peers. Replica only applies incoming provider files.</div>
          </div>
          <select class="codex-helper-text-input" ${helperSyncFieldAttribute}="role" aria-label="Sync role">
            <option value="primary">Primary</option>
            <option value="replica">Replica</option>
          </select>
        </div>
        <div class="codex-helper-settings-row">
          <div class="codex-helper-settings-row-copy">
            <div class="codex-helper-settings-row-title">Auto-sync</div>
            <div class="codex-helper-settings-row-description">When this Mac is primary, push providers, OAuth tokens, and the active provider after a switch.</div>
          </div>
          <label class="codex-helper-switch" aria-label="Auto-sync">
            <input type="checkbox" ${helperSyncToggleAttribute}="autoSync">
            <span class="codex-helper-switch-track" aria-hidden="true"><span class="codex-helper-switch-thumb"></span></span>
          </label>
        </div>
      `,
    )}
    <section class="codex-helper-settings-section" data-codex-helper-sync-peers-section>
      <div class="codex-helper-provider-list-header">
        <div class="codex-helper-settings-section-title">Peers</div>
        <button type="button" class="codex-helper-provider-add-button" ${helperCommandAttribute}="sync-peer-add" aria-label="Add peer">${nativeSettingsStandardIconSvg("plus")}</button>
      </div>
      ${nativeSettingsPanel(`
        <div class="codex-helper-settings-scroll" data-codex-helper-sync-peers></div>
        ${nativeSettingsListFooter("data-codex-helper-sync-status")}
      `)}
    </section>
  `;
}

function syncPeerFormBackButton() {
  return `<button type="button" class="helper-settings-back" ${helperCommandAttribute}="sync-peer-cancel">${nativeSettingsStandardIconSvg("chevron-left")}<span>Back</span></button>`;
}

function closeSyncPeerDialog() {
  if (syncPeerDialogRoot?.isConnected) syncPeerDialogRoot.remove();
  syncPeerDialogRoot = null;
}

function requireSyncSettingsHost() {
  const host = helperSettingsContentHost();
  if (!(host instanceof HTMLElement)) {
    throw new Error("Helper Settings content host not found");
  }
  return host;
}

function syncPeerActionButton(command, peerId, label) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = helperActionClass;
  button.setAttribute(helperCommandAttribute, command);
  button.setAttribute("data-codex-helper-sync-peer-id", peerId || "");
  button.textContent = label;
  return button;
}

function syncCommandPeerId(source) {
  return (
    source?.getAttribute("data-codex-helper-sync-peer-id") ||
    source?.closest?.("[data-codex-helper-sync-peer-id]")?.getAttribute("data-codex-helper-sync-peer-id") ||
    syncPeerDialogRoot?.getAttribute("data-codex-helper-sync-peer-id") ||
    ""
  );
}

function returnToSyncList(result) {
  const host = requireSyncSettingsHost();
  renderNativeHelperSettingsPage(host, "sync");
  if (result) renderSync(result);
  else {
    refreshSyncPage().catch((error) => {
      setHelperText("[data-codex-helper-sync-status]", error?.message || String(error));
    });
  }
}

function syncPeerDialogField(name) {
  return syncPeerDialogRoot?.querySelector(`[data-codex-helper-sync-peer-field="${name}"]`);
}


function setSyncPeerDialogValue(name, value) {
  const node = syncPeerDialogField(name);
  if (node instanceof HTMLInputElement || node instanceof HTMLSelectElement) {
    node.value = value;
  }
}

function setSyncPeerDialogError(message) {
  const node = syncPeerDialogRoot?.querySelector("[data-codex-helper-sync-peer-dialog-error]");
  if (node) node.textContent = message || "";
}

function syncPeerAuthFields() {
  const method = syncPeerDialogField("authMethod")?.value || "identity";
  const identityRow = syncPeerDialogRoot?.querySelector("[data-codex-helper-sync-peer-identity-row]");
  const passwordRow = syncPeerDialogRoot?.querySelector("[data-codex-helper-sync-peer-password-row]");
  if (identityRow instanceof HTMLElement) identityRow.hidden = method !== "identity";
  if (passwordRow instanceof HTMLElement) passwordRow.hidden = method !== "password";
}

function openSyncPeerDialog(mode, peer = null) {
  closeSyncPeerDialog();
  const host = requireSyncSettingsHost();
  const title = mode === "edit" ? "Edit peer" : "Add peer";
  const dialog = document.createElement("section");
  dialog.setAttribute(helperNativeSettingsPageAttribute, "sync");
  dialog.className = "codex-helper-native-settings-page helper-settings-page";
  dialog.setAttribute("data-codex-helper-sync-peer-dialog", "true");
  dialog.setAttribute("data-codex-helper-sync-peer-mode", mode);
  if (peer?.id) dialog.setAttribute("data-codex-helper-sync-peer-id", peer.id);
  const authMethod = peer?.authMethod === "password" ? "password" : "identity";
  dialog.innerHTML = `
    <div class="helper-settings-page-inner">
      <header class="helper-settings-page-header">
        ${syncPeerFormBackButton()}
        <h1 class="helper-settings-page-title"></h1>
      </header>
      <div class="codex-helper-provider-dialog-body">
        ${providerFieldRow("Name", `<input data-codex-helper-sync-peer-field="name" placeholder="Mac Mini" aria-label="Peer name">`)}
        ${providerFieldRow("Host", `<input data-codex-helper-sync-peer-field="host" placeholder="mini.sgponte" aria-label="SSH host">`)}
        ${providerFieldRow("User", `<input data-codex-helper-sync-peer-field="user" placeholder="loocor" aria-label="SSH user">`)}
        ${providerFieldRow("Port", `<input data-codex-helper-sync-peer-field="port" type="number" min="1" max="65535" aria-label="SSH port">`)}
        ${providerFieldRow(
          "Auth",
          `<select data-codex-helper-sync-peer-field="authMethod" aria-label="SSH authentication">
            <option value="identity">Identity file</option>
            <option value="password">Password</option>
          </select>`,
        )}
        ${providerFieldRow(
          "Identity file",
          `<input data-codex-helper-sync-peer-field="identityFile" placeholder="~/.ssh/mac.mini_rsa" aria-label="SSH identity file">`,
          { attr: "data-codex-helper-sync-peer-identity-row" },
        )}
        ${providerFieldRow(
          "Password",
          `<input data-codex-helper-sync-peer-field="password" type="password" placeholder="${SYNC_MASKED_PASSWORD}" autocomplete="off" aria-label="SSH password">`,
          { hidden: true, attr: "data-codex-helper-sync-peer-password-row" },
        )}
        <div class="codex-helper-provider-dialog-error" data-codex-helper-sync-peer-dialog-error></div>
      </div>
    </div>
    <div class="codex-helper-provider-dialog-actions">
      ${mode === "edit" ? `<button type="button" class="codex-helper-provider-delete" ${helperCommandAttribute}="sync-peer-delete" data-codex-helper-sync-peer-id="${peer?.id || ""}">Delete</button>` : ""}
      <span class="codex-helper-provider-dialog-spacer"></span>
      <button type="button" ${helperCommandAttribute}="sync-peer-save">Save</button>
    </div>
  `;
  dialog.querySelector(".helper-settings-page-title").textContent = title;
  host.replaceChildren(dialog);
  helperNativeSettingsRoot = dialog;
  helperNativeSettingsContentHost = host;
  helperNativeSettingsActivePage = "sync";
  updateNativeSettingsActiveEntry("sync");
  syncPeerDialogRoot = dialog;
  setSyncPeerDialogValue("name", peer?.name || "");
  setSyncPeerDialogValue("host", peer?.host || "");
  setSyncPeerDialogValue("user", peer?.user || "");
  setSyncPeerDialogValue("port", String(peer?.port || 22));
  setSyncPeerDialogValue("authMethod", authMethod);
  setSyncPeerDialogValue("identityFile", peer?.identityFile || "");
  syncPeerDialogField("authMethod")?.addEventListener("change", () => syncPeerAuthFields());
  syncPeerAuthFields();
}

function syncPeerDialogPayload() {
  if (!(syncPeerDialogRoot instanceof HTMLElement)) {
    throw new Error("Sync peer form is not available");
  }
  const portInput = syncPeerDialogField("port");
  const portValue = portInput instanceof HTMLInputElement ? Number(portInput.value) : 22;
  const authMethod = syncPeerDialogField("authMethod")?.value || "identity";
  const payload = {
    id: syncPeerDialogRoot.getAttribute("data-codex-helper-sync-peer-id") || "",
    name: syncPeerDialogField("name")?.value?.trim() || "",
    host: syncPeerDialogField("host")?.value?.trim() || "",
    user: syncPeerDialogField("user")?.value?.trim() || "",
    port: portValue,
    authMethod,
  };
  if (authMethod === "password") {
    payload.password = syncPeerDialogField("password")?.value || "";
  } else {
    payload.identityFile = syncPeerDialogField("identityFile")?.value?.trim() || "";
  }
  return payload;
}

function syncStatusText(peer) {
  const status = peer?.lastStatus;
  if (!status) return "Never synced";
  return status.message || (status.ok ? "OK" : "Failed");
}

function createSyncPeerRow(peer) {
  const row = document.createElement("div");
  row.className = "codex-helper-settings-compact-row codex-helper-sync-peer-row codex-helper-provider-row-openable";
  row.setAttribute("data-codex-helper-sync-peer-id", peer.id || "");
  row.setAttribute(helperCommandAttribute, "sync-peer-edit");
  row.setAttribute("aria-label", `Edit ${peer.name || peer.host || "peer"}`);
  const text = document.createElement("div");
  text.className = "codex-helper-settings-compact-text";
  const title = document.createElement("div");
  title.className = "codex-helper-settings-row-title";
  title.textContent = peer.name || peer.host || "Peer";
  const description = document.createElement("div");
  description.className = "codex-helper-settings-row-description";
  const target = [peer.user, peer.host].filter(Boolean).join("@");
  description.textContent = `${target}${peer.port && peer.port !== 22 ? `:${peer.port}` : ""} · ${syncStatusText(peer)}`;
  text.append(title, description);
  const actions = document.createElement("div");
  actions.className = "codex-helper-sync-peer-actions";
  actions.append(
    syncPeerActionButton("sync-test", peer.id, "Test"),
    syncPeerActionButton("sync-now", peer.id, "Sync"),
  );
  const chevron = document.createElement("span");
  chevron.className = "codex-helper-provider-chevron";
  chevron.setAttribute("aria-hidden", "true");
  chevron.innerHTML = nativeSettingsStandardIconSvg("chevron-right");
  row.append(text, actions, chevron);
  return row;
}

function renderSync(result) {
  if (syncPeerDialogRoot?.isConnected) return;
  const self = result?.self || {};
  const peers = Array.isArray(result?.peers) ? result.peers : [];
  const role = self.role === "replica" ? "replica" : "primary";
  setHelperText("[data-codex-helper-sync-self-name]", self.name || "This Mac");
  setHelperText(
    "[data-codex-helper-sync-self-detail]",
    role === "replica"
      ? "Replica. Incoming provider files are applied automatically. This Mac does not push."
      : "Primary. Configure peers below, then test SSH and sync.",
  );
  for (const root of helperSettingsRoots()) {
    const roleSelect = root.querySelector(`[${helperSyncFieldAttribute}="role"]`);
    if (roleSelect instanceof HTMLSelectElement) roleSelect.value = role;
    const autoSync = root.querySelector(`[${helperSyncToggleAttribute}="autoSync"]`);
    if (autoSync instanceof HTMLInputElement) {
      autoSync.checked = Boolean(self.autoSync);
      autoSync.disabled = role === "replica";
    }
    const addButton = root.querySelector(`[${helperCommandAttribute}="sync-peer-add"]`);
    if (addButton instanceof HTMLButtonElement) addButton.disabled = role === "replica";
    const list = root.querySelector("[data-codex-helper-sync-peers]");
    if (!(list instanceof HTMLElement)) continue;
    list.textContent = "";
    if (result?.status && result.status !== "ok" && result.status !== "failed") {
      list.appendChild(createScrollEmptyMessage(resultText(result)));
      continue;
    }
    if (role === "replica") {
      list.appendChild(
        createScrollEmptyMessage("Peers are configured on the primary Mac."),
      );
      continue;
    }
    if (peers.length === 0) {
      list.appendChild(
        createScrollEmptyMessage("Add a peer with SSH host, user, and an identity file or password."),
      );
    } else {
      for (const peer of peers) list.appendChild(createSyncPeerRow(peer));
    }
  }
  let status = "No peers";
  if (result?.message) status = resultText(result);
  else if (peers.length) status = `${peers.length} peer${peers.length === 1 ? "" : "s"}`;
  else if (role === "replica") status = "Replica";
  setHelperText("[data-codex-helper-sync-status]", status);
}

async function refreshSyncPage(result) {
  if (result) {
    renderSync(result);
    return result;
  }
  const next = await bridge("/sync/get");
  renderSync(next);
  return next;
}

async function handleSyncCommand(command, source) {
  const peerId = syncCommandPeerId(source);
  switch (command) {
    case "sync-peer-add":
      openSyncPeerDialog("new");
      return;
    case "sync-peer-cancel":
      returnToSyncList();
      return;
    case "sync-peer-edit": {
      const current = await bridge("/sync/get");
      const peer = (current?.peers || []).find((item) => item.id === peerId);
      if (!peer) {
        setHelperText("[data-codex-helper-sync-status]", "Sync peer not found");
        return;
      }
      openSyncPeerDialog("edit", peer);
      return;
    }
    case "sync-peer-save": {
      const payload = syncPeerDialogPayload();
      const result = await bridge("/sync/peers/save", payload);
      if (result?.status !== "ok") {
        setSyncPeerDialogError(resultText(result));
        return;
      }
      returnToSyncList(result);
      setHelperText("[data-codex-helper-sync-status]", "Peer saved");
      return;
    }
    case "sync-peer-delete": {
      if (!peerId) return;
      const result = await bridge("/sync/peers/delete", { id: peerId });
      if (syncPeerDialogRoot?.isConnected) {
        if (result?.status !== "ok") {
          setSyncPeerDialogError(resultText(result));
          return;
        }
        returnToSyncList(result);
      } else {
        renderSync(result);
      }
      setHelperText(
        "[data-codex-helper-sync-status]",
        result?.status === "ok" ? "Peer deleted" : resultText(result),
      );
      return;
    }
    case "sync-test": {
      if (!peerId) return;
      setHelperText("[data-codex-helper-sync-status]", "Testing SSH…");
      const result = await bridge("/sync/test", { id: peerId });
      renderSync(result);
      setHelperText(
        "[data-codex-helper-sync-status]",
        result?.status === "ok" ? "SSH connection succeeded" : resultText(result),
      );
      return;
    }
    case "sync-now": {
      setHelperText("[data-codex-helper-sync-status]", "Syncing…");
      const result = await bridge("/sync/now", peerId ? { id: peerId } : {});
      renderSync(result);
      setHelperText(
        "[data-codex-helper-sync-status]",
        result?.status === "ok" ? "Synced" : resultText(result),
      );
      return;
    }
    default:
      return;
  }
}

async function handleSyncToggle(input) {
  const key = input.getAttribute(helperSyncToggleAttribute) || "";
  if (key !== "autoSync") return;
  input.disabled = true;
  const result = await bridge("/sync/set", { autoSync: input.checked });
  input.disabled = false;
  if (result?.status !== "ok") {
    input.checked = !input.checked;
    renderSync(result);
    setHelperText("[data-codex-helper-sync-status]", resultText(result));
    return;
  }
  renderSync(result);
}

async function handleSyncField(select) {
  const key = select.getAttribute(helperSyncFieldAttribute) || "";
  if (key !== "role") return;
  const result = await bridge("/sync/set", { role: select.value });
  if (result?.status !== "ok") {
    setHelperText("[data-codex-helper-sync-status]", resultText(result));
    await refreshSyncPage();
    return;
  }
  renderSync(result);
}
