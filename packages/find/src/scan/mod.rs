//! Per-agent history scanners (port of zehn's `src/scan.zig`).
//!
//! | Agent    | History location                                                      |
//! |----------|-----------------------------------------------------------------------|
//! | claude   | `~/.claude/history.jsonl`                                             |
//! | codex    | `~/.codex/history.jsonl` and `~/.codex/sessions/**/*.jsonl`           |
//! | pi       | `~/.pi/agent/sessions/*/*.jsonl`                                      |
//! | opencode | `~/.local/share/opencode/opencode.db` (SQLite)                        |
//! | cursor   | `~/.cursor/projects/*/agent-transcripts/*/*.jsonl`                    |
//! | grok     | `~/.grok/sessions/*/*/chat_history.jsonl` plus sibling `summary.json` |
//! | empryo   | `<repo>/.empryo/sessions/*/session.jsonl` for each repo in `~/.empryo/threads.db` |
//!
//! CDXC:PromptSearch 2026-08-20:
//! opencode history used to be read by shelling out to the `sqlite3` CLI, so a
//! machine without it silently lost opencode results. The Rust port links
//! SQLite directly, which removes that external dependency instead of papering
//! over it with a skip-and-warn path.

mod claude;
mod codex;
mod cursor;
mod dates;
mod empryo;
mod files;
mod grok;
mod json;
mod opencode;
mod pi;
mod prompt_text;
mod records;
mod scanner;
#[cfg(test)]
mod tests;

pub use dates::*;
pub use empryo::*;
use files::*;
pub use grok::*;
pub use json::*;
use prompt_text::*;
pub use records::*;
pub use scanner::*;
