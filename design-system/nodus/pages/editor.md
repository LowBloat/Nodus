# Nodus — editor workspace override

This page override replaces the warm cream/amber direction from `MASTER.md` for the native editor workspace.

## Product idea

Nodus is a calm writing desk whose pages happen to travel directly between trusted devices. Notes are primary; networking is quiet infrastructure.

## Tokens

| Role | Value |
|---|---|
| App background | `#F4F7FB` |
| Paper | `#FFFFFF` |
| Sidebar | `#EAF0F6` |
| Ink | `#182230` |
| Muted ink | `#626C81` |
| Border | `#DCE3EC` |
| Connection blue | `#3267E3` |
| Synced green | `#237A57` |
| Warning | `#B54708` |

Interface typography and the raw Markdown editor use Inter at 13–17px. The rendered note uses Source Serif 4 at 17–18px with a readable measure below 80 characters. Monospace is reserved for fenced code and pairing IDs.

## Layout

```text
┌ Library, 248 ┐┌──────────── writing surface ────────────┐┌ Sync, 292 ┐
│ Nodus        ││ note title        Saved   Edit Preview ││ status     │
│ search       ││                                          ││ device     │
│ notes        ││        readable editorial column         ││ peers      │
│              ││                                          ││ + connect  │
└──────────────┘└──────────────────────────────────────────┘└────────────┘
```

Everything is left aligned. The document body is centered inside the writing surface, but its text never is. Pairing expands only on request or when no peer exists.

## Interaction rules

- Editing only changes an in-memory draft and visibly marks the note as unsaved.
- `Ctrl+S` and the Save button persist the active draft, acknowledge success, and request one sync pass.
- No timer initiates outbound sync.
- Switching notes preserves each unsaved draft in memory.
- Closing with drafts open presents Save all, Discard, and Cancel actions.
- Remote saves can still arrive because the listener remains available.
- Pairing uses one invitation code: the receiving device asks for access and the device that issued the code must approve a modal showing the requester name and identity prefix.

## Self-critique

The original cream/amber palette, all-caps micro-labels, dark text fields, and monospace editor read as a generated developer console. This override removes those defaults. The distinctive move is restrained: serif writing inside a crisp connected-device shell. Repeated cards and ornamental badges are intentionally avoided.
