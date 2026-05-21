<script lang="ts">
  import { onMount, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import Settings from "./Settings.svelte";

  type ClipRow = {
    id: number;
    plaintext: string;
    createdMs: number;
    pinned: boolean;
    label?: string | null;
  };

  let searchEl = $state<HTMLInputElement | undefined>(undefined);
  let listEl = $state<HTMLUListElement | undefined>(undefined);

  type ListView = "pinned" | "all" | "labeled";

  const LIST_VIEW_ORDER: ListView[] = ["pinned", "all", "labeled"];
  const LIST_VIEW_LABEL: Record<ListView, string> = {
    pinned: "Pinned",
    all: "All",
    labeled: "Labeled",
  };
  /** Circular ←/→ between views (display order: pinned · all · labeled). */
  const LIST_VIEW_LEFT: Record<ListView, ListView> = {
    all: "pinned",
    pinned: "labeled",
    labeled: "all",
  };
  const LIST_VIEW_RIGHT: Record<ListView, ListView> = {
    all: "labeled",
    pinned: "all",
    labeled: "pinned",
  };

  let query = $state("");
  let clips = $state<ClipRow[]>([]);
  let listView = $state<ListView>("all");
  /** Row index in `displayClips`; `-1` = filter focused. */
  let selectedIdx = $state(-1);

  const displayClips = $derived(clipsForListView(clips, listView));
  const rowCount = $derived(displayClips.length);
  const listViewCounts = $derived({
    pinned: clips.filter((c) => c.pinned).length,
    all: clips.length,
    labeled: clips.filter((c) => clipHasLabel(c)).length,
  });

  function sortByRecency(a: ClipRow, b: ClipRow): number {
    if (b.createdMs !== a.createdMs) {
      return b.createdMs - a.createdMs;
    }
    return b.id - a.id;
  }

  function clipsForListView(rows: ClipRow[], mode: ListView): ClipRow[] {
    if (mode === "pinned") {
      return rows.filter((c) => c.pinned);
    }
    if (mode === "labeled") {
      return rows.filter((c) => clipHasLabel(c));
    }
    // All: chronological only — pinned clips are not grouped at the top.
    return [...rows].sort(sortByRecency);
  }

  function clipAtIndex(idx: number): ClipRow | undefined {
    if (idx < 0) {
      return undefined;
    }
    return displayClips[idx];
  }

  function syncSelectionAfterViewChange(anchorId: number | null): void {
    const visible = clipsForListView(clips, listView);
    if (anchorId === null) {
      selectedIdx = -1;
      clampSelection();
      return;
    }
    const idx = visible.findIndex((c) => c.id === anchorId);
    selectedIdx = idx >= 0 ? idx : visible.length > 0 ? 0 : -1;
    clampSelection();
  }

  function setListView(next: ListView): void {
    if (listView === next) {
      return;
    }
    const anchorId =
      selectedIdx >= 0 ? (clipAtIndex(selectedIdx)?.id ?? null) : null;
    listView = next;
    syncSelectionAfterViewChange(anchorId);
  }

  function stepListView(direction: "left" | "right"): void {
    setListView(
      direction === "left" ? LIST_VIEW_LEFT[listView] : LIST_VIEW_RIGHT[listView],
    );
  }

  let statusHintText = $state("");
  let helpOpen = $state(false);
  /** After prefix in filter: next 1–9 picks that row (digits still type normally). */
  let prefixPickArmed = $state(false);
  let prefixPickPending = $state("");
  let quickPickPrefixEnabled = $state(false);
  let quickPickPrefix = $state(".");
  let page = $state<"picker" | "settings">("picker");
  let globalHotkeyLabel = $state("⌘⇧E");

  type SettingsSnapshot = {
    activeLabel: string;
    quickPickPrefixEnabled: boolean;
    quickPickPrefix: string;
  };

  function applySettingsSnapshot(snap: SettingsSnapshot): void {
    globalHotkeyLabel = snap.activeLabel;
    quickPickPrefixEnabled = snap.quickPickPrefixEnabled;
    quickPickPrefix = snap.quickPickPrefix || ".";
    if (!quickPickPrefixEnabled) {
      prefixPickArmed = false;
      prefixPickPending = "";
    }
  }

  let debounceTimer: ReturnType<typeof setTimeout> | undefined;
  let hintTimer: ReturnType<typeof setTimeout> | undefined;
  let editingLabelIdx = $state(-1);
  let labelDraft = $state("");

  function clipHasLabel(clip: ClipRow): boolean {
    return (clip.label?.trim().length ?? 0) > 0;
  }

  function clipDisplayText(clip: ClipRow): string {
    const t = clip.plaintext.trim();
    return t || "(empty)";
  }

  function clampSelection(): void {
    if (rowCount === 0 || selectedIdx < 0) {
      if (rowCount === 0) {
        selectedIdx = -1;
      }
      return;
    }
    selectedIdx = Math.min(selectedIdx, rowCount - 1);
  }

  const QUICK_PICK_SLOTS = 9;

  const isMac =
    typeof navigator !== "undefined" && navigator.platform.includes("Mac");

  function isPrimaryModifier(ev: KeyboardEvent): boolean {
    return isMac ? ev.metaKey : ev.ctrlKey;
  }

  function openSettings(): void {
    helpOpen = false;
    page = "settings";
  }

  function leaveSettings(): void {
    page = "picker";
    void focusFilter();
  }

  function keyHasNoModifiers(ev: KeyboardEvent): boolean {
    return !ev.metaKey && !ev.ctrlKey && !ev.altKey;
  }

  function digitIndexFromKey(ev: KeyboardEvent): number | null {
    if (ev.key >= "1" && ev.key <= "9") {
      return Number(ev.key) - 1;
    }
    if (/^Numpad[1-9]$/.test(ev.code)) {
      return Number(ev.code.slice(6)) - 1;
    }
    return null;
  }

  function cancelPrefixPickArm(extra = ""): void {
    prefixPickArmed = false;
    prefixPickPending = "";
    if (extra !== "") {
      query += extra;
      scheduleLoad();
    }
  }

  function prefixMatchesKey(ev: KeyboardEvent, ch: string): boolean {
    return keyHasNoModifiers(ev) && ev.key === ch;
  }

  async function tryQuickPick(
    ev: KeyboardEvent,
    inFilter: boolean,
  ): Promise<boolean> {
    const prefix = quickPickPrefix;
    const prefixEnabled = quickPickPrefixEnabled && prefix.length > 0;

    if (prefixPickArmed) {
      const digitIdx = digitIndexFromKey(ev);
      if (digitIdx !== null && digitIdx < rowCount && keyHasNoModifiers(ev)) {
        ev.preventDefault();
        cancelPrefixPickArm();
        selectedIdx = digitIdx;
        await copyAtIndex(digitIdx, { keepPickerOpen: ev.shiftKey });
        return true;
      }
      if (inFilter && prefixEnabled) {
        if (prefixMatchesKey(ev, prefix[0]!) && prefix.length === 1) {
          ev.preventDefault();
          cancelPrefixPickArm(prefix + prefix);
          return true;
        }
        if (ev.key.length === 1 && keyHasNoModifiers(ev)) {
          ev.preventDefault();
          cancelPrefixPickArm(prefix + ev.key);
          return true;
        }
      }
      cancelPrefixPickArm();
      return false;
    }

    if (prefixEnabled && inFilter && rowCount > 0 && keyHasNoModifiers(ev)) {
      const nextIdx = prefixPickPending.length;
      if (nextIdx < prefix.length && ev.key === prefix[nextIdx]) {
        ev.preventDefault();
        prefixPickPending += ev.key;
        if (prefixPickPending === prefix) {
          prefixPickPending = "";
          prefixPickArmed = true;
        }
        return true;
      }
      if (prefixPickPending.length > 0) {
        const pending = prefixPickPending;
        prefixPickPending = "";
        query += pending;
        scheduleLoad();
      }
    }

    if (!inFilter && keyHasNoModifiers(ev)) {
      const digitIdx = digitIndexFromKey(ev);
      if (digitIdx !== null && digitIdx < rowCount) {
        ev.preventDefault();
        selectedIdx = digitIdx;
        await copyAtIndex(digitIdx, { keepPickerOpen: ev.shiftKey });
        return true;
      }
    }

    return false;
  }

  async function focusFilter(opts?: { select?: boolean }): Promise<void> {
    await tick();
    if (!searchEl) {
      return;
    }
    searchEl.scrollIntoView({ block: "nearest" });
    searchEl.focus();
    if (opts?.select) {
      searchEl.select();
    }
  }

  function flashHint(text: string, ms = 2200): void {
    clearTimeout(hintTimer);
    statusHintText = text;
    hintTimer = setTimeout(() => {
      statusHintText = "";
    }, ms);
  }

  async function highlightSelection(opts?: {
    focusRow?: boolean;
  }): Promise<void> {
    clampSelection();
    if (opts?.focusRow !== true) {
      return;
    }
    if (selectedIdx < 0) {
      return;
    }
    await tick();
    const row = document.querySelector<HTMLElement>(
      `[data-clip-idx="${selectedIdx}"]`,
    );
    row?.focus({ preventScroll: true });
    row?.scrollIntoView({ block: "nearest" });
  }

  async function hidePicker(): Promise<void> {
    try {
      await invoke("picker_hide");
    } catch {
      /* picker_hide unavailable outside Tauri */
    }
  }

  async function copyAtIndex(
    idx: number,
    opts?: { keepPickerOpen?: boolean; keepSearchFocused?: boolean },
  ): Promise<boolean> {
    if (rowCount === 0) {
      return false;
    }
    const safeIdx = Math.min(Math.max(idx, 0), rowCount - 1);
    const clip = clipAtIndex(safeIdx);
    if (!clip) {
      return false;
    }
    try {
      await invoke("clips_copy", { id: clip.id });
      if (opts?.keepPickerOpen !== true) {
        await hidePicker();
        flashHint(isMac ? "Copied — press ⌘V to paste" : "Copied — press Ctrl+V to paste");
        return true;
      }
      flashHint(`Copied #${clip.id} (${clip.plaintext.length} chars)`);
      await highlightSelection({ focusRow: !opts?.keepSearchFocused });
      return true;
    } catch (err) {
      flashHint(String(err));
      return false;
    }
  }

  async function loadClips(opts?: {
    anchorId?: number | null;
    preserveFilterFocus?: boolean;
  }): Promise<void> {
    editingLabelIdx = -1;
    const trimmed = query.trim();
    const filterFocused =
      opts?.preserveFilterFocus === true ||
      (searchEl !== undefined && document.activeElement === searchEl);

    const anchorBefore = filterFocused
      ? (opts?.anchorId ?? null)
      : (opts?.anchorId ??
        (selectedIdx >= 0 ? (clipAtIndex(selectedIdx)?.id ?? null) : null));

    const refocusRow =
      !filterFocused &&
      ((listEl?.matches(":focus-within") ?? false) ||
        document.activeElement?.closest(".clip-row") !== null);

    let rows: ClipRow[];
    if (trimmed === "") {
      rows = await invoke<ClipRow[]>("clips_recent", { limit: 120 });
    } else {
      rows = await invoke<ClipRow[]>("clips_search", {
        query: trimmed,
        limit: 120,
      });
    }

    clips = rows;
    const visible = clipsForListView(rows, listView);

    if (filterFocused) {
      if (anchorBefore === null) {
        selectedIdx = -1;
      } else {
        const idx = visible.findIndex((r) => r.id === anchorBefore);
        selectedIdx = idx >= 0 ? idx : -1;
      }
    } else {
      if (anchorBefore === null) {
        selectedIdx = visible.length > 0 ? 0 : -1;
      } else {
        const idx = visible.findIndex((r) => r.id === anchorBefore);
        selectedIdx = idx >= 0 ? idx : visible.length > 0 ? 0 : -1;
      }
    }
    clampSelection();

    await tick();
    if (selectedIdx >= 0 && refocusRow) {
      await highlightSelection({ focusRow: true });
    } else if (filterFocused) {
      await focusFilter();
    }
  }

  function scheduleLoad(): void {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      const inFilter =
        searchEl !== undefined && document.activeElement === searchEl;
      void loadClips({ preserveFilterFocus: inFilter });
    }, 140);
  }

  function onSlashShortcut(ev: KeyboardEvent): void {
    if (ev.defaultPrevented || ev.repeat || page === "settings") {
      return;
    }

    const ae = document.activeElement;

    if (ev.key === "/" && !isPrimaryModifier(ev) && !ev.altKey) {
      if (ae === searchEl || ae instanceof HTMLTextAreaElement) {
        return;
      }
      ev.preventDefault();
      void focusFilter({ select: true });
      return;
    }

    if (
      ev.key.toLowerCase() === "k" &&
      isPrimaryModifier(ev) &&
      !ev.altKey
    ) {
      if (ae instanceof HTMLButtonElement) {
        return;
      }
      ev.preventDefault();
      void focusFilter({ select: true });
    }
  }

  function cancelLabelEdit(): void {
    editingLabelIdx = -1;
    void highlightSelection({ focusRow: true });
  }

  async function commitLabelEdit(): Promise<void> {
    const idx = editingLabelIdx;
    if (idx < 0) {
      return;
    }
    const clip = clipAtIndex(idx);
    if (!clip) {
      editingLabelIdx = -1;
      return;
    }
    editingLabelIdx = -1;
    const trimmed = labelDraft.trim();
    try {
      const ok = await invoke<boolean>("clips_set_label", {
        id: clip.id,
        label: trimmed === "" ? null : trimmed,
      });
      if (!ok) {
        flashHint("Could not set label — clip missing?");
        return;
      }
      flashHint(
        trimmed === ""
          ? `Label cleared on #${clip.id}`
          : `Labeled #${clip.id}: ${trimmed}`,
      );
      await loadClips({ anchorId: clip.id });
    } catch (err) {
      flashHint(String(err));
    }
  }

  function startLabelEdit(idx: number): void {
    const clip = clipAtIndex(idx);
    if (!clip) {
      return;
    }
    editingLabelIdx = idx;
    labelDraft = clip.label ?? "";
    void tick().then(() => {
      const input = document.querySelector<HTMLInputElement>(
        `[data-label-idx="${idx}"]`,
      );
      input?.focus();
      input?.select();
    });
  }

  async function onLabelKeydown(
    ev: KeyboardEvent,
    idx: number,
  ): Promise<void> {
    if (ev.key === "Enter") {
      ev.preventDefault();
      ev.stopPropagation();
      if (editingLabelIdx === idx) {
        await commitLabelEdit();
      }
      return;
    }
    if (ev.key === "Escape") {
      ev.preventDefault();
      ev.stopPropagation();
      cancelLabelEdit();
    }
  }

  async function onNavKeyCapture(ev: KeyboardEvent): Promise<void> {
    if (ev.defaultPrevented || ev.repeat || searchEl === undefined) {
      return;
    }

    if (page === "settings") {
      if (ev.key === "Escape") {
        ev.preventDefault();
        leaveSettings();
      }
      return;
    }

    if (helpOpen && ev.key === "Escape") {
      ev.preventDefault();
      helpOpen = false;
      return;
    }

    const ae = document.activeElement;
    const search = searchEl;
    const inFilter = ae === search;

    if (ev.key === "?" && !inFilter) {
      ev.preventDefault();
      helpOpen = !helpOpen;
      return;
    }

    if (ev.key === "," && isPrimaryModifier(ev) && !ev.altKey) {
      ev.preventDefault();
      openSettings();
      return;
    }

    if (editingLabelIdx >= 0) {
      return;
    }

    if (prefixPickArmed && ev.key === "Escape") {
      ev.preventDefault();
      cancelPrefixPickArm();
      return;
    }

    if (await tryQuickPick(ev, inFilter)) {
      return;
    }
    const listRowEl =
      ae instanceof Element ? ae.closest(".clip-row") : null;
    const inListNav = listRowEl !== null;
    const listSelectionActive = selectedIdx >= 0;
    const isArrowVertical =
      ev.key === "ArrowDown" || ev.key === "ArrowUp";

    if (inFilter) {
      if (ev.key === "ArrowLeft" || ev.key === "ArrowRight") {
        ev.preventDefault();
        stepListView(ev.key === "ArrowLeft" ? "left" : "right");
        return;
      }

      if (ev.key === "Escape") {
        ev.preventDefault();
        if (prefixPickArmed || prefixPickPending.length > 0) {
          cancelPrefixPickArm();
          return;
        }
        if (query.trim() !== "") {
          query = "";
          selectedIdx = -1;
          await loadClips({ preserveFilterFocus: true });
          flashHint("Filter cleared");
        } else {
          await focusFilter();
        }
        return;
      }

      if (ev.key === "Enter") {
        if (rowCount === 0) {
          return;
        }
        ev.preventDefault();
        selectedIdx = 0;
        const keepOpen = ev.shiftKey;
        void copyAtIndex(0, {
          keepPickerOpen: keepOpen,
          keepSearchFocused: keepOpen,
        });
        return;
      }

      if (ev.key === "ArrowUp") {
        ev.preventDefault();
        helpOpen = true;
        return;
      }

      if (ev.key === "ArrowDown") {
        if (rowCount === 0) {
          return;
        }
        ev.preventDefault();
        selectedIdx = 0;
        await highlightSelection({ focusRow: true });
      }
      return;
    }

    if (rowCount === 0) {
      return;
    }

    if (
      isArrowVertical &&
      (inListNav || listSelectionActive)
    ) {
      ev.preventDefault();
    } else if (!inListNav && !listSelectionActive) {
      return;
    }

    if (
      ae instanceof HTMLButtonElement &&
      (ev.key === "Enter" || ev.key === " ")
    ) {
      return;
    }

    if (ev.key === "Escape") {
      ev.preventDefault();
      selectedIdx = -1;
      await focusFilter();
      return;
    }

    const last = rowCount - 1;

    if (ev.key === "ArrowDown") {
      if (!(inListNav || listSelectionActive)) {
        return;
      }
      if (selectedIdx < 0) {
        selectedIdx = 0;
      } else {
        selectedIdx = Math.min(selectedIdx + 1, last);
      }
      await highlightSelection({ focusRow: true });
      return;
    }

    if (ev.key === "ArrowUp") {
      if (!(inListNav || listSelectionActive)) {
        return;
      }
      if (selectedIdx <= 0) {
        selectedIdx = -1;
        await focusFilter();
        return;
      }
      selectedIdx = Math.max(selectedIdx - 1, 0);
      await highlightSelection({ focusRow: true });
      return;
    }

    if (ev.key === "Enter") {
      ev.preventDefault();
      if (selectedIdx < 0) {
        selectedIdx = 0;
      }
      await copyAtIndex(selectedIdx, { keepPickerOpen: ev.shiftKey });
      return;
    }

    if (selectedIdx < 0) {
      return;
    }

    const key = ev.key.toLowerCase();
    if (key === "p" && !isPrimaryModifier(ev) && !ev.altKey) {
      ev.preventDefault();
      const clip = clipAtIndex(selectedIdx);
      if (clip) {
        await togglePinForClip(clip);
      }
      return;
    }

    if (key === "d" && !isPrimaryModifier(ev) && !ev.altKey) {
      ev.preventDefault();
      const clip = clipAtIndex(selectedIdx);
      if (clip) {
        await deleteClip(clip);
      }
      return;
    }

    if (key === "l" && !isPrimaryModifier(ev) && !ev.altKey) {
      ev.preventDefault();
      startLabelEdit(selectedIdx);
    }
  }

  async function togglePinForClip(clip: ClipRow): Promise<void> {
    const nextPinned = !clip.pinned;
    try {
      const ok = await invoke<boolean>("clips_set_pinned", {
        id: clip.id,
        pinned: nextPinned,
      });
      if (!ok) {
        flashHint("Could not update pin — clip missing?");
        return;
      }
      flashHint(nextPinned ? `Pinned #${clip.id}` : `Unpinned #${clip.id}`);
    } catch (err) {
      flashHint(String(err));
    }
  }

  async function deleteClip(clip: ClipRow): Promise<void> {
    try {
      const ok = await invoke<boolean>("clips_delete", { id: clip.id });
      if (!ok) {
        flashHint("Could not delete — clip missing?");
        return;
      }
      flashHint(`Deleted #${clip.id}`);
      await loadClips();
    } catch (err) {
      flashHint(String(err));
    }
  }

  async function onPinToggle(ev: MouseEvent, clip: ClipRow): Promise<void> {
    ev.preventDefault();
    ev.stopPropagation();
    const anchorId = clip.id;
    if (clipAtIndex(selectedIdx)?.id !== anchorId) {
      selectedIdx = displayClips.findIndex((c) => c.id === anchorId);
    }
    await togglePinForClip(clip);
    await loadClips({ anchorId });
  }

  async function onDeleteClick(ev: MouseEvent, clip: ClipRow): Promise<void> {
    ev.preventDefault();
    ev.stopPropagation();
    await deleteClip(clip);
  }

  function onLabelClick(ev: MouseEvent, idx: number): void {
    ev.preventDefault();
    ev.stopPropagation();
    selectedIdx = idx;
    startLabelEdit(idx);
  }

  function previewClick(ev: MouseEvent, idx: number): void {
    ev.preventDefault();
    selectedIdx = idx;
    void copyAtIndex(idx);
  }

  onMount(() => {
    document.addEventListener("keydown", onNavKeyCapture, true);
    document.addEventListener("keydown", onSlashShortcut);

    let unlisten: (() => void) | undefined;
    const unlistenPromise = listen("clips-updated", () => {
      void loadClips();
    }).then((u) => {
      unlisten = u;
    });

    void (async () => {
      try {
        const snap = await invoke<SettingsSnapshot>("settings_get");
        applySettingsSnapshot(snap);
      } catch {
        /* settings IPC unavailable outside Tauri */
      }
      await loadClips();
      await tick();
      await focusFilter();
    })();

    return () => {
      document.removeEventListener("keydown", onNavKeyCapture, true);
      document.removeEventListener("keydown", onSlashShortcut);
      void unlistenPromise.then(() => unlisten?.());
      clearTimeout(debounceTimer);
      clearTimeout(hintTimer);
    };
  });
