use crate::rows::{row, section, Page, Section};

pub(crate) fn page() -> Page {
    Page {
        id: "accounts",
        title: "Accounts",
        sections: vec![accounts()],
    }
}

pub(crate) fn accounts() -> Section {
    section(
        "accounts",
        "Accounts",
        vec![
            row("accounts", "Accounts, usage stats and automatic continuation", "Current CLI login, Claude cswap, Codex xswap, update, reinstall or uninstall Claude Swap and Codex Swap, sidebar usage strip, status lines, usage limits and resets, account indicators, switching, hide emails, privacy, error recovery and retry settings."),
            row("claudeAutoRedeemExpiringResets", "Auto-redeem expiring Claude resets", "Ghostex automatically uses a banked Claude reset 5 minutes before it expires, so it isn't lost."),
            row("claudeAutoRedeemResetsAtLimit", "Also use expiring Claude resets at a limit", "When you hit a Claude usage limit and a banked reset expires within 24 hours, Ghostex automatically uses it right away instead of waiting for its last 5 minutes. Needs Auto-redeem expiring Claude resets."),
            row("codexAutoRedeemExpiringResets", "Auto-redeem expiring Codex resets", "Ghostex automatically uses a banked Codex reset 5 minutes before it expires, so it isn't lost."),
            row("codexAutoRedeemResetsAtLimit", "Also use expiring Codex resets at a limit", "When you hit a Codex usage limit and a banked reset expires within 24 hours, Ghostex automatically uses it right away instead of waiting for its last 5 minutes. Needs Auto-redeem expiring Codex resets."),
        ],
    )
}
