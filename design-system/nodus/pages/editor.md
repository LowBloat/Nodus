# Nodus — editor workspace override

This page override replaces the warm cream/amber direction from `MASTER.md` for the native editor workspace.

## Product idea

Nodus is a calm writing desk whose pages happen to travel directly between trusted devices. Notes are primary; networking is quiet infrastructure.

## Tokens

| Role | Value |
|---|---|
| App background | `#F7F8FA` |
| Paper | `#FFFFFF` |
| Sidebar | `#F0F2F6` |
| Ink | `#1B1F28` |
| Muted ink | `#676F7F` |
| Border | `#DCE1E9` |
| Connection blue | `#446EE7` |
| Synced green | `#237A57` |
| Warning | `#B54708` |

The dark shell uses `#13161C` for the workspace, `#161920` for navigation,
`#191D25` for raised surfaces, `#2D3340` for structural lines, `#EEF1F6`
for text, and `#7396FF` for connection actions. Hover and active surfaces step
up in luminance instead of turning every control blue.

Interface typography and the raw Markdown editor use Inter at 13–17px. The rendered note uses Source Serif 4 at 17–18px with a readable measure below 80 characters. Monospace is reserved for fenced code and pairing IDs.

## Layout

```text
┌──────────────── native titlebar / window controls ──────────────────┐
├ vault/search ───── write · split · read ───── status/theme/sync ────┤
├───────────────┬───────────────────────────────┬─────────────────────┤
│ vault         │ active note tab               │ synchronization     │
│ + new note    ├───────────────────────────────┤ status              │
│               │ contextual block toolbar      │ this device         │
│ folders       ├───────────────────────────────┤ trusted devices     │
│   note.md     │ readable editorial column     │ guided pairing      │
│ note.md       │                               │                     │
└───────────────┴───────────────────────────────┴─────────────────────┘
```

Everything is left aligned. The document body is centered inside the writing surface, but its text never is. Pairing expands only on request or when no peer exists.

The shell is drawn by egui in the native OS window. It contains no browser or
webview. On Windows the borderless chrome keeps native drag, minimize,
maximize, close, and edge/corner resize behavior.

## Interaction rules

- Editing starts a 700 ms debounce; expiry saves the active Markdown file and requests one sync pass.
- Changing the first H1 renames the Markdown file on that same debounce; the sidebar and breadcrumb follow the resulting filename.
- An active paragraph reserves one row per actual content line, avoiding vertical jumps between neighboring blocks.
- The block toolbar always reserves the same vertical measure; activating a block never moves the document.
- The file sidebar mirrors nested vault folders and keeps filenames as ordinary Markdown paths.
- At compact widths, side panels shrink and view-mode labels collapse to icons before the writing column is sacrificed.
- `Ctrl+S` persists and syncs immediately. The top bar reports `Salvando…` and `Salvo` instead of presenting save as a primary action.
- Switching notes flushes the current note before navigation.
- Closing with drafts open presents Save all, Discard, and Cancel actions.
- Remote saves can still arrive because the listener remains available.
- Pairing uses one invitation code: the receiving device asks for access and the device that issued the code must approve a modal showing the requester name and identity prefix.
- The vault selector is the workspace anchor. Adding a vault uses the native folder picker; nested vaults are rejected with an inline recovery message.
- Pairing identity, invitation code, and trusted devices belong to the active vault. Inactive vaults do not sync in the background.

## Self-critique

The original cream/amber palette, all-caps micro-labels, dark text fields, and monospace editor read as a generated developer console. This override removes those defaults. The distinctive move is restrained: serif writing inside a crisp connected-device shell. Repeated cards and ornamental badges are intentionally avoided.
