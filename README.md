# Kastanomata's Scroll of Sleuthing

A fast, local-first prep and print toolkit for Dungeon Masters. It turns art into printable tokens, standees, cards and tiled battle maps, keeps everything in a searchable library, and runs the table's music.

It is a ground-up Rust rewrite of *Orb of Many Functions* (Python/Tkinter), built with Tauri 2 and Svelte 5. Every feature is also exposed as an MCP tool, so an LLM can drive the app.

## Features
- **Library**: tokens, cut-outs and maps with tags, aliases and pins, full-text search, and import from Orb of Many Functions.
- **Tokens**:
  - Token Cutter: frame any image in a border ring (Roll20 gold/grey, thin versions, or your own PNGs), with background colour or eyedropper, pan/zoom framing, and colour/tone filters that touch the image but never the ring.
  - Token Sheet: printable PDF sheets by creature size, with copies and cut borders.
- **Cut-outs**: a lasso object cutter with contour outline, and flat or standee object sheets.
- **Cards**: a markdown card editor with an exact preview and duplex overflow printing.
- **Maps**:
  - Map Paginator: grid sizing (rows×cols, reference square, PPI, cell side), grid detection, perspective de-warp, embedded-grid removal, 14 print filters with 18 presets, and tiled PDF export with overlap, cut lines and a fewest-pages layout.
  - AoE templates: printable spell templates.
- **Music**: tracks, playlists, soundscapes, soundboards and a live mixer, with `.dmix` mix files.
- **Automation**: an MCP server (`sleuth-mcp`) and a CLI (`sleuth-cli`), both thin wrappers over the same service layer as the UI.

## Install (Windows)
Build the MSI installer (see below) and run it:

```powershell
cd app; npm run tauri build
# -> target/release/bundle/msi/Scroll of Sleuthing_<version>_x64_en-US.msi
```

The installer also registers `.dmix` files with the app.

Your data lives in `%APPDATA%\sleuth`: a SQLite database plus a content-addressed `blobs/` folder for images and PDFs. Set `SLEUTH_DATA_DIR` to use another folder.

## Build from source
Prerequisites:
- Rust stable (pinned by `rust-toolchain.toml`, with rustfmt and clippy)
- Visual Studio 2022 with the "Desktop development with C++" workload (MSVC and the Windows SDK)
- Node 20+
- WebView2, which ships with Windows 11

Run Rust commands from **PowerShell**. In Git Bash, `/usr/bin/link` shadows the MSVC linker.

```powershell
cd app; npm install; cd ..

cd app; npm run tauri dev      # desktop app (Vite on :5173 + Tauri)
cd app; npm run dev            # UI only, in a browser, against an in-memory mock backend

pwsh scripts/check.ps1         # full gate: fmt, clippy -D warnings, cargo test, svelte-check, vitest, Playwright
pwsh scripts/check.ps1 -Quick  # the same without the Tauri crate and the e2e tests
cargo run -p sleuth-cli -- --help
```

The Playwright tests use the installed Microsoft Edge, so no browser download is needed.

## MCP server
```powershell
cargo build -p sleuth-mcp --release
claude mcp add sleuth -- <repo>/target/release/sleuth-mcp.exe --data-dir <dir>
```

The tools are listed in [`docs/mcp-tools.md`](docs/mcp-tools.md). The MCP process and the app can share a data folder; every write lands in a change log, which keeps the UI in sync.

## Project layout
| Path | What it is |
|---|---|
| `crates/core` | Pure logic and image algorithms: no IO, no async |
| `crates/store` | SQLite (migrations, change log) and the blake3 blob store |
| `crates/service` | The only API: the UI, MCP and CLI all call it |
| `crates/pdf` | PDF output with one bundled font, so preview metrics match the PDF |
| `crates/mcp`, `crates/cli` | MCP server and command-line front ends |
| `crates/legacy` | Importer for Orb of Many Functions data |
| `app/` | Tauri 2 shell (`src-tauri`) and Svelte 5 UI (`src`) |
| `assets/` | Bundled token borders, card templates and fonts |
| `fixtures/parity` | Golden outputs from the Python app for parity tests |
| `docs/` | Architecture, data model, UX rules and one spec per feature |

## Documentation
- [`docs/architecture.md`](docs/architecture.md): the overall design.
- [`docs/features/`](docs/features): feature specs, each with a parity checklist and its intentional changes from the Python app.
- [`docs/roadmap.md`](docs/roadmap.md): phase status.
- [`CLAUDE.md`](CLAUDE.md): rules for contributors and coding agents.

## Status and branches
Work in progress, currently in alpha testing.
- `dev`: active development.
- `master`: receives the rewrite only once it fully matches Orb of Many Functions.
- `sprint/*` and `ui/*`: short-lived work branches that merge into `dev`.