</script>

<main class="wrap">
  {#if page === "settings"}
    <Settings
      onBack={leaveSettings}
      onHotkeyChange={(label) => {
        globalHotkeyLabel = label;
      }}
      onSettingsChange={applySettingsSnapshot}
    />
  {:else}
  <header class="head">
    <h1 class="logo">ESTM</h1>
    <div class="head-actions">
      <div class="help-wrap">
        <button
          type="button"
          class="btn-icon btn-help"
          aria-label="Keyboard shortcuts"
          aria-expanded={helpOpen}
          aria-controls="help-panel"
          title="Keyboard shortcuts"
          onclick={() => {
            helpOpen = !helpOpen;
          }}
        >
          <span class="help-icon" aria-hidden="true">?</span>
        </button>
        {#if helpOpen}
          <button
            type="button"
            class="help-backdrop"
            aria-label="Close keyboard shortcuts"
            onclick={() => {
              helpOpen = false;
            }}
          ></button>
          <div
            id="help-panel"
            class="help-panel"
            role="dialog"
            aria-labelledby="help-title"
          >
            <h2 id="help-title" class="help-title">Keyboard shortcuts</h2>
            <div class="kbd-hint">
              <p>
                <kbd>{globalHotkeyLabel}</kbd>
                toggle window globally
              </p>
              <p>Type in the filter to narrow results.</p>
              <p>
                <kbd>←</kbd>
                <kbd>→</kbd>
                switch view (Pinned · All · Labeled)
              </p>
              <p>
                <kbd>Enter</kbd>
                copies, hides picker, refocuses previous app (press <kbd>⌘V</kbd>
                to paste) · <kbd>Shift</kbd>+<kbd>Enter</kbd> copy and keep picker
                open
              </p>
              <p>
                <kbd>1</kbd>…<kbd>9</kbd>
                in the list · Shift keeps picker open
              </p>
              {#if quickPickPrefixEnabled}
                <p>
                  <kbd>{quickPickPrefix}</kbd>
                  then
                  <kbd>1</kbd>…<kbd>9</kbd>
                  pick from the filter (digits still searchable) · type
                  <kbd>{quickPickPrefix}{quickPickPrefix}</kbd>
                  for a literal prefix
                </p>
              {/if}
              <p>
                <kbd>↓</kbd>
                from filter into the list ·
                <kbd>↑</kbd>
                in filter opens this help · from top list row back to filter
              </p>
              <p>
                <kbd>↑</kbd>
                <kbd>↓</kbd>
                and
                <kbd>Enter</kbd>
                in the list to move and copy
              </p>
              <p>
                <kbd>L</kbd>
                label ·
                <kbd>P</kbd>
                pin/unpin ·
                <kbd>D</kbd>
                delete highlighted row
              </p>
              <p>
                <kbd>?</kbd>
                toggle this help (anywhere except the filter) ·
                <kbd>{isMac ? "⌘," : "Ctrl+,"}</kbd>
                settings
              </p>
              <p>
                <kbd>Esc</kbd>
                close help · clears filter (from list: back to filter)
              </p>
              <p>
                <kbd>/</kbd>
                or
                <kbd title="macOS: ⌘K">Ctrl+K</kbd>
                jump to filter
                <span class="kbd-muted">(mac: ⌘K)</span>
              </p>
            </div>
          </div>
        {/if}
      </div>
      <button
        type="button"
        class="btn-icon"
        aria-label="Open settings"
        title="Settings"
        onclick={openSettings}
      >
        <svg
          class="icon-gear"
          width="18"
          height="18"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            fill="currentColor"
            d="M12 15.5A3.5 3.5 0 0 1 8.5 12 3.5 3.5 0 0 1 12 8.5a3.5 3.5 0 0 1 3.5 3.5 3.5 3.5 0 0 1-3.5 3.5m7.43-2.53c.04-.32.07-.64.07-.97 0-.33-.03-.65-.07-.97l2.11-1.65a.5.5 0 0 0 .12-.64l-2-3.46a.5.5 0 0 0-.6-.22l-2.49 1a7.03 7.03 0 0 0-1.68-1.01l-.38-2.65A.5.5 0 0 0 14 2h-4a.5.5 0 0 0-.49.42l-.38 2.65a7.03 7.03 0 0 0-1.68 1.01l-2.49-1a.5.5 0 0 0-.6.22l-2 3.46a.5.5 0 0 0 .12.64l2.11 1.65c-.04.32-.07.64-.07.97 0 .33.03.65.07.97l-2.11 1.65a.5.5 0 0 0-.12.64l2 3.46a.5.5 0 0 0 .6.22l2.49-1c.52.4 1.08.73 1.68 1.01l.38 2.65a.5.5 0 0 0 .49.42h4a.5.5 0 0 0 .49-.42l.38-2.65c.6-.28 1.16-.61 1.68-1.01l2.49 1a.5.5 0 0 0 .6-.22l2-3.46a.5.5 0 0 0-.12-.64l-2.11-1.65Z"
          />
        </svg>
      </button>
    </div>
  </header>

  <div
    class="list-view-bar"
    role="tablist"
    tabindex="0"
    aria-label="Clip list view"
    onkeydown={(e) => {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        stepListView("left");
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        stepListView("right");
      }
    }}
  >
    {#each LIST_VIEW_ORDER as mode (mode)}
      <button
        type="button"
        class="list-view-tab"
        class:list-view-tab--active={listView === mode}
        role="tab"
        aria-selected={listView === mode}
        tabindex={listView === mode ? 0 : -1}
        onclick={() => setListView(mode)}
      >
        {LIST_VIEW_LABEL[mode]}
        <span class="list-view-count">{listViewCounts[mode]}</span>
      </button>
    {/each}
  </div>

  <section class="search-row">
    <input
      bind:this={searchEl}
      bind:value={query}
      type="text"
      placeholder="Filter clips and labels…"
      autocomplete="off"
      spellcheck="false"
      inputmode="search"
      id="clip-search"
      oninput={() => scheduleLoad()}
    />
  </section>

  <p id="status-hint" class="status-hint" role="status" aria-live="polite">
    {statusHintText}
  </p>

  <ul
    bind:this={listEl}
    id="clip-list"
    class="clip-list"
    role="listbox"
    aria-label={`${LIST_VIEW_LABEL[listView]} clips`}
    aria-live="polite"
  >
    {#each displayClips as clip, idx (clip.id)}
      <li
        class="clip-row"
        class:clip-row--pinned={clip.pinned}
        class:has-label={clipHasLabel(clip)}
        class:is-selected={idx === selectedIdx && rowCount > 0}
        role="option"
        tabindex="-1"
        data-clip-idx={idx}
        aria-selected={idx === selectedIdx ? "true" : "false"}
      >
        <div class="clip-row-head">
          {#if idx < QUICK_PICK_SLOTS}
            <span
              class="clip-rank"
              aria-label={quickPickPrefixEnabled
                ? `${quickPickPrefix} then ${idx + 1}`
                : `Row ${idx + 1}`}
            >
              {#if quickPickPrefixEnabled}
                <span class="clip-rank-dot" aria-hidden="true"
                  >{quickPickPrefix}</span
                >
              {/if}
              {idx + 1}
            </span>
          {/if}
          {#if editingLabelIdx === idx}
            <input
              type="text"
              class="label-input"
              data-label-idx={idx}
              bind:value={labelDraft}
              maxlength="80"
              placeholder="Label (Enter saves, Esc cancels)"
              aria-label="Clip label"
              onkeydown={(e) => void onLabelKeydown(e, idx)}
            />
          {:else if clipHasLabel(clip)}
            <div class="clip-row-titles">
              <span class="clip-title">{clip.label}</span>
              <span class="meta">
                {new Date(clip.createdMs).toLocaleString()} · #{clip.id}
                {#if clip.pinned}
                  · pinned
                {/if}
              </span>
            </div>
          {:else}
            <span class="meta meta-inline">
              {new Date(clip.createdMs).toLocaleString()} · #{clip.id}
              {#if clip.pinned}
                · pinned
              {/if}
            </span>
          {/if}
          <div class="clip-row-actions">
            <button
              type="button"
              class="btn-label"
              tabindex="-1"
              aria-label={clipHasLabel(clip) ? "Edit label" : "Add label"}
              onclick={(e) => onLabelClick(e, idx)}
            >
              Label
            </button>
            <button
              type="button"
              class="btn-pin"
              tabindex="-1"
              aria-label={clip.pinned ? "Unpin this clip" : "Pin this clip"}
              onclick={(e) => void onPinToggle(e, clip)}
            >
              {clip.pinned ? "Unpin" : "Pin"}
            </button>
            <button
              type="button"
              class="btn-delete"
              tabindex="-1"
              aria-label="Delete this clip from history"
              onclick={(e) => void onDeleteClick(e, clip)}
            >
              Delete
            </button>
          </div>
        </div>
        <button
          type="button"
          class="clip-preview-trigger"
          tabindex="-1"
          onclick={(e) => previewClick(e, idx)}
        >
          <pre class="clip-preview">{clipDisplayText(clip)}</pre>
        </button>
      </li>
    {/each}
  </ul>
  {/if}
</main>
