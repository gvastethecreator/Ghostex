use std::ops::Range;

use super::{is_horizontal_rule, is_titled_horizontal_rule, strip_ansi_sgr};

/// CDXC:SessionChat 2026-09-15 DECISION:
/// User: support sending ! shell commands from Chat for Claude agents, like Codex.
/// Claude replaces its normal prompt glyph with !, so readiness and draft readers must accept both inside the input frame.
pub(super) const CLAUDE_COMPOSER_MARKERS: &[char] = &['❯', '!'];

/// CDXC:SessionChat 2026-09-08 WHY:
/// Rewind restores wrapped drafts in both Claude and Codex. Claude's former three-row signature rejected those drafts before Send could clear them.
/// Readiness, rewind and replacement must agree on the whole input region, including empty and continued rows.
pub(super) fn rule_input_region(lines: &[String], markers: &[char]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_titled_horizontal_rule(line))?;
    let start = (head + 1..foot).find(|&i| !lines[i].trim().is_empty())?;
    lines[start]
        .trim_start()
        .starts_with(markers)
        .then_some(start..foot)
}

/// CDXC:SessionChat 2026-09-11 WHY:
/// Clearing must read every logical row: Hermes can prefix its marker with a profile, Pi has no marker, and OMP paints its final input row inside the bottom corners.
/// Readiness uses these same regions so a populated multiline draft remains eligible for clearing.
pub(super) fn unmarked_rule_input_region(lines: &[String]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| is_horizontal_rule(line))?;
    let head = lines[..foot]
        .iter()
        .rposition(|line| is_titled_horizontal_rule(line))?;
    Some(head + 1..foot)
}

/// ZCode 3.11.2-24 uses Pi's unmarked editor, followed by its model/mode footer.
/// Require that footer immediately after the editor so a rule inside a picker cannot qualify.
pub(super) fn zcode_input_region(lines: &[String]) -> Option<Range<usize>> {
    let region = unmarked_rule_input_region(lines)?;
    let footer = lines.get(region.end + 1)?.trim();
    (footer.starts_with('◈')
        && footer.contains("◉")
        && footer.contains("⚡")
        && lines[region.end + 2..]
            .iter()
            .all(|line| line.trim().is_empty()))
    .then_some(region)
}

/// CDXC:AgentScreenDetection 2026-10-04 WHY:
/// Freebuff 0.2.12 draws its input as the last rounded box above its footer (`<model> · <folder> · /model to change · Chat: …`, then `← for history · ? for help`), but its ads and its `ask_user` form use the same rounded box. The form puts a nested Submit box and an `↑↓ navigate` hint inside, so a box holding either is not the input.
pub(super) fn freebuff_input_region(lines: &[String]) -> Option<Range<usize>> {
    let help = lines
        .iter()
        .rposition(|line| line.contains("for history") && line.contains("for help"))?;
    let foot = lines[..help].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╰') && line.ends_with('╯')
    })?;
    if lines[foot + 1..help]
        .iter()
        .any(|line| line.trim_start().starts_with(['│', '╭', '╰']))
    {
        return None;
    }
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╭') && line.ends_with('╮')
    })?;
    lines[head + 1..foot]
        .iter()
        .all(|line| {
            let line = line.trim();
            line.starts_with('│')
                && line.ends_with('│')
                && !line[3..].contains(['╭', '╰'])
                && !line.contains("↑↓ navigate")
        })
        .then_some(head + 1..foot)
}

const FREEBUFF_PLACEHOLDERS: &[&str] = &[
    "enter a coding task or / for commands",
    "enter a coding task",
    "add to the current task (/ for commands)",
    "add to the current task",
    "ctrl-c to cancel queued messages",
    "enter bash command...",
    "describe a feature/bug or other request to be fleshed out...",
    "add instructions for this skill, or press enter to run it as-is...",
    "describe what you want to plan...",
    "describe what to review...",
    "enter image path or ctrl+v to paste",
];

