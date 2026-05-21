<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  type PickerHotkey = {
    shift: boolean;
    alt: boolean;
    cmd: boolean;
    ctrl: boolean;
    key: string;
  };

  type SettingsSnapshot = {
    pickerHotkey: PickerHotkey;
    activeLabel: string;
    quickPickPrefixEnabled: boolean;
    quickPickPrefix: string;
    historyMaxEntries: number;
    historyUseMaxAge: boolean;
    historyMaxAgeDays: number;
    appVersion: string;
    buildTimestamp: string;
  };

  type Props = {
    onBack: () => void;
    onHotkeyChange?: (label: string) => void;
    onSettingsChange?: (snap: SettingsSnapshot) => void;
  };

  let { onBack, onHotkeyChange, onSettingsChange }: Props = $props();

  let shift = $state(true);
  let alt = $state(false);
  let cmd = $state(true);
  let ctrl = $state(false);
  let key = $state("E");
  let previewLabel = $state("⌘⇧E");
  let quickPickPrefixEnabled = $state(false);
  let quickPickPrefix = $state(".");
  let historyMaxEntries = $state(500);
  let historyUseMaxAge = $state(true);
  let historyMaxAgeDays = $state(14);
  let saving = $state(false);
  let clearing = $state(false);
  let errorText = $state("");
  let appVersion = $state("…");
  let buildTimestamp = $state("…");

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  const isMac =
    typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  function currentHotkey(): PickerHotkey {
    return { shift, alt, cmd, ctrl, key: key.trim() || "E" };
  }

  function applySnapshot(snap: SettingsSnapshot): void {
    shift = snap.pickerHotkey.shift;
    alt = snap.pickerHotkey.alt;
    cmd = snap.pickerHotkey.cmd;
    ctrl = snap.pickerHotkey.ctrl;
    key = snap.pickerHotkey.key;
    previewLabel = snap.activeLabel;
    quickPickPrefixEnabled = snap.quickPickPrefixEnabled;
    quickPickPrefix = snap.quickPickPrefix || ".";
    historyMaxEntries = snap.historyMaxEntries;
    historyUseMaxAge = snap.historyUseMaxAge;
    historyMaxAgeDays = snap.historyMaxAgeDays;
    appVersion = snap.appVersion;
    buildTimestamp = snap.buildTimestamp;
    onHotkeyChange?.(snap.activeLabel);
    onSettingsChange?.(snap);
  }

  async function persist(): Promise<void> {
    if (saving) {
      return;
    }
    saving = true;
    errorText = "";
    try {
      const snap = await invoke<SettingsSnapshot>("settings_set_picker_hotkey", {
        hotkey: currentHotkey(),
      });
      applySnapshot(snap);
    } catch (err) {
      errorText = String(err);
    } finally {
      saving = false;
    }
  }

  function scheduleSave(): void {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void persist();
    }, 320);
  }

  function scheduleQuickPickSave(): void {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void persistQuickPick();
    }, 320);
  }

  async function persistQuickPick(): Promise<void> {
    if (saving) {
      return;
    }
    saving = true;
    errorText = "";
    try {
      const snap = await invoke<SettingsSnapshot>("settings_set_quick_pick_prefix", {
        enabled: quickPickPrefixEnabled,
        prefix: quickPickPrefix.trim() || ".",
      });
      applySnapshot(snap);
    } catch (err) {
      errorText = String(err);
    } finally {
      saving = false;
    }
  }

  function scheduleHistorySave(): void {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      void persistHistoryLimits();
    }, 320);
  }

  async function persistHistoryLimits(): Promise<void> {
    if (saving) {
      return;
    }
    saving = true;
    errorText = "";
    try {
      const snap = await invoke<SettingsSnapshot>("settings_set_history_limits", {
        maxEntries: historyMaxEntries,
        useMaxAge: historyUseMaxAge,
        maxAgeDays: historyMaxAgeDays,
      });
      applySnapshot(snap);
    } catch (err) {
      errorText = String(err);
    } finally {
      saving = false;
    }
  }

  function onPrefixInput(ev: Event): void {
    const el = ev.currentTarget as HTMLInputElement;
    quickPickPrefix = el.value.slice(0, 8);
    el.value = quickPickPrefix;
    scheduleQuickPickSave();
  }

  function onKeyInput(ev: Event): void {
    const el = ev.currentTarget as HTMLInputElement;
    const one = el.value.replace(/[^a-zA-Z]/g, "").slice(0, 1).toUpperCase();
    key = one || "E";
    el.value = key;
    scheduleSave();
  }

  async function load(): Promise<void> {
    const snap = await invoke<SettingsSnapshot>("settings_get");
    applySnapshot(snap);
  }

  async function onClearHistory(): Promise<void> {
    if (clearing || saving) {
      return;
    }
    if (
      !confirm(
        "Remove every remembered clip from this machine?\nPins are cleared too.",
      )
    ) {
      return;
    }
    clearing = true;
    errorText = "";
    try {
      await invoke("clips_clear");
    } catch (err) {
      errorText = String(err);
    } finally {
      clearing = false;
    }
  }

  onMount(() => {
    void load();
    return () => clearTimeout(saveTimer);
  });
</script>

