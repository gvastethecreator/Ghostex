/// The `<style id="ghostex-theme">` every published page starts with: Ghostex's default dark
/// palette, the light palette when the viewer's system is light, and a plain base so an unstyled
/// page already reads like the app. It comes first in `<head>`, so the page's own CSS wins.
///
/// Neutrals come from the app's default theme (`packages/core-ui/styles/theme.css` for dark,
/// `packages/core-ui/styles/modals-light.css` for light); status colors from the chat's status
/// tones (`packages/gx-chat-core/visual/status-tone.json`).
pub(super) const THEME_STYLE: &str = concat!(
    "<style id=\"ghostex-theme\">",
    ":root{color-scheme:dark;",
    "--background:#0e0e0e;--foreground:#c8cdd5;",
    "--muted:#2a2a2a;--muted-foreground:#747b85;",
    "--card:#252525;--card-foreground:#c8cdd5;",
    "--border:rgba(255,255,255,0.11);--input:rgba(255,255,255,0.15);--ring:#737373;",
    "--primary:#7da4f8;--primary-foreground:#0e0e0e;",
    "--accent:#2a2a2a;--accent-foreground:#e5e7eb;",
    "--destructive:#fb7185;--warning:#fbbf24;--success:#34d399;--info:#86d3f8;",
    "--code-background:#161616;--code-foreground:#e6edf3;",
    "--chart-1:#60a5fa;--chart-2:#f59e0b;--chart-3:#34d399;",
    "--chart-4:#f472b6;--chart-5:#a78bfa;--chart-6:#22d3ee;",
    "--radius:8px;",
    "--font-sans:-apple-system, BlinkMacSystemFont, \"Segoe UI\", system-ui, sans-serif;",
    "--font-mono:\"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace}",
    "@media (prefers-color-scheme: light){:root{color-scheme:light;",
    "--background:#ffffff;--foreground:#262626;",
    "--muted:#f1f1f1;--muted-foreground:#626262;",
    "--card:#ffffff;--card-foreground:#262626;",
    "--border:rgba(0,0,0,0.14);--input:rgba(0,0,0,0.16);--ring:#737373;",
    "--primary:#2563eb;--primary-foreground:#ffffff;",
    "--accent:#e9e9e9;--accent-foreground:#262626;",
    "--destructive:#b91c1c;--warning:#b45309;--success:#047857;--info:#0369a1;",
    "--code-background:#f6f8fa;--code-foreground:#1f2328;",
    "--chart-1:#2563eb;--chart-2:#d97706;--chart-3:#059669;",
    "--chart-4:#db2777;--chart-5:#7c3aed;--chart-6:#0891b2}}",
    "html{background:var(--background);color:var(--foreground);font-family:var(--font-sans);",
    "font-size:14px;line-height:1.5;-webkit-font-smoothing:antialiased}",
    "body{margin:0;padding:16px}",
    "code,kbd,pre,samp{font-family:var(--font-mono)}",
    "</style>",
);