/// The typed text of Freebuff's input box. An empty input shows the cursor `▍` followed by a
/// dim placeholder; typed text shows the cursor where it was left.
fn freebuff_composer_input(lines: &[String]) -> Option<SessionChatComposerInput> {
    let region = freebuff_input_region(lines)?;
    let rows: Vec<String> = lines[region.clone()]
        .iter()
        .map(|line| {
            let line = line.trim();
            let line = line.strip_prefix('│').unwrap_or(line);
            line.strip_suffix('│').unwrap_or(line).trim().to_string()
        })
        .collect();
    let first = rows.iter().position(|row| !row.is_empty());
    let last = rows.iter().rposition(|row| !row.is_empty());
    let rows = match (first, last) {
        (Some(first), Some(last)) => &rows[first..=last],
        _ => &rows[..0],
    };
    let placeholder = rows.len() == 1
        && rows[0].starts_with('▍')
        && FREEBUFF_PLACEHOLDERS
            .contains(&rows[0]['▍'.len_utf8()..].trim().to_lowercase().as_str());
    let text = if placeholder {
        String::new()
    } else {
        rows.iter()
            .map(|row| row.replace('▍', ""))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    };
    Some(SessionChatComposerInput {
        text,
        rows: region.len(),
        shell_mode: false,
        placeholder,
    })
}

pub(super) fn hermes_input_region(lines: &[String]) -> Option<Range<usize>> {
    let region = unmarked_rule_input_region(lines)?;
    let start = region.clone().find(|&i| !lines[i].trim().is_empty())?;
    super::is_profiled_marker_line(&lines[start], '❯').then_some(start..region.end)
}

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's box composer draws its statusline into the top border, and that line changes with the symbol preset (`π >` with Unicode symbols, `󰵗` and powerline glyphs with Nerd Font) and turns into a spinner mid-turn. Requiring `π` and `>` there read a Nerd Font OMP as never ready, so its first chat message waited in the queue forever.
/// The frame is the signature instead: OMP merges the input's last row into the foot (`╰─ text ─╯`), while its dialogs and welcome card close with a solid box rule.
pub(super) fn omp_input_region(lines: &[String]) -> Option<Range<usize>> {
    omp_box_input_region(lines).or_else(|| omp_band_input_region(lines))
}

fn omp_box_input_region(lines: &[String]) -> Option<Range<usize>> {
    let foot = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let interior = lines[foot].trim().strip_prefix('╰')?.strip_suffix('╯')?;
    if !interior.starts_with('─')
        || interior
            .chars()
            .all(|c| ('\u{2500}'..='\u{257f}').contains(&c))
    {
        return None;
    }
    let head = lines[..foot].iter().rposition(|line| {
        let line = line.trim();
        line.starts_with('╭') && line.ends_with('╮')
    })?;
    if !lines[head + 1..foot].iter().all(|line| {
        let line = line.trim();
        line.starts_with('│') && (line.ends_with('│') || line.ends_with('█'))
    }) {
        return None;
    }
    Some(head + 1..foot + 1)
}

/// The cue OMP's status band composer draws at column 0 of the input's first row; later rows are indented three spaces.
const OMP_BAND_CUE: &str = "╰─";

/// CDXC:AgentScreenDetection 2026-10-07 WHY:
/// OMP 18.0.10 made the status band its default composer shape (`composer.shape: band`, also offered first by its setup wizard): the statusline is a flush row above an unframed input whose first row starts with `╰─ ` and has no `╭` head or `╯` corner. Reading only the rounded box called every default OMP "not on screen yet", so chat messages never reached its terminal and users pasted them in by hand.
/// The cue is OMP's own literal gutter, not a themed glyph, and its overlays and tool frames close with `╰───╯` rules, so a `╰─` row whose rest is not a rule, with no frame drawn under it, is the live input. The row above it is the band (empty while the statusline starts). The slash-command list OMP opens under the input is not indented, so the input ends at the last indented row.
fn omp_band_input_region(lines: &[String]) -> Option<Range<usize>> {
    let last = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let cue = lines[..=last]
        .iter()
        .rposition(|line| line.starts_with(OMP_BAND_CUE))?;
    let rest = lines[cue][OMP_BAND_CUE.len()..].trim_end();
    if cue == 0
        || rest.starts_with(|c| ('\u{2500}'..='\u{257f}').contains(&c))
        || rest.ends_with('╯')
        || lines[cue + 1..=last]
            .iter()
            .any(|line| line.trim_start().starts_with(['╭', '╰', '│']))
    {
        return None;
    }
    let rows = lines[cue + 1..=last]
        .iter()
        .take_while(|line| line.starts_with("   ") || line.trim().is_empty())
        .count();
    let end = (cue + 1..=cue + rows)
        .rfind(|&row| !lines[row].trim().is_empty())
        .unwrap_or(cue);
    Some(cue..end + 1)
}

