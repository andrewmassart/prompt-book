# Prompt Book

A desktop app for viewing Claude Code, Copilot CLI, and Codex CLI session transcripts. Built with Tauri v2, React, and Rust.

## Install

### macOS (Homebrew)

```bash
brew tap andrewmassart/tap
brew install prompt-book
```

### Windows / Manual

Download the latest installer from the [Releases](https://github.com/andrewmassart/prompt-book/releases) page.

## Features

- Auto-discovers sessions from `~/.claude/projects/`, `~/.copilot/session-state/`, and `~/.codex/sessions/`
- Parses Claude Code, Copilot CLI, and Codex CLI JSONL formats into a unified view
- Drag-and-drop or open arbitrary `.jsonl` files
- Collapsible message bubbles, tool call blocks, and thinking blocks
- Visual indicators for plan mode, auto/accept-edits mode, and sub-agent messages
- Export sessions as self-contained HTML files
- In-memory session caching for instant navigation

## Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/)

## Getting Started

```bash
git clone https://github.com/andrewmassart/prompt-book.git
cd prompt-book
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

Produces platform-specific installers in `src-tauri/target/release/bundle/`.
