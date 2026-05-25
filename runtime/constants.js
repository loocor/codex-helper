// Constants and mutable runtime state
const helperEntryAttribute = "data-codex-helper-settings-entry";
const helperContentHostAttribute = "data-codex-helper-content-host";
const helperPageAttribute = "data-codex-helper-settings-page";
const helperCommandAttribute = "data-codex-helper-command";
const helperToggleAttribute = "data-codex-helper-setting-toggle";
const helperToastAttribute = "data-codex-helper-toast";
const helperSessionActionPrefix = "codex-helper-session-";
const helperSettingsSectionAttribute = "data-codex-helper-settings-section";
const helperNativeSettingsGroupAttribute =
  "data-codex-helper-native-settings-group";
const helperNativeSettingsEntryAttribute =
  "data-codex-helper-native-settings-entry";
const helperNativeSettingsPageAttribute =
  "data-codex-helper-native-settings-page";
const helperNativeSettingsContentHostAttribute =
  "data-codex-helper-native-settings-content-host";
const helperPortCommandAttribute = "data-codex-helper-port-command";
const helperPortsPinnedAttribute = "data-codex-helper-ports-pinned";
let portsSurface = "none";
const helperSettingsPanelId = "codex-helper-settings-panel";
const helperRepoUrl = "https://github.com/loocor/codex-helper";
const helperBuildDate = "__CODEX_HELPER_BUILD_DATE__";
const helperActionClass =
  "codex-helper-action border-token-border user-select-none no-drag cursor-interaction flex shrink-0 items-center gap-1 border whitespace-nowrap rounded-lg px-2 py-1 text-sm text-token-foreground bg-token-foreground/5 enabled:hover:bg-token-foreground/10";
const helperPanelClass =
  "codex-helper-panel flex flex-col divide-y-[0.5px] divide-token-border overflow-hidden rounded-lg border border-token-border";
let observerInstalled = false;
let helperRuntimeObserver = null;
let helperPageRoot = null;
let helperNativeSettingsRoot = null;
let helperNativeSettingsContentHost = null;
let helperNativeSettingsContentStash = null;
let helperNativeSettingsActivePage = "";
let helperContentHost = null;
let helperContentStash = null;
let pendingSessionMenuContext = null;
let sessionContextMenuMapRestore = null;
let sessionContextMenuReplayInFlight = false;
let pendingPortScan = 0;
let maintainPortsPanelTimer = 0;
let refreshPortsPanelTimer = 0;
let pinnedSummaryHideTimer = 0;
let portScanIntervalId = 0;
let remotePortSyncInFlight = false;
let managedPortStopInFlight = false;
let pinnedSummaryCardRef = null;
let pinnedPortsLastSnapshot = "";
let lastPortScanSessionKey = "";
let resolvedRemoteForwardingContext = null;
let portForwardMenuRoot = null;
let portForwardMenuAnchorRow = null;
let portForwardSettingsAnchorButton = null;
let portForwardDialogRoot = null;
const detectedPorts = new Map();
const portDiscoveryStates = new Map();
const suppressedPortMappings = new Set();
let featureSettings = {
  sessionDeleteEnabled: false,
  markdownExportEnabled: false,
  sessionMoveEnabled: false,
  portForwardingEnabled: false,
  portAutoForwardWeb: true,
  portSameLocalPort: true,
};
let featureSettingsLoaded = false;
