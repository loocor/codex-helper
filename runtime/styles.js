// Injected stylesheet for Helper UI
function installHelperStyles() {
  let style = document.getElementById("codex-helper-runtime-style");
  if (!(style instanceof HTMLStyleElement)) {
    style = document.createElement("style");
    style.id = "codex-helper-runtime-style";
    document.head.appendChild(style);
  }
  style.textContent = `
      [${helperEntryAttribute}][data-active="true"] {
        background: color-mix(in srgb, currentColor 10%, transparent) !important;
      }
      [data-codex-helper-muted-selected="true"] {
        background: transparent !important;
        box-shadow: none !important;
      }
      [${helperContentHostAttribute}][data-codex-helper-active="true"] > :not([${helperPageAttribute}]) {
        display: none !important;
      }
      [${helperContentHostAttribute}][data-codex-helper-active="true"] {
        min-height: 0 !important;
        overflow: auto !important;
      }
      [${helperPageAttribute}] {
        display: flex;
        flex-direction: column;
        border-top: 0.5px solid var(--color-token-border, rgba(26, 28, 31, 0.12));
        padding-top: var(--padding-panel, 20px);
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperPageAttribute}] [data-codex-helper-backups] {
        display: contents;
      }
      [${helperPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperPageAttribute}] a.codex-helper-external-link,
      [${helperDialogPageAttribute}] a.codex-helper-external-link {
        text-decoration: none;
      }
      [${helperPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperPageAttribute}] pre[data-codex-helper-log],
      [${helperDialogPageAttribute}] pre[data-codex-helper-log] {
        margin: 0;
        padding: 12px;
        white-space: pre-wrap;
        word-break: break-word;
        font-size: 12px;
        line-height: 1.45;
      }
      [${helperPageAttribute}] .codex-helper-settings-scroll,
      [${helperDialogPageAttribute}] .codex-helper-settings-scroll {
        max-height: 160px;
        overflow: auto;
        min-height: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-row,
      [${helperDialogPageAttribute}] .codex-helper-settings-compact-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        min-width: 0;
        padding: 8px 12px;
        border-top: 0.5px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-row:first-child,
      [${helperDialogPageAttribute}] .codex-helper-settings-compact-row:first-child {
        border-top: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-text,
      [${helperDialogPageAttribute}] .codex-helper-settings-compact-text {
        min-width: 0;
        flex: 1 1 auto;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        font-size: 13px;
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-settings-compact-meta,
      [${helperDialogPageAttribute}] .codex-helper-settings-compact-meta {
        flex-shrink: 0;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperPageAttribute}] .codex-helper-settings-scroll-empty,
      [${helperDialogPageAttribute}] .codex-helper-settings-scroll-empty {
        padding: 12px;
        font-size: 13px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [${helperDialogPageAttribute}] {
        display: flex;
        flex-direction: column;
        color: inherit;
      }
      [${helperDialogPageAttribute}] .codex-helper-panel {
        background-color: var(--color-background-panel, var(--color-token-bg-fog));
      }
      [${helperPageAttribute}] .codex-helper-settings-section-title,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-title {
        padding: 0;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-heading,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-heading {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
        padding: 0 2px;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-link {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 6px;
        background: transparent;
        color: color-mix(in srgb, currentColor 70%, transparent);
        cursor: pointer;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link:hover,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-link:hover {
        background: color-mix(in srgb, currentColor 8%, transparent);
        color: inherit;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link:focus-visible,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-link:focus-visible {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperPageAttribute}] .codex-helper-settings-section-link svg,
      [${helperDialogPageAttribute}] .codex-helper-settings-section-link svg {
        width: 14px;
        height: 14px;
      }
      [${helperDialogPageAttribute}] .codex-helper-action {
        border-color: transparent;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch {
        position: relative;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input {
        position: absolute;
        width: 1px;
        height: 1px;
        opacity: 0;
        pointer-events: none;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:focus-visible + span {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:checked + span {
        background-color: var(--color-token-charts-blue, rgb(48, 145, 255));
      }
      [${helperDialogPageAttribute}] .codex-helper-switch input:checked + span > span {
        transform: translateX(14px);
      }
      [${helperSettingsDialogAttribute}] {
        position: fixed;
        inset: 0;
        z-index: 2147483646;
        display: grid;
        place-items: center;
        padding: 24px;
        background: rgba(0, 0, 0, 0.28);
        color: CanvasText;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-panel {
        width: min(760px, calc(100vw - 48px));
        max-height: min(820px, calc(100vh - 48px));
        display: flex;
        flex-direction: column;
        overflow: hidden;
        border-radius: 12px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        box-shadow: 0 24px 80px color-mix(in srgb, black 28%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 16px 18px;
        border-bottom: 1px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-title {
        font-size: 16px;
        font-weight: 600;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 28px;
        height: 28px;
        border: 0;
        border-radius: 8px;
        padding: 0;
        background: transparent;
        color: inherit;
        cursor: pointer;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close:hover {
        background: color-mix(in srgb, currentColor 8%, transparent);
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close:focus-visible {
        outline: 2px solid var(--color-token-focus-border, rgb(48, 145, 255));
        outline-offset: 2px;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-close svg {
        width: 16px;
        height: 16px;
      }
      [${helperSettingsDialogAttribute}] .codex-helper-settings-dialog-body {
        overflow: auto;
        padding: 18px;
      }
      [data-codex-helper-port-row] {
        position: relative;
        padding-right: 52px;
      }
      [data-codex-helper-port-row] .codex-helper-port-row-actions {
        position: absolute;
        right: 8px;
        top: 50%;
        transform: translateY(-50%);
        display: inline-flex;
        align-items: center;
        gap: 2px;
        opacity: 0;
        pointer-events: none;
        transition: opacity 120ms ease;
      }
      [data-codex-helper-port-row]:hover .codex-helper-port-row-actions,
      [data-codex-helper-port-row]:focus-within .codex-helper-port-row-actions {
        opacity: 1;
        pointer-events: auto;
      }
      .codex-helper-port-row-action {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        width: 22px;
        height: 22px;
        border: 0;
        border-radius: 6px;
        padding: 0;
        background: transparent;
        color: color-mix(in srgb, currentColor 68%, transparent);
        cursor: pointer;
      }
      .codex-helper-port-row-action:hover,
      .codex-helper-port-row-action:focus-visible {
        background: color-mix(in srgb, currentColor 10%, transparent);
        color: inherit;
        outline: none;
      }
      .codex-helper-port-row-action svg {
        width: 13px;
        height: 13px;
      }
      [data-codex-helper-port-menu] {
        position: fixed;
        z-index: 2147483646;
        min-width: 168px;
        padding: 4px;
        border-radius: 8px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        color: CanvasText;
        box-shadow: 0 12px 36px color-mix(in srgb, black 18%, transparent);
      }
      [data-codex-helper-port-menu] button {
        display: flex;
        width: 100%;
        align-items: center;
        justify-content: flex-start;
        gap: 8px;
        border: 0;
        border-radius: 6px;
        padding: 7px 8px;
        background: transparent;
        color: inherit;
        font: inherit;
        font-size: 13px;
        text-align: left;
        cursor: pointer;
      }
      [data-codex-helper-port-menu] button:hover,
      [data-codex-helper-port-menu] button:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [data-codex-helper-port-menu] svg {
        width: 14px;
        height: 14px;
      }
      [${helperSettingsDialogAttribute}] [${helperDialogPageAttribute}] {
        border-top: 0;
        padding-top: 0;
      }
      [${helperToastAttribute}] {
        position: fixed;
        right: 24px;
        bottom: 24px;
        z-index: 2147483647;
        max-width: min(420px, calc(100vw - 48px));
        border-radius: 10px;
        padding: 10px 12px;
        background: color-mix(in srgb, Canvas 96%, currentColor 4%);
        color: CanvasText;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        box-shadow: 0 12px 34px color-mix(in srgb, black 18%, transparent);
        font-size: 13px;
      }
      [${helperToastAttribute}] button {
        margin-left: 10px;
        border: 0;
        border-radius: 7px;
        padding: 5px 8px;
        background: color-mix(in srgb, currentColor 10%, transparent);
        color: inherit;
        font: inherit;
        cursor: pointer;
      }
      [data-codex-helper-project-move] {
        position: fixed;
        inset: 0;
        z-index: 2147483646;
        background: color-mix(in srgb, black 18%, transparent);
      }
      [data-codex-helper-project-move] .codex-helper-project-move-panel {
        position: fixed;
        width: min(360px, calc(100vw - 32px));
        max-height: min(420px, calc(100vh - 32px));
        overflow: auto;
        border-radius: 12px;
        border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
        background: Canvas;
        color: CanvasText;
        box-shadow: 0 16px 40px color-mix(in srgb, black 22%, transparent);
      }
      [data-codex-helper-project-move] .codex-helper-project-move-header {
        padding: 14px 16px 10px;
        border-bottom: 1px solid color-mix(in srgb, currentColor 10%, transparent);
      }
      [data-codex-helper-project-move] .codex-helper-project-move-title {
        font-size: 14px;
        font-weight: 600;
      }
      [data-codex-helper-project-move] .codex-helper-project-move-list {
        padding: 8px;
      }
      [data-codex-helper-project-move] .codex-helper-project-move-empty {
        padding: 12px;
        font-size: 13px;
        color: color-mix(in srgb, currentColor 55%, transparent);
      }
      [data-codex-helper-project-move] .codex-helper-project-move-item {
        display: block;
        width: 100%;
        border: 0;
        border-radius: 8px;
        padding: 10px 12px;
        background: transparent;
        color: inherit;
        text-align: left;
        cursor: pointer;
      }
      [data-codex-helper-project-move] .codex-helper-project-move-item:hover,
      [data-codex-helper-project-move] .codex-helper-project-move-item:focus-visible {
        background: color-mix(in srgb, currentColor 8%, transparent);
        outline: none;
      }
      [data-codex-helper-project-move] .codex-helper-project-move-item-title {
        font-size: 13px;
        font-weight: 500;
      }
      [data-codex-helper-project-move] .codex-helper-project-move-item-path {
        margin-top: 2px;
        font-size: 12px;
        color: color-mix(in srgb, currentColor 55%, transparent);
        word-break: break-all;
      }
    `;
}
