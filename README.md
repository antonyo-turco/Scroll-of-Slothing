# Kastanomata's Scroll of Sleuthing

A fast, local-first tabletop prep toolkit for Dungeon Masters, written in Rust (Tauri 2 + Svelte 5).
It is a ground-up rewrite of *Orb of Many Functions* (Python/Tkinter).

## Features
- **Library**: assets (tokens, cut-outs, maps) with tags, aliases, pins, campaigns and sessions
- **Tokens**: a token cutter (border ring, background, pan/zoom framing) and printable token sheets
- **Cut-outs**: a lasso object cutter with contour outline, and flat or standee object sheets
- **Cards**: a markdown card editor with exact preview and duplex overflow printing
- **Maps**: a map paginator (grid fitting, de-warp, grid removal, filters, tiled PDF) and AoE templates
- **LLM-ready**: every feature is exposed as an MCP tool
- **Music**: specified in `docs/features/music/`, not implemented yet

## Status
Work in progress. See `docs/roadmap.md` for the phases, `docs/` for the specification, and `CLAUDE.md` for contributor/agent notes.

## Development (Windows)
Prerequisites:
- Rust stable
- Visual Studio 2022 with the "Desktop development with C++" workload (MSVC and the Windows SDK)
- Node 20+
- WebView2, which ships with Windows 11

```powershell
cd app; npm install; cd ..
pwsh scripts/check.ps1         # full quality gate
cd app; npm run tauri dev      # run the desktop app
```