/// CDXC:AgentScreenDetection 2026-10-01 WHY:
/// OMP's empty composer shows one right-aligned gesture hint in its foot (`╰─  ⇧⇥ to change thinking effort ─╯`, or `󰘶 󰌒 …` with Nerd Font symbols): the key as one accent-colored span, the label italic. Reading it as a draft failed every send with "could not be cleared". Typed input is never italic, so an italic label with only one key span before it is the hint.
fn omp_hint_only(line: &StyledLine) -> bool {
    let is_chrome = |ch: char| ch.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&ch);
    let Some(label) = line
        .chars
        .iter()
        .position(|(ch, style)| style.italic && !is_chrome(*ch))
    else {
        return false;
    };
    let mut key = line.chars[..label]
        .iter()
        .filter(|(ch, _)| !is_chrome(*ch))
        .map(|(_, style)| (style.foreground_rgb, style.foreground_index));
    let key_color = key.next();
    key.all(|color| Some(color) == key_color)
        && line.chars[label..]
            .iter()
            .all(|(ch, style)| style.italic || is_chrome(*ch))
}

/// CDXC:AgentScreenDetection 2026-09-09 WHY:
/// Cursor 2026.09.08 renders either half-block borders or a background-filled input with blank padding, depending on terminal capabilities.
/// The borderless layout must have its model/usage footer and context footer below the input, with no open menu after them; an arrow alone also appears in pickers.
pub(super) fn cursor_input_region(lines: &[String]) -> Option<Range<usize>> {
    if let Some(foot) = lines
        .iter()
        .rposition(|line| super::is_frame_rule(line, '\u{2580}', 9, 10))
    {
        if let Some(head) = lines[..foot]
            .iter()
            .rposition(|line| super::is_frame_rule(line, '\u{2584}', 9, 10))
        {
            let start = (head + 1..foot).find(|&i| lines[i].trim_start().starts_with('→'))?;
            return Some(start..foot);
        }
    }
    let footer = lines
        .iter()
        .rposition(|line| crate::session_chat_options::match_cursor_statusline(line).is_some())?;
    let tail: Vec<_> = lines[footer + 1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if tail.len() != 1 || !tail[0].contains("Ctx ") || !tail[0].trim_end().ends_with("% used") {
        return None;
    }
    let start = lines[..footer]
        .iter()
        .rposition(|line| line.trim_start().starts_with('→'))?;
    if start == 0 || !lines[start - 1].trim().is_empty() || !lines[footer - 1].trim().is_empty() {
        return None;
    }
    let end = (start + 1..footer)
        .rfind(|&i| !lines[i].trim().is_empty())
        .map_or(start + 1, |i| i + 1);
    Some(start..end)
}

pub fn claude_composer_draft(screen: &str) -> Option<String> {
    if super::is_claude_code_agents_screen(screen) {
        return None;
    }
    let lines: Vec<_> = screen.lines().map(strip_ansi_sgr).collect();
    let region = rule_input_region(&lines, CLAUDE_COMPOSER_MARKERS)?;
    let first = lines[region.start].trim_start();
    let text = first.strip_prefix('❯').unwrap_or(first);
    Some(
        std::iter::once(text.trim())
            .chain(
                lines[region.start + 1..region.end]
                    .iter()
                    .map(|line| line.trim()),
            )
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[derive(Debug)]
pub struct SessionChatComposerInput {
    pub text: String,
    pub rows: usize,
    /// An empty shell editor still needs to return to normal mode before replacement.
    pub shell_mode: bool,
    placeholder: bool,
}

impl SessionChatComposerInput {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
            || self.placeholder
            || (self.shell_mode && self.text.trim() == "!")
    }
}

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    faint: bool,
    italic: bool,
    inverse: bool,
    underline: bool,
    blink: bool,
    hidden: bool,
    crossed_out: bool,
    foreground_rgb: Option<[u16; 3]>,
    foreground_index: Option<u16>,
    background_rgb: Option<[u16; 3]>,
}

struct StyledLine {
    chars: Vec<(char, Style)>,
    text: String,
}

/// CDXC:SessionChat 2026-09-08 WHY:
/// Plain captures make Codex's empty placeholder indistinguishable from a draft. VT captures preserve its faint style and distinguish the live bold prompt from dim transcript echoes, without depending on placeholder wording.
fn styled_lines(screen: &str) -> Vec<StyledLine> {
    let mut style = Style::default();
    screen
        .lines()
        .map(|line| {
            let mut chars = line.chars().peekable();
            let mut visible = Vec::new();
            while let Some(ch) = chars.next() {
                if ch != '\u{1b}' {
                    visible.push((ch, style));
                    continue;
                }
                if chars.next() != Some('[') {
                    continue;
                }
                let mut parameters = String::new();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        if next == 'm' {
                            let values: Vec<_> = parameters
                                .split(';')
                                .map(|p| p.parse::<u16>().unwrap_or(0))
                                .collect();
                            let mut index = 0;
                            while index < values.len() {
                                match values[index] {
                                    0 => style = Style::default(),
                                    1 => style.bold = true,
                                    2 => style.faint = true,
                                    3 => style.italic = true,
                                    23 => style.italic = false,
                                    4 | 21 => style.underline = true,
                                    24 => style.underline = false,
                                    5 | 6 => style.blink = true,
                                    25 => style.blink = false,
                                    8 => style.hidden = true,
                                    28 => style.hidden = false,
                                    9 => style.crossed_out = true,
                                    29 => style.crossed_out = false,
                                    7 => style.inverse = true,
                                    27 => style.inverse = false,
                                    30..=37 | 39 | 90..=97 => {
                                        style.foreground_rgb = None;
                                        style.foreground_index = match values[index] {
                                            30..=37 => Some(values[index] - 30),
                                            90..=97 => Some(values[index] - 90 + 8),
                                            _ => None,
                                        };
                                    }
                                    40..=47 | 49 | 100..=107 => style.background_rgb = None,
                                    22 => {
                                        style.bold = false;
                                        style.faint = false;
                                    }
                                    38 | 48 | 58 => {
                                        let rgb = if values.get(index + 1) == Some(&2) {
                                            values
                                                .get(index + 2..index + 5)
                                                .and_then(|rgb| rgb.try_into().ok())
                                        } else {
                                            None
                                        };
                                        match values[index] {
                                            38 => {
                                                style.foreground_rgb = rgb;
                                                style.foreground_index =
                                                    if values.get(index + 1) == Some(&5) {
                                                        values.get(index + 2).copied()
                                                    } else {
                                                        None
                                                    };
                                            }
                                            48 => style.background_rgb = rgb,
                                            _ => {}
                                        }
                                        index += match values.get(index + 1) {
                                            Some(2) => 4,
                                            Some(5) => 2,
                                            _ => 0,
                                        };
                                    }
                                    _ => {}
                                }
                                index += 1;
                            }
                        }
                        break;
                    }
                    parameters.push(next);
                }
            }
            StyledLine {
                text: visible.iter().map(|(ch, _)| *ch).collect(),
                chars: visible,
            }
        })
        .collect()
}

