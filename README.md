# ESTM

**ESTM** is pronounced like *esteem*. The name stands for **external short-term memory**: the idea that your clipboard holds the small pieces you are juggling right now—snippets you still need—but outside your head, so you can switch tasks without losing them.

> **Disclaimer** — Much of this project was written with AI assistance. It is an early prototype, not audited for security or reliability. ESTM reads and stores your clipboard contents locally. **Use at your own risk.** Do not use it for secrets you cannot afford to expose, and review the code yourself before trusting it with sensitive data.

## Status (v0.1 prototype)

**Tauri v2 + Vite + Svelte 5 + TypeScript** clipboard manager. Plaintext history works end-to-end; several v1 checklist items are still open (see [Not yet](#not-yet)).

### Shipped

| Area | What you get |
|------|----------------|
| **History** | SQLite-backed plaintext clips; background `NSPasteboard` watcher; consecutive duplicate suppression (normalized fingerprint). Data under `~/Library/Application Support/fun.estm.ESTM/` (`estm.sqlite`, `settings.json`). |
| **Quick picker** | Compact window with type-to-filter search; global shortcut **toggles** show/hide (default **⌘⇧E**, configurable in Settings). |
| **Pick & refocus** | Choosing a clip copies to the system clipboard, hides the picker, and restores the previous frontmost app — you press **⌘V** to paste. **Enter** or row click: copy + hide + refocus. **Shift+Enter**: copy and keep the picker open. |
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
| 3 | Paste without app switching | ✅ Copy + refocus; you **⌘V** (no synthetic paste) |
| 4 | Menu bar |  Icon + open/quit only |
| 5 | Persistence rules | ✅ Configurable |
| 7 | Pin / favorites | ✅ |
| 8 | Merge / edit | ⛔ Not planned |
| 9 | Duplicate suppression | ✅ |
| 10 | Formats | Text only |
| 11 | Keyboard-first | ✅ |

### Permissions

Grant **System Settings → Privacy & Security → Accessibility** for ESTM if the global picker shortcut requires it on your macOS version. Picking a clip does **not** simulate **⌘V** — only clipboard + refocus.

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

### CI

| Workflow | When | What |
|----------|------|------|
| [ci.yml](.github/workflows/ci.yml) | Every **pull request** | `npm run build`, `cargo check`, `cargo clippy` on macOS |
| [release-macos.yml](.github/workflows/release-macos.yml) | Push to **`release`** (or manual) | Builds ad-hoc signed **`.dmg`** (Apple Silicon); workflow **Artifact** + **GitHub Release** (`v<version>-build.<run>`) |

#### Installing a release build

CI builds are **not notarized** (no Apple Developer ID yet). macOS may say the app is **“damaged”** (`beschädigt`) when you download it from GitHub — that is Gatekeeper + quarantine, not a broken bundle.

After opening the `.dmg` and copying **ESTM.app** to Applications:

```bash
xattr -cr /Applications/ESTM.app
```

Then start it once via **right-click → Open** (not double-click). Later launches work normally. Grant **Privacy & Security → Accessibility** when prompted if needed for the global shortcut.

Recommended editor extensions: **Tauri**, **rust-analyzer**, **Better TOML** for manifests.