<section class="settings-panel" aria-labelledby="settings-heading">
  <header class="settings-head">
    <button type="button" class="btn-back" onclick={onBack} title="Esc">
      ← Back
    </button>
    <h2 id="settings-heading" class="settings-title">Settings</h2>
  </header>

  <div class="settings-card">
    <p class="setting-label">Show / hide picker</p>
    <p class="setting-desc">
      Global shortcut to toggle the ESTM window. Pick at least one modifier.
    </p>

    <fieldset class="modifier-fieldset" disabled={saving}>
      <legend class="modifier-legend">Modifiers</legend>
      <label class="modifier-check">
        <input
          type="checkbox"
          bind:checked={shift}
          onchange={() => scheduleSave()}
        />
        <span>Shift</span>
      </label>
      <label class="modifier-check">
        <input
          type="checkbox"
          bind:checked={alt}
          onchange={() => scheduleSave()}
        />
        <span>Alt{isMac ? " (⌥)" : ""}</span>
      </label>
      <label class="modifier-check">
        <input
          type="checkbox"
          bind:checked={cmd}
          onchange={() => scheduleSave()}
        />
        <span>{isMac ? "Cmd (⌘)" : "Win / Super"}</span>
      </label>
      <label class="modifier-check">
        <input
          type="checkbox"
          bind:checked={ctrl}
          onchange={() => scheduleSave()}
        />
        <span>Ctrl{isMac ? " (⌃)" : ""}</span>
      </label>
    </fieldset>

    <div class="key-row">
      <label class="key-label" for="picker-hotkey-key">Letter</label>
      <input
        id="picker-hotkey-key"
        class="key-input"
        type="text"
        inputmode="text"
        maxlength="1"
        autocomplete="off"
        spellcheck="false"
        aria-label="Shortcut letter key"
        value={key}
        disabled={saving}
        oninput={onKeyInput}
      />
    </div>

    <p class="hotkey-preview">
      Active shortcut:
      <kbd class="preview-kbd">{previewLabel}</kbd>
      {#if saving}
        <span class="saving-hint">…</span>
      {/if}
    </p>

    {#if errorText}
      <p class="settings-error" role="alert">{errorText}</p>
    {/if}
  </div>

  <div class="settings-card">
    <p class="setting-label">Prefix quick-pick</p>
    <p class="setting-desc">
      When enabled, type your prefix in the filter then
      <kbd>1</kbd>–<kbd>9</kbd> to pick that row (digits still work in search).
      Type the prefix twice to search for it literally.
    </p>

    <label class="modifier-check quick-pick-toggle">
      <input
        type="checkbox"
        bind:checked={quickPickPrefixEnabled}
        disabled={saving}
        onchange={() => scheduleQuickPickSave()}
      />
      <span>Enable prefix quick-pick</span>
    </label>

    <div class="key-row">
      <label class="key-label" for="quick-pick-prefix">Prefix</label>
      <input
        id="quick-pick-prefix"
        class="key-input prefix-input"
        type="text"
        maxlength="8"
        autocomplete="off"
        spellcheck="false"
        aria-label="Quick-pick prefix characters"
        value={quickPickPrefix}
        disabled={saving || !quickPickPrefixEnabled}
        oninput={onPrefixInput}
      />
    </div>
  </div>

  <div class="settings-card">
    <p class="setting-label">History storage</p>
    <p class="setting-desc">
      Unpinned clips are removed when over the entry limit or older than max age.
      Pinned clips are never removed by age.
    </p>

    <div class="key-row">
      <label class="key-label" for="history-max-entries">Max entries</label>
      <input
        id="history-max-entries"
        class="key-input number-input"
        type="number"
        min="1"
        max="50000"
        step="1"
        bind:value={historyMaxEntries}
        disabled={saving}
        onchange={() => scheduleHistorySave()}
      />
    </div>

    <label class="modifier-check quick-pick-toggle">
      <input
        type="checkbox"
        bind:checked={historyUseMaxAge}
        disabled={saving}
        onchange={() => scheduleHistorySave()}
      />
      <span>Also limit by age (unpinned only)</span>
    </label>

    <div class="key-row">
      <label class="key-label" for="history-max-age-days">Max age (days)</label>
      <input
        id="history-max-age-days"
        class="key-input number-input"
        type="number"
        min="1"
        max="3650"
        step="1"
        bind:value={historyMaxAgeDays}
        disabled={saving || !historyUseMaxAge}
        onchange={() => scheduleHistorySave()}
      />
    </div>
    {#if !historyUseMaxAge}
      <p class="setting-desc setting-desc--tight">
        Only the entry count applies; age is ignored.
      </p>
    {/if}

    <div class="settings-danger-row">
      <button
        type="button"
        class="btn-danger"
        disabled={saving || clearing}
        onclick={() => void onClearHistory()}
      >
        {clearing ? "Clearing…" : "Clear history"}
      </button>
      <p class="setting-desc setting-desc--tight">
        Deletes all clips and pins from this Mac. Cannot be undone.
      </p>
    </div>
  </div>

  <div class="settings-card">
    <p class="setting-label">Build</p>
    <p class="setting-desc setting-desc--mono">
      v{appVersion} · {buildTimestamp}
    </p>
  </div>
</section>