/// CDXC:AgentScreenDetection 2026-09-14 WHY:
/// Codex's sparkle.rs blends eight single-dot Braille glyphs with the terminal palette, so light and tinted themes are not grayscale. Stars have explicit RGB foreground/background and no text modifiers; typed Braille inherits the default foreground.
/// Effort animations tint columns independently, so a star's background need not equal the prompt's background.
fn clear_codex_composer_particles(lines: &mut [StyledLine]) {
    let Some(start) = codex_composer_start(lines) else {
        return;
    };
    let Some(background) = lines[start].chars[0].1.background_rgb else {
        return;
    };
    let start = (0..start)
        .rev()
        .find(|&i| {
            !lines[i]
                .chars
                .first()
                .is_some_and(|(_, style)| style.background_rgb == Some(background))
        })
        .map_or(0, |i| i + 1);
    for line in &mut lines[start..] {
        if !line
            .chars
            .first()
            .is_some_and(|(_, style)| style.background_rgb == Some(background))
        {
            break;
        }
        for (ch, style) in &mut line.chars {
            if matches!(ch, '⠁' | '⠂' | '⠄' | '⠈' | '⠐' | '⠠' | '⡀' | '⢀')
                && !style.bold
                && !style.faint
                && !style.italic
                && !style.inverse
                && !style.underline
                && !style.blink
                && !style.hidden
                && !style.crossed_out
                && style.background_rgb.is_some()
                && style.foreground_rgb.is_some()
            {
                *ch = ' ';
            }
        }
        line.text = line.chars.iter().map(|(ch, _)| *ch).collect();
    }
}

