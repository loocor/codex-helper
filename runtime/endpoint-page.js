function nativeSettingsEndpointPageContent() {
  return `
    ${nativeSettingsGroupSection(
      "Base URL",
      `
        <div class="codex-helper-settings-row">
          <div class="codex-helper-settings-row-copy">
            <div class="codex-helper-settings-row-title" data-codex-helper-endpoint-base>http://127.0.0.1:3721/v1</div>
            <div class="codex-helper-settings-row-description">
              Chat Completions: POST /v1/chat/completions<br>
              Responses: POST /v1/responses
            </div>
            <div class="codex-helper-settings-row-description" data-codex-helper-endpoint-note></div>
            <div class="codex-helper-endpoint-models" data-codex-helper-endpoint-models></div>
          </div>
          <button type="button" class="codex-helper-native-settings-icon-button" ${helperCommandAttribute}="endpoint-copy-base" aria-label="Copy Base URL">${nativeSettingsStandardIconSvg("copy")}</button>
        </div>
      `,
    )}
    <section class="codex-helper-settings-section">
      <div class="codex-helper-provider-list-header">
        <div class="codex-helper-settings-section-title">API Keys</div>
      </div>
      ${nativeSettingsPanel(`
        <div class="codex-helper-endpoint-generate">
          <input class="codex-helper-text-input" data-codex-helper-endpoint-name placeholder="Note, e.g. Coser" aria-label="API key note">
          <button type="button" class="codex-helper-native-settings-icon-button" ${helperCommandAttribute}="endpoint-generate" aria-label="Generate API key">${nativeSettingsStandardIconSvg("dices")}</button>
        </div>
        <div class="codex-helper-settings-scroll" data-codex-helper-endpoint-keys></div>
        ${nativeSettingsListFooter("data-codex-helper-endpoint-status")}
      `)}
    </section>
  `;
}

function renderEndpoint(result) {
  const baseUrl = result?.baseUrl || "http://127.0.0.1:3721/v1";
  setHelperText("[data-codex-helper-endpoint-base]", baseUrl);
  const note = result?.officialActive
    ? "Official ChatGPT is active, so this endpoint is not serving requests."
    : "Other agents should set this Base URL and one of the keys below. Helper uses the active provider.";
  setHelperText("[data-codex-helper-endpoint-note]", note);
  renderEndpointModels(result);
  const keys = Array.isArray(result?.keys) ? result.keys : [];
  const statusText =
    result?.status === "ok"
      ? keys.length
        ? `${keys.length} key${keys.length === 1 ? "" : "s"}`
        : "No keys. Local requests stay open until you add one."
      : resultText(result);
  setHelperText("[data-codex-helper-endpoint-status]", statusText);
  const lists = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-endpoint-keys]"))
    .filter((panel) => panel instanceof HTMLElement);
  for (const list of lists) {
    list.textContent = "";
    if (result?.status !== "ok") {
      list.appendChild(createScrollEmptyMessage(statusText));
      continue;
    }
    if (keys.length === 0) {
      list.appendChild(
        createScrollEmptyMessage("Generate a key for Coser or another local agent."),
      );
      continue;
    }
    for (const key of keys) {
      list.appendChild(createEndpointKeyRow(key));
    }
  }
}

function createEndpointKeyRow(key) {
  const row = document.createElement("div");
  row.className = "codex-helper-settings-compact-row";
  row.setAttribute("data-codex-helper-endpoint-key-id", key.id || "");
  const text = document.createElement("div");
  text.className = "codex-helper-settings-compact-text";
  const name = document.createElement("div");
  name.className = "codex-helper-settings-row-title";
  name.textContent = key.name || "Untitled";
  const secret = document.createElement("div");
  secret.className = "codex-helper-endpoint-secret";
  secret.textContent = key.secret || "";
  text.append(name, secret);
  const actions = document.createElement("div");
  actions.className = "codex-helper-endpoint-key-actions";
  const copy = document.createElement("button");
  copy.type = "button";
  copy.className = "codex-helper-native-settings-icon-button";
  copy.setAttribute(helperCommandAttribute, "endpoint-copy-key");
  copy.setAttribute("data-codex-helper-endpoint-secret", key.secret || "");
  copy.setAttribute("aria-label", "Copy API key");
  copy.innerHTML = nativeSettingsStandardIconSvg("copy");
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "codex-helper-native-settings-icon-button";
  remove.setAttribute(helperCommandAttribute, "endpoint-delete-key");
  remove.setAttribute("data-codex-helper-endpoint-key-id", key.id || "");
  remove.setAttribute("aria-label", "Delete API key");
  remove.innerHTML = nativeSettingsStandardIconSvg("trash-2");
  actions.append(copy, remove);
  row.append(text, actions);
  return row;
}

function endpointNameValue() {
  const input = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-endpoint-name]"))
    .find((node) => node instanceof HTMLInputElement);
  return input instanceof HTMLInputElement ? input.value.trim() : "";
}

function endpointModelNames(result) {
  const names = [];
  const seen = new Set();
  for (const value of Array.isArray(result?.models) ? result.models : []) {
    const name = String(value || "").trim();
    if (!name) continue;
    const key = name.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    names.push(name);
  }
  return names;
}

function renderEndpointModels(result) {
  const models = endpointModelNames(result);
  const lists = helperSettingsRoots()
    .map((root) => root.querySelector("[data-codex-helper-endpoint-models]"))
    .filter((node) => node instanceof HTMLElement);
  for (const list of lists) {
    list.textContent = "";
    for (const model of models) {
      const tag = document.createElement("button");
      tag.type = "button";
      tag.className = "codex-helper-endpoint-model-tag";
      tag.setAttribute(helperCommandAttribute, "endpoint-copy-model");
      tag.setAttribute("data-codex-helper-endpoint-model", model);
      tag.setAttribute("title", `Copy ${model}`);
      tag.setAttribute("aria-label", `Copy model ${model}`);
      tag.textContent = model;
      list.appendChild(tag);
    }
  }
}

async function copyEndpointText(value) {
  if (!value) return;
  await navigator.clipboard.writeText(value);
}

async function handleEndpointCommand(command, source) {
  if (command === "endpoint-copy-base") {
    const node = helperSettingsRoots()
      .map((root) => root.querySelector("[data-codex-helper-endpoint-base]"))
      .find((item) => item instanceof HTMLElement);
    await copyEndpointText(node?.textContent || "");
    return;
  }
  if (command === "endpoint-copy-key") {
    await copyEndpointText(source?.getAttribute("data-codex-helper-endpoint-secret") || "");
    return;
  }
  if (command === "endpoint-copy-model") {
    await copyEndpointText(source?.getAttribute("data-codex-helper-endpoint-model") || "");
    return;
  }
  if (command === "endpoint-generate") {
    const result = await bridge("/endpoint/keys", {
      name: endpointNameValue(),
      length: 32,
    });
    renderEndpoint(result);
    return;
  }
  if (command === "endpoint-delete-key") {
    const id = source?.getAttribute("data-codex-helper-endpoint-key-id") || "";
    if (!id) return;
    const result = await bridge("/endpoint/keys/delete", { id });
    renderEndpoint(result);
  }
}
