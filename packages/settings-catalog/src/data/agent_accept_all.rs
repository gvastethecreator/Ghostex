use crate::json::{opt, Opt, J};

/// CDXC:AgentLauncher 2026-06-11-17:08:
/// Accept All selects must render the user-facing option label while collapsed,
/// not the raw mode key. Base UI needs root item metadata before the popup is
/// mounted, so keep the labels next to the mode type and reuse them in Settings
/// and standalone agent configuration.
pub const AGENT_ACCEPT_ALL_MODE_SELECT_ITEMS: &[Opt] = &[
    opt("Use app default", "inherit"),
    opt("Skip permissions", "enabled"),
    opt("Keep default", "disabled"),
];

/// CDXC:AgentLauncher 2026-05-19-10:05:
/// Flags were verified from each vendor CLI `--help` on 2026-05-19. Antigravity
/// CLI (`agy`) uses `--dangerously-skip-permissions` for Accept All. Factory Droid
/// interactive mode has no skip flag; only `droid exec` exposes
/// `--skip-permissions-unsafe`, so the default `droid` launcher stays unsupported.
///
/// CDXC:AgentLauncher 2026-06-09-14:22:
/// OpenCode TUI does not expose a permission-bypass CLI flag. Keep it supported
/// through gxserver's runtime permission config path so macOS Settings can show
/// the same Accept All control without claiming the stored command gets a flag.
pub const AGENT_ACCEPT_ALL_SPECS: J = J::Obj(&[
    (
        "antigravity",
        J::Obj(&[
            ("kind", J::Str("flag")),
            (
                "aliases",
                J::Arr(&[J::Str("--dangerously-skip-permissions")]),
            ),
            ("canonicalFlag", J::Str("--dangerously-skip-permissions")),
        ]),
    ),
    (
        "amp",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("--dangerously-allow-all")])),
            ("canonicalFlag", J::Str("--dangerously-allow-all")),
        ]),
    ),
    ("mastra", J::Obj(&[("kind", J::Str("runtimeConfig"))])),
    ("zcode", J::Null),
    ("freebuff", J::Null),
    (
        "claude",
        J::Obj(&[
            ("kind", J::Str("flag")),
            (
                "aliases",
                J::Arr(&[J::Str("--dangerously-skip-permissions")]),
            ),
            ("canonicalFlag", J::Str("--dangerously-skip-permissions")),
        ]),
    ),
    (
        "codex",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("--yolo")])),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    ("codebuddy", J::Null),
    (
        "command-code",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("--yolo")])),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    (
        "copilot",
        J::Obj(&[
            ("kind", J::Str("flag")),
            (
                "aliases",
                J::Arr(&[J::Str("--allow-all"), J::Str("--yolo")]),
            ),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    (
        "cursor",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("--force"), J::Str("--yolo")])),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    ("devin", J::Null),
    ("droid", J::Null),
    // CDXC:AgentProviders 2026-10-06 DECISION: "The approvals toggle maps to an Empryo launch flag only if one exists. Otherwise the toggle is hidden for Empryo, and Empryo's own `yolo` setting stays the user's business." Empryo 3.9.0-beta has no such flag (only `/yolo` and the `yolo` config key).
    ("empryo", J::Null),
    (
        "gemini",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("-y"), J::Str("--yolo")])),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    (
        "grok",
        J::Obj(&[
            ("kind", J::Str("flag")),
            ("aliases", J::Arr(&[J::Str("--always-approve")])),
            ("canonicalFlag", J::Str("--always-approve")),
        ]),
    ),
    ("hermes-agent", J::Null),
    (
        "kimi",
        J::Obj(&[
            ("kind", J::Str("flag")),
            (
                "aliases",
                J::Arr(&[
                    J::Str("--auto-approve"),
                    J::Str("--yes"),
                    J::Str("-y"),
                    J::Str("--yolo"),
                ]),
            ),
            ("canonicalFlag", J::Str("--yolo")),
        ]),
    ),
    ("kiro", J::Null),
    ("omp", J::Null),
    (
        "openclaude",
        J::Obj(&[
            ("kind", J::Str("flag")),
            (
                "aliases",
                J::Arr(&[J::Str("--dangerously-skip-permissions")]),
            ),
            ("canonicalFlag", J::Str("--dangerously-skip-permissions")),
        ]),
    ),
    ("opencode", J::Obj(&[("kind", J::Str("runtimeConfig"))])),
    ("pi", J::Null),
    ("qoder", J::Null),
    ("rovodev", J::Null),
]);