fn codex_composer_start(lines: &[StyledLine]) -> Option<usize> {
    // The live prefix is in column zero; continuation rows start after LIVE_PREFIX_COLS.
    // Stop at the newest prefix, including disabled and shell composers, rather than a transcript echo.
    let start = lines.iter().rposition(|line| {
        line.chars
            .first()
            .is_some_and(|(ch, _)| matches!(ch, '›' | '»' | '!'))
    })?;
    let (marker, style) = lines[start].chars[0];
    (marker != '!' && style.bold && !style.faint).then_some(start)
}

fn codex_input_region(lines: &[StyledLine]) -> Option<Range<usize>> {
    let start = codex_composer_start(lines)?;
    // Selection rows make both the marker and label bold. The textarea only adds bold with inverse for search matches, which can remain after accepting a Vim search.
    if lines[start].chars[1..]
        .iter()
        .find(|(ch, _)| !ch.is_whitespace())
        .is_some_and(|(_, style)| style.bold && !style.faint && !style.inverse)
    {
        return None;
    }
    let foot = if let Some(background) = lines[start].chars[0].1.background_rgb {
        // The composer owns one bottom padding row, even with no statusline or a multiline footer.
        let end = (start + 1..lines.len())
            .find(|&i| {
                !lines[i]
                    .chars
                    .first()
                    .is_some_and(|(_, style)| style.background_rgb == Some(background))
            })
            .unwrap_or(lines.len());
        end.checked_sub(1).filter(|&foot| foot > start)?
    } else {
        let last = (start + 1..lines.len()).rfind(|&i| !lines[i].text.trim().is_empty())?;
        (start + 1..last).rfind(|&i| lines[i].text.trim().is_empty())?
    };
    if !lines[foot].text.trim().is_empty() {
        return None;
    }
    // History/Vim searches keep the draft visible but route input into a separate footer editor.
    if lines[foot + 1..].iter().any(|line| {
        let text = line.text.trim();
        text.starts_with("reverse-i-search:")
            || line
                .chars
                .iter()
                .find(|(ch, _)| !ch.is_whitespace())
                .is_some_and(|(ch, style)| {
                    matches!(ch, '/' | '?')
                        && !style.bold
                        && !style.faint
                        && style.foreground_index == Some(6)
                })
    }) {
        return None;
    }
    let body = &lines[start].text;
    if body.contains("Viewing sub-agent")
        && lines[start].chars[1..]
            .iter()
            .filter(|(ch, _)| !ch.is_whitespace())
            .all(|(_, style)| style.faint)
    {
        return None;
    }
    Some(start..foot)
}

/// Codex renders remote attachments above the marker, separated from its textarea by a blank row.
/// They must count as a draft or clear verification skips Ctrl+C and attaches old images to the next message.
fn codex_remote_images(lines: &[StyledLine], start: usize) -> Vec<String> {
    if start == 0 || !lines[start - 1].text.trim().is_empty() {
        return Vec::new();
    }
    let mut images: Vec<_> = lines[..start - 1]
        .iter()
        .rev()
        .take_while(|line| {
            let label = line.text.trim();
            label
                .strip_prefix("[Image #")
                .and_then(|label| label.strip_suffix(']'))
                .is_some_and(|number| {
                    !number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit())
                })
                && line
                    .chars
                    .iter()
                    .filter(|(ch, _)| !ch.is_whitespace())
                    .all(|(_, style)| style.foreground_index == Some(6))
        })
        .map(|line| line.text.trim().to_string())
        .collect();
    images.reverse();
    images
}

