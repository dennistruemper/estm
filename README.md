# ESTM

**ESTM** is pronounced like *esteem*. The name stands for **external short-term memory**: the idea that your clipboard holds the small pieces you are juggling right now—snippets you still need—but outside your head, so you can switch tasks without losing them.

## Status (v0.1 prototype)

**Tauri v2 + Vite + Svelte 5 + TypeScript** clipboard manager. Plaintext history works end-to-end; several v1 checklist items are still open (see [Not yet](#not-yet)).

### Shipped

| Area | What you get |
|------|----------------|
| **History** | SQLite-backed plaintext clips; background `NSPasteboard` watcher; consecutive duplicate suppression (normalized fingerprint). Data under `~/Library/Application Support/fun.estm.ESTM/` (`estm.sqlite`, `settings.json`). |
| **Quick picker** | Compact window with type-to-filter search; global shortcut **toggles** show/hide (default **⌘⇧E**, configurable in Settings). |
| **Pick & paste** | Choosing a clip copies to the system clipboard, hides the picker, restores the previous frontmost app, then simulates **⌘V** (~150 ms later). **Enter** or row click: copy + hide + paste. **Shift+Enter**: copy and keep the picker open (no paste). |
| **Menu bar** | Tray icon → **Open picker…**; **Quit**. No “recent clips” submenu yet. |
| **Retention** | Settings: max entry count (default **500**), optional max age in days for **unpinned** clips (default **14**), or count-only (age off). Pinned clips are exempt from age eviction. **Clear history** in Settings. |
| **Pins & labels** | Pin/unpin per row; optional label (search matches label + body; labels do not auto-pin). **Pinned · All · Labeled** views with **←/→** from the filter. |
| **Keyboard UX** | List **1–9** quick-pick; optional **prefix + 1–9** from the filter (off by default, prefix configurable). **`/`** or **⌘K** → filter; **↓** filter → list; **↑** in filter opens **`?`** help; **Esc** closes help / clears filter / list → filter; **L** label · **P** pin · **D** delete; **`?`** toggles help (not while typing in filter); **⌘,** → Settings; **Esc** leaves Settings. In-app **`?`** panel lists shortcuts. |
| **Settings** | Picker hotkey (modifiers + letter), prefix quick-pick, history limits — persisted and applied immediately (prune on change). |

**IPC (Tauri):** `clips_recent`, `clips_search`, `clips_clear`, `clips_copy`, `clips_set_pinned`, `clips_set_label`, `clips_delete`, `picker_hide`, `settings_get`, `settings_set_picker_hotkey`, `settings_set_quick_pick_prefix`, `settings_set_history_limits`. Frontend listens for **`clips-updated`**.



Checklist reference (item **8** omitted by design):

| # | Feature | Status |
|---|---------|--------|
| 1 | History | ✅ Plaintext |
| 2 | Quick picker | ✅ |
| 3 | Paste without app switching | ✅ |
| 4 | Menu bar |  Icon + open/quit only |
| 5 | Persistence rules | ✅ Configurable |
| 7 | Pin / favorites | ✅ |
| 8 | Merge / edit | ⛔ Not planned |
| 9 | Duplicate suppression | ✅ |
| 10 | Formats | Text only |
| 11 | Keyboard-first | ✅ |

### Permissions

Grant **System Settings → Privacy & Security → Accessibility** for ESTM so the global picker shortcut, focus restore, and simulated **⌘V** after a pick work reliably.

---

## Development

### Prerequisites

- **Rust** — [rustup](https://rustup.rs/); ensure `cargo` is on `PATH`.
- **Xcode Command Line Tools**
- **Node.js** — npm (see `package-lock.json`)

### Commands

```bash
npm install          # frontend + @tauri-apps/cli
npm run tauri dev    # Vite dev server + Rust watch
npm run tauri build  # release .app bundle
```

Recommended editor extensions: **Tauri**, **rust-analyzer**, **Better TOML** for manifests.