/// Input only, excluding transcript and footer. Call with a VT capture when proving a draft empty.
pub fn session_chat_composer_input(agent: &str, screen: &str) -> Option<SessionChatComposerInput> {
    if matches!(agent, "claude" | "openclaude") && super::is_claude_code_agents_screen(screen) {
        return None;
    }
    if agent == "grok" {
        return super::grok_composer_draft(screen).map(|text| {
            // CDXC:AgentScreenDetection 2026-09-09 WHY: Grok's empty composer paints "Type a message..." in RGB 78,78,78 rather than SGR faint. Treating it as a draft held model selections forever; checking its VT style protects real input with the same words.
            // CDXC:AgentScreenDetection 2026-09-16 WHY:
            // Grok's unaccepted next-prompt suggestion is italic RGB 88,88,88. Treating it as typed input made verified clearing fail, blocking /compact and queued chat messages even though typing replaces the suggestion.
            let placeholder = styled_lines(screen)
                .iter()
                .rev()
                .find(|line| super::is_boxed_marker_line(&line.text, '❯'))
                .is_some_and(|line| {
                    let Some(marker) = line.chars.iter().position(|(ch, _)| *ch == '❯') else {
                        return false;
                    };
                    let Some(border) = line
                        .chars
                        .iter()
                        .rposition(|(ch, _)| *ch == '│')
                        .filter(|border| *border > marker)
                    else {
                        return false;
                    };
                    let input_chars: Vec<_> = line.chars[marker + 1..border]
                        .iter()
                        .filter(|(ch, _)| !ch.is_whitespace())
                        .collect();
                    !input_chars.is_empty()
                        && input_chars.iter().all(|(_, style)| {
                            (text == "Type a message..."
                                && style.foreground_rgb == Some([78, 78, 78]))
                                || (style.italic && style.foreground_rgb == Some([88, 88, 88]))
                        })
                });
            SessionChatComposerInput {
                text,
                rows: 1,
                shell_mode: false,
                placeholder,
            }
        });
    }
    if agent == "freebuff" {
        let lines: Vec<String> = screen.lines().map(strip_ansi_sgr).collect();
        return freebuff_composer_input(&lines);
    }
    let mut lines = styled_lines(screen);
    if agent == "codex" {
        clear_codex_composer_particles(&mut lines);
    }
    let plain: Vec<_> = lines.iter().map(|line| line.text.clone()).collect();
    if matches!(agent, "pi" | "omp" | "zcode") {
        let (region, omp_band) = if agent == "zcode" {
            (zcode_input_region(&plain)?, false)
        } else if agent == "pi" {
            (unmarked_rule_input_region(&plain)?, false)
        } else if let Some(region) = omp_box_input_region(&plain) {
            (region, false)
        } else {
            (omp_band_input_region(&plain)?, true)
        };
        let text = plain[region.clone()]
            .iter()
            .map(|line| {
                if omp_band {
                    // The band's first row carries the `╰─ ` cue, later rows a three-space indent.
                    let row = match line.strip_prefix(OMP_BAND_CUE) {
                        Some(row) => row.strip_prefix(' ').unwrap_or(row),
                        None => line.strip_prefix("   ").unwrap_or(line),
                    };
                    row.trim_end().to_string()
                } else if agent == "omp" {
                    // OMP merges its final input row into ╰─ text ─╯.
                    let line = line.trim();
                    let inner = line
                        .chars()
                        .skip(1)
                        .take(line.chars().count().saturating_sub(2))
                        .collect::<String>();
                    if line.starts_with('╰') {
                        let inner = inner.strip_prefix('─').unwrap_or(&inner);
                        inner.strip_suffix('─').unwrap_or(inner).to_string()
                    } else {
                        inner
                    }
                } else {
                    line.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        return Some(SessionChatComposerInput {
            text,
            rows: region.len(),
            shell_mode: false,
            placeholder: agent == "omp" && region.len() == 1 && omp_hint_only(&lines[region.start]),
        });
    }
    let region = match agent {
        "claude" | "openclaude" => rule_input_region(&plain, CLAUDE_COMPOSER_MARKERS)?,
        "antigravity" => rule_input_region(&plain, &['>'])?,
        "cursor" => cursor_input_region(&plain)?,
        "hermes-agent" => hermes_input_region(&plain)?,
        // CDXC:AgentScreenDetection 2026-09-11 DECISION:
        // User: keep Codex 0.153 and earlier working alongside 0.154, with or without stars. The bold prompt and dim placeholder identify input independently of optional particles and background colors.
        "codex" => codex_input_region(&lines)?,
        _ => return None,
    };
    let first = &lines[region.start];
    let marker = first.chars.iter().position(|(ch, _)| {
        if agent == "hermes-agent" {
            *ch == '❯'
        } else {
            !ch.is_whitespace()
        }
    })?;
    let shell_mode = matches!(agent, "claude" | "openclaude") && first.chars[marker].0 == '!';
    let body: Vec<_> = first.chars[marker + 1..]
        .iter()
        .chain(
            lines[region.start + 1..region.end]
                .iter()
                .flat_map(|line| line.chars.iter()),
        )
        .filter(|(ch, _)| !ch.is_whitespace())
        .collect();
    // The shell marker is part of the logical draft, including for paste and returned-prompt comparisons.
    let text_start = if shell_mode { marker } else { marker + 1 };
    let text = std::iter::once(
        first.chars[text_start..]
            .iter()
            .map(|(ch, _)| *ch)
            .collect::<String>(),
    )
    .chain(
        plain[region.start + 1..region.end]
            .iter()
            .map(|line| line.trim_end().to_string()),
    )
    .collect::<Vec<_>>()
    .join("\n")
    .trim()
    .to_string();
    // Cursor's caret inverts the first placeholder character while the rest stays faint.
    let placeholder = !body.is_empty()
        && body.iter().enumerate().all(|(index, (_, style))| {
            style.faint
                // Hermes's prompt_toolkit placeholder is italic; editable input inherits the terminal style.
                || (agent == "hermes-agent" && style.italic)
                || (agent == "cursor" && index == 0 && style.inverse && body.len() > 1)
        })
        && !text.to_lowercase().contains("[paste");
    let mut input = SessionChatComposerInput {
        text,
        rows: region.len(),
        shell_mode,
        placeholder,
    };
    if agent == "codex" {
        let mut images = codex_remote_images(&lines, region.start);
        if !images.is_empty() {
            input.rows += images.len() + 1;
            if !input.placeholder && !input.text.is_empty() {
                images.push(input.text);
            }
            input.text = images.join("\n");
            input.placeholder = false;
        }
    }
    Some(input)
}

#[cfg(test)]
mod zcode_tests {
    use super::*;
    use crate::session_chat_composer::detect_session_chat_composer_ready;

    #[test]
    fn zcode_editor_accepts_multiline_drafts_but_not_open_pickers() {
        let screen = "Turn cancelled.\n────────────────────────────────────────\nfirst line\nsecond line\n────────────────────────────────────────\n ◈ zai/glm-5.2 ─ ◉ build ─ ⚡ max ─ ctx 100% left\n";
        let input = session_chat_composer_input("zcode", screen).unwrap();
        assert_eq!(input.text, "first line\nsecond line");
        assert_eq!(input.rows, 2);
        assert!(!detect_session_chat_composer_ready(Some("zcode"), screen)
            .blocks_message_for(Some("zcode")));
        for blocked in [
            String::new(),
            screen.replace("◈ zai", "other"),
            format!("{screen}Choose a model\n"),
        ] {
            assert!(detect_session_chat_composer_ready(Some("zcode"), &blocked)
                .blocks_message_for(Some("zcode")));
            assert!(session_chat_composer_input("zcode", &blocked).is_none());
        }
    }
}
