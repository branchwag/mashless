//! Renders an [`Analysis`] into a self-contained HTML page and writes it to
//! disk. Port of `mashless.report`, retargeted from Markdown to HTML so the
//! report can open straight in the user's default browser.

use std::fs;
use std::io;
use std::path::PathBuf;

use chrono::Local;

use crate::analyzer::Analysis;
use crate::recorder::Session;

fn plural(n: i64, word: &str) -> String {
    format!("{} {}{}", n, word, if n == 1 { "" } else { "s" })
}

fn fmt_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{}h {}m {}s", h, m, s)
    } else if m > 0 {
        format!("{}m {}s", m, s)
    } else {
        format!("{}s", s)
    }
}

/// Longest run and total presses across a list of counted runs.
fn summarize(counts: &[i64]) -> (i64, i64) {
    let longest = counts.iter().copied().max().unwrap_or(0);
    let total: i64 = counts.iter().sum();
    (longest, total)
}

struct Tip {
    heading: String,
    severity: u8,
    lines: Vec<String>,
}

/// Build the ordered list of tips. Mirrors the Lua `build_tips`.
fn build_tips(a: &Analysis) -> Vec<Tip> {
    let mut tips: Vec<Tip> = Vec::new();

    // Vertical movement -------------------------------------------------------
    if !a.vertical.is_empty() {
        let counts: Vec<i64> = a.vertical.iter().map(|r| r.count).collect();
        let (longest, total) = summarize(&counts);
        let longest_run = a.vertical.iter().find(|r| r.count == longest);
        let mut lines = vec![format!(
            "You walked vertically with `j`/`k` in long unbroken runs {} (longest: **{} presses in a row**).",
            plural(a.vertical.len() as i64, "time"),
            longest
        )];
        if let Some(lr) = longest_run {
            if lr.dist > 0 {
                let dir = if lr.tok == "k" || lr.tok == "<Up>" { "k" } else { "j" };
                lines.push(format!(
                    "That longest run moved you **{} lines** — `{}{}` does it in 2-3 keystrokes.",
                    lr.dist, lr.dist, dir
                ));
            }
        }
        lines.extend([
            "Faster ways to travel vertically:".to_string(),
            "  - `{count}j` / `{count}k` — turn on `:set relativenumber` and the count is shown for you.".to_string(),
            "  - `}` / `{` — jump by paragraph / blank-line block.".to_string(),
            "  - `<C-d>` / `<C-u>` — scroll half a page and keep the cursor centred.".to_string(),
            "  - `gg` / `G` / `{line}G` — top, bottom, or an absolute line number.".to_string(),
            "  - `<C-o>` / `<C-i>` — jump back/forward through your jump history.".to_string(),
        ]);
        tips.push(Tip {
            heading: "Vertical movement".into(),
            severity: if total >= 30 { 3 } else { 2 },
            lines,
        });
    }

    // Horizontal movement -----------------------------------------------------
    if !a.horizontal.is_empty() {
        let counts: Vec<i64> = a.horizontal.iter().map(|r| r.count).collect();
        let (longest, total) = summarize(&counts);
        let example = a.horizontal.iter().find(|r| r.ch.is_some());
        let mut lines = vec![format!(
            "You inched sideways with `h`/`l` in long runs {} (longest: **{} in a row**).",
            plural(a.horizontal.len() as i64, "time"),
            longest
        )];
        if let Some(ex) = example {
            if let Some(ch) = ex.ch {
                lines.push(format!(
                    "One run ended on the character `{}` — `f{}` would have jumped straight there.",
                    ch, ch
                ));
            }
        }
        lines.extend([
            "Faster ways to travel within a line:".to_string(),
            "  - `w` / `b` / `e` — move word by word.".to_string(),
            "  - `f{char}` / `t{char}` — jump onto / just before the next occurrence of a char (`;` / `,` to repeat).".to_string(),
            "  - `0` / `^` / `$` — start of line / first non-blank / end of line.".to_string(),
            "  - `%` — jump to the matching bracket.".to_string(),
        ]);
        tips.push(Tip {
            heading: "Horizontal movement".into(),
            severity: if total >= 30 { 3 } else { 2 },
            lines,
        });
    }

    // Arrow keys --------------------------------------------------------------
    if a.normal_arrows > 0 {
        tips.push(Tip {
            heading: "Arrow keys in normal mode".into(),
            severity: 2,
            lines: vec![
                format!("You used the arrow keys {} in normal mode.", plural(a.normal_arrows, "time")),
                "Stay on the home row: `h` `j` `k` `l` do the same thing without the reach.".into(),
                "If it helps the habit stick, you can even unmap the arrows in normal mode.".into(),
            ],
        });
    }

    if a.insert_arrows > 0 {
        tips.push(Tip {
            heading: "Arrow keys in insert mode".into(),
            severity: 1,
            lines: vec![
                format!("You used the arrow keys {} while in insert mode.", plural(a.insert_arrows, "time")),
                "Repositioning is usually cleaner from normal mode: `<Esc>`, move, then re-enter.".into(),
                "For a single quick hop without leaving insert mode, use `<C-o>{motion}`.".into(),
            ],
        });
    }

    // Character deletion ------------------------------------------------------
    if !a.x_runs.is_empty() {
        let counts: Vec<i64> = a.x_runs.iter().map(|r| r.count).collect();
        let (longest, _) = summarize(&counts);
        tips.push(Tip {
            heading: "Deleting character by character".into(),
            severity: 2,
            lines: vec![
                format!(
                    "You pressed `x` repeatedly {} (longest: **{} in a row**).",
                    plural(a.x_runs.len() as i64, "time"),
                    longest
                ),
                "Delete in bigger bites:".into(),
                "  - `{count}x` — delete several characters at once.".into(),
                "  - `dw` / `de` — delete to the end of a word.".into(),
                "  - `d$` (or `D`) — delete to the end of the line.".into(),
                "  - `diw` / `daw` — delete the inner word / a word plus its whitespace.".into(),
            ],
        });
    }

    if !a.dd_runs.is_empty() {
        let counts: Vec<i64> = a.dd_runs.iter().map(|r| r.count).collect();
        let (longest, _) = summarize(&counts);
        tips.push(Tip {
            heading: "Deleting lines one at a time".into(),
            severity: 1,
            lines: vec![
                format!(
                    "You ran `dd` in repeated bursts {} (longest: **{} lines in a row**).",
                    plural(a.dd_runs.len() as i64, "time"),
                    longest
                ),
                "`{count}dd` deletes several lines at once, and `dap` / `dip` delete a whole paragraph.".into(),
                "Or select with `V`, extend with `j`, then `d`.".into(),
            ],
        });
    }

    // Word-motion spam --------------------------------------------------------
    if !a.word_runs.is_empty() {
        let counts: Vec<i64> = a.word_runs.iter().map(|r| r.count).collect();
        let (longest, _) = summarize(&counts);
        tips.push(Tip {
            heading: "Long word-motion chains".into(),
            severity: 1,
            lines: vec![
                format!(
                    "You chained `w`/`b`/`e` in long runs {} (longest: **{} in a row**).",
                    plural(a.word_runs.len() as i64, "time"),
                    longest
                ),
                "For a known target, `f{char}` or a `/search` jumps there directly instead of stepping word by word.".into(),
            ],
        });
    }

    if !a.undo_runs.is_empty() {
        let counts: Vec<i64> = a.undo_runs.iter().map(|r| r.count).collect();
        let (longest, _) = summarize(&counts);
        tips.push(Tip {
            heading: "Long undo streaks".into(),
            severity: 1,
            lines: vec![
                format!(
                    "You tapped `u` in long streaks {} (longest: **{} in a row**).",
                    plural(a.undo_runs.len() as i64, "time"),
                    longest
                ),
                "`{count}u` undoes several steps at once, and `:earlier 1m` / `g-` travel the undo tree by time.".into(),
            ],
        });
    }

    tips
}

const CHEATSHEET: &[(&str, &str)] = &[
    ("Jump to line N", "{N}G` or `:{N}<CR>"),
    ("Down/up N lines", "{N}j` / `{N}k"),
    ("Next/prev word", "w` / `b` (`e` = end of word)"),
    ("To char X on line", "f{X}` / `t{X}`, repeat with `;` / `,"),
    ("Line ends", "0` start, `^` first non-blank, `$` end"),
    ("Paragraph jump", "}` / `{"),
    ("Half-page scroll", "<C-d>` / `<C-u>"),
    ("Search", "/text<CR>`, `n` / `N`, `*` for word under cursor"),
    ("Matching bracket", "%"),
    ("Delete word / line", "diw`, `daw`, `dd`, `dap"),
    ("Jump history", "<C-o>` back, `<C-i>` forward"),
];

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Plain-English name for a bare Vim key mnemonic (the text inside `< >`, or a
/// chord's trailing key). Returns `None` for anything unrecognised.
fn key_name(s: &str) -> Option<&'static str> {
    Some(match s.to_ascii_lowercase().as_str() {
        "cr" | "return" | "enter" => "Enter",
        "esc" => "Escape",
        "tab" => "Tab",
        "bs" => "Backspace",
        "del" => "Delete",
        "space" => "Space",
        "up" => "Up arrow",
        "down" => "Down arrow",
        "left" => "Left arrow",
        "right" => "Right arrow",
        "home" => "Home",
        "end" => "End",
        "pageup" => "Page Up",
        "pagedown" => "Page Down",
        "leader" => "leader",
        _ => return None,
    })
}

/// Translate a single `<...>` key token into a readable name, expanding
/// modifier chords (`<C-o>` → `Ctrl-o`, `<C-S-Right>` → `Ctrl-Shift-Right
/// arrow`). Returns `None` for tokens we don't recognise, so they're left
/// verbatim.
fn expand_key(token: &str) -> Option<String> {
    let inner = token.strip_prefix('<')?.strip_suffix('>')?;
    if inner.is_empty() {
        return None;
    }
    if let Some(n) = key_name(inner) {
        return Some(n.to_string());
    }
    // Peel off stacked modifier prefixes: C- S- M- A- D-.
    const MODS: &[(&str, &str)] = &[
        ("c-", "Ctrl"),
        ("s-", "Shift"),
        ("m-", "Alt"),
        ("a-", "Alt"),
        ("d-", "Cmd"),
    ];
    let mut parts: Vec<String> = Vec::new();
    let mut cur = inner;
    while let Some(&(_, name)) = MODS
        .iter()
        .find(|(p, _)| cur.len() > 2 && cur[..2].eq_ignore_ascii_case(p))
    {
        parts.push(name.to_string());
        cur = &cur[2..];
    }
    if parts.is_empty() {
        return None;
    }
    let key = key_name(cur).map(str::to_string).unwrap_or_else(|| cur.to_string());
    parts.push(key);
    Some(parts.join("-"))
}

/// Read a code span for Vim key notation and return a plain-English gloss, or
/// `None` if it contains none. Angle-bracket tokens are expanded in place and
/// the rest of the span is left as written, e.g. `:{N}<CR>` → `:{N} Enter`.
fn decipher(span: &str) -> Option<String> {
    if !span.contains('<') {
        return None;
    }
    let mut out = String::new();
    let mut found = false;
    let mut rest = span;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt..];
        match after.find('>') {
            Some(gt) => {
                let token = &after[..=gt];
                match expand_key(token) {
                    Some(name) => {
                        out.push(' ');
                        out.push_str(&name);
                        out.push(' ');
                        found = true;
                    }
                    None => out.push_str(token),
                }
                rest = &after[gt + 1..];
            }
            None => {
                out.push_str(after);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    if found {
        // Collapse the padding we injected around each expansion.
        Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
    } else {
        None
    }
}

/// Escape HTML, then render `code` spans and **bold** to tags. Vim key
/// notation inside a code span is replaced with its plain-English reading.
fn inline(s: &str) -> String {
    let mut out = String::new();
    // `code` spans: odd-indexed backtick segments become <code>.
    for (i, seg) in s.split('`').enumerate() {
        if i % 2 == 1 {
            let shown = decipher(seg).unwrap_or_else(|| seg.to_string());
            out.push_str(&format!("<code>{}</code>", html_escape(&shown)));
        } else {
            // **bold**: odd-indexed `**` segments become <strong>.
            let escaped = html_escape(seg);
            for (j, part) in escaped.split("**").enumerate() {
                if j % 2 == 1 {
                    out.push_str("<strong>");
                    out.push_str(part);
                    out.push_str("</strong>");
                } else {
                    out.push_str(part);
                }
            }
        }
    }
    out
}

/// Render a tip's flat line list into HTML, grouping `  - ` bullets into lists.
fn render_lines(lines: &[String], out: &mut String) {
    let mut in_list = false;
    for line in lines {
        if let Some(item) = line.strip_prefix("  - ") {
            if !in_list {
                out.push_str("<ul>\n");
                in_list = true;
            }
            out.push_str("<li>");
            out.push_str(&inline(item));
            out.push_str("</li>\n");
        } else {
            if in_list {
                out.push_str("</ul>\n");
                in_list = false;
            }
            out.push_str("<p>");
            out.push_str(&inline(line));
            out.push_str("</p>\n");
        }
    }
    if in_list {
        out.push_str("</ul>\n");
    }
}

/// Share Tech Mono (latin subset, weight 400), embedded as a base64 `woff2`
/// data URI so the report renders in the intended font fully offline — no
/// Google Fonts round-trip. The asset is the same file Google Fonts serves.
const FONT_FACE: &str = concat!(
    "@font-face{font-family:'Share Tech Mono';font-style:normal;font-weight:400;",
    "font-display:swap;src:url(data:font/woff2;base64,",
    include_str!("share_tech_mono.woff2.b64"),
    ") format('woff2');}\n"
);

const STYLE: &str = r#"
:root {
  --bg: #0a0c12; --surface: #0d1119; --surface-hover: #121a28;
  --border: rgba(79,195,247,0.18); --border-strong: rgba(79,195,247,0.4);
  --fg: #d8f0ff; --muted: #8ba9bd;
  --accent: #4fc3f7; --accent-hover: #7fd4fa;
  --code-bg: rgba(79,195,247,0.08); --code-fg: #7fd4fa;
  --grid: rgba(79,195,247,0.04);
  --s1: #50c878; --s2: #fab432; --s3: #ff5470;
  --mono: "Share Tech Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
* { box-sizing: border-box; }
body {
  margin: 0; background: var(--bg); color: var(--fg);
  font: 16px/1.65 var(--mono);
  -webkit-font-smoothing: antialiased;
}
/* Faint cyber grid overlay, fixed behind all content */
body::before {
  content: ""; position: fixed; inset: 0; z-index: 0; pointer-events: none;
  background-image:
    linear-gradient(var(--grid) 1px, transparent 1px),
    linear-gradient(90deg, var(--grid) 1px, transparent 1px);
  background-size: 40px 40px;
}
.wrap { position: relative; z-index: 1; max-width: 820px; margin: 0 auto; padding: 3rem 1.25rem 5rem; }

/* Uppercase, letter-spaced "eyebrow" label with a leading accent tick */
.eyebrow {
  display: inline-flex; align-items: center; gap: .55rem;
  font-size: .7rem; line-height: 1; letter-spacing: .28em; text-transform: uppercase;
  color: var(--accent); margin-bottom: 1rem;
}
.eyebrow::before { content: ""; width: 1.4rem; height: 1px; background: currentColor; opacity: .6; }

header h1 {
  margin: 0 0 .4rem; font-size: 2rem; font-weight: 400; letter-spacing: .01em;
  text-shadow: 0 0 18px rgba(79,195,247,.35);
}
header .date { color: var(--muted); font-size: .85rem; text-transform: uppercase; letter-spacing: .1em; }

h2 {
  font-size: 1.15rem; font-weight: 400; text-transform: uppercase; letter-spacing: .16em;
  margin: 3rem 0 1.25rem; padding-left: .9rem; position: relative;
}
h2::before {
  content: ""; position: absolute; left: 0; top: .1em; bottom: .1em; width: 3px;
  background: var(--accent); box-shadow: 0 0 12px rgba(79,195,247,.6);
}

code {
  background: var(--code-bg); color: var(--code-fg);
  padding: .12em .45em; border: 1px solid var(--border); font-size: .88em; font-family: var(--mono);
}
/* Share Tech Mono has no bold weight; emphasise with accent + glow instead */
strong { color: var(--accent); font-weight: 400; text-shadow: 0 0 12px rgba(79,195,247,.3); }

.summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: .75rem; list-style: none; padding: 0; margin: 0; }
.summary li {
  background: var(--surface); border: 1px solid var(--border); padding: .9rem 1rem;
  transition: border-color .2s ease, box-shadow .2s ease;
}
.summary li:hover { border-color: var(--border-strong); box-shadow: 0 0 26px -8px rgba(79,195,247,.5); }
.summary .k { display: block; color: var(--muted); font-size: .7rem; text-transform: uppercase; letter-spacing: .18em; margin-bottom: .3rem; }
.summary .v { font-size: 1.5rem; color: var(--accent); text-shadow: 0 0 14px rgba(79,195,247,.3); }
.files { color: var(--muted); font-size: .9rem; margin: 1.25rem 0 0; }
.files code { font-size: .82em; }

.tip {
  background: var(--surface); border: 1px solid var(--border); border-left: 3px solid var(--s2);
  padding: 1.1rem 1.35rem; margin: 1rem 0;
  transition: border-color .2s ease, box-shadow .2s ease;
}
.tip:hover { border-color: var(--border-strong); box-shadow: 0 0 26px -10px rgba(79,195,247,.4); }
.tip.s1 { border-left-color: var(--s1); }
.tip.s2 { border-left-color: var(--s2); }
.tip.s3 { border-left-color: var(--s3); }
.tip h3 { margin: 0 0 .6rem; font-size: 1.05rem; font-weight: 400; display: flex; align-items: center; gap: .7rem; flex-wrap: wrap; }
.tip p { margin: .5rem 0; }
.tip ul { margin: .5rem 0; padding-left: 1.3rem; }
.tip li { margin: .3rem 0; }
.tip li::marker { color: var(--accent); }

.badge {
  font-size: .62rem; letter-spacing: .16em; text-transform: uppercase;
  padding: .25em .6em; border: 1px solid currentColor; white-space: nowrap;
}
.badge.s1 { color: var(--s1); }
.badge.s2 { color: var(--s2); }
.badge.s3 { color: var(--s3); }

table { border-collapse: collapse; width: 100%; font-size: .95rem; }
table th, table td { text-align: left; padding: .55rem .8rem; border-bottom: 1px solid var(--border); }
table th { color: var(--accent); font-weight: 400; text-transform: uppercase; letter-spacing: .12em; font-size: .75rem; }
table tr:hover td { background: var(--surface-hover); }

.clean { background: var(--surface); border: 1px solid var(--border); padding: 1.5rem; }
footer { margin-top: 3.5rem; color: var(--muted); font-size: .8rem; border-top: 1px solid var(--border); padding-top: 1.25rem; text-transform: uppercase; letter-spacing: .1em; }
"#;

const SEVERITY_LABEL: [&str; 3] = ["minor", "worth fixing", "time sink"];

/// Render an analysis + session into a full HTML document.
pub fn render(a: &Analysis, session: &Session) -> String {
    let duration = (Local::now() - session.start).num_seconds();
    let files: Vec<&String> = session.files.iter().collect();

    let mut tips = build_tips(a);
    // Most severe first (stable, to preserve build order within a severity).
    tips.sort_by(|x, y| y.severity.cmp(&x.severity));

    let mut b = String::new();
    b.push_str("<div class=\"wrap\">\n");
    b.push_str("<header>\n<span class=\"eyebrow\">// motion analysis</span>\n<h1>mashless session report</h1>\n");
    b.push_str(&format!(
        "<div class=\"date\">{}</div>\n</header>\n",
        html_escape(&Local::now().format("%A %d %B %Y, %H:%M").to_string())
    ));

    // Summary -----------------------------------------------------------------
    b.push_str("<h2>Summary</h2>\n<ul class=\"summary\">\n");
    let summary_items = [
        ("Session length".to_string(), fmt_duration(duration)),
        ("Keystrokes".to_string(), a.total_keys.to_string()),
        ("Wasted (est.)".to_string(), format!("~{}", a.wasted)),
        ("Efficiency".to_string(), format!("{} / 100", a.efficiency)),
        ("Tips".to_string(), tips.len().to_string()),
    ];
    for (k, v) in summary_items {
        b.push_str(&format!(
            "<li><span class=\"k\">{}</span><span class=\"v\">{}</span></li>\n",
            html_escape(&k),
            html_escape(&v)
        ));
    }
    b.push_str("</ul>\n");
    if !files.is_empty() {
        b.push_str(&format!(
            "<p class=\"files\"><strong>{}</strong> touched:",
            plural(files.len() as i64, "file")
        ));
        for f in &files {
            b.push_str(&format!(" <code>{}</code>", html_escape(f)));
        }
        b.push_str("</p>\n");
    }

    // Tips --------------------------------------------------------------------
    if tips.is_empty() {
        b.push_str("<h2>No inefficiencies spotted</h2>\n<div class=\"clean\">\n");
        if a.total_keys < 20 {
            b.push_str("<p>Barely any motions were recorded this session — not enough to judge.</p>\n");
        } else {
            b.push_str("<p>Clean session — your motions looked efficient. Keep it up.</p>\n");
        }
        b.push_str("</div>\n");
    } else {
        b.push_str("<h2>What you can improve</h2>\n");
        for (i, tip) in tips.iter().enumerate() {
            let sev = tip.severity.clamp(1, 3) as usize;
            b.push_str(&format!("<div class=\"tip s{}\">\n", sev));
            b.push_str(&format!(
                "<h3>{}. {} <span class=\"badge s{}\">{}</span></h3>\n",
                i + 1,
                inline(&tip.heading),
                sev,
                SEVERITY_LABEL[sev - 1]
            ));
            render_lines(&tip.lines, &mut b);
            b.push_str("</div>\n");
        }
    }

    // Cheat-sheet -------------------------------------------------------------
    b.push_str("<h2>Motion cheat-sheet</h2>\n<table>\n<thead><tr><th>Goal</th><th>Keys</th></tr></thead>\n<tbody>\n");
    for (goal, keys) in CHEATSHEET {
        b.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>\n",
            html_escape(goal),
            inline(&format!("`{}`", keys))
        ));
    }
    b.push_str("</tbody>\n</table>\n");

    b.push_str("<footer>Generated by mashless — badges: minor, worth fixing, time sink.</footer>\n");
    b.push_str("</div>\n");

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>mashless session report</title>\n<style>{}{}</style>\n</head>\n<body>\n{}</body>\n</html>\n",
        FONT_FACE, STYLE, b
    )
}

/// Analyze + render + write a report file. Returns the path written.
pub fn write(output_dir: &str, a: &Analysis, session: &Session) -> io::Result<PathBuf> {
    fs::create_dir_all(output_dir)?;
    let name = Local::now().format("mashless-%Y-%m-%d-%H%M%S.html").to_string();
    let mut path = PathBuf::from(output_dir);
    path.push(name);
    let html = render(a, session);
    fs::write(&path, html)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decipher_expands_notation() {
        assert_eq!(decipher("<CR>").as_deref(), Some("Enter"));
        assert_eq!(decipher("<C-o>").as_deref(), Some("Ctrl-o"));
        assert_eq!(decipher("<C-i>").as_deref(), Some("Ctrl-i"));
        assert_eq!(decipher("<Esc>").as_deref(), Some("Escape"));
        assert_eq!(decipher(":{N}<CR>").as_deref(), Some(":{N} Enter"));
        assert_eq!(decipher("/text<CR>").as_deref(), Some("/text Enter"));
        assert_eq!(decipher("<C-o>{motion}").as_deref(), Some("Ctrl-o {motion}"));
        assert_eq!(decipher("<S-Tab>").as_deref(), Some("Shift-Tab"));
    }

    #[test]
    fn decipher_ignores_plain_spans() {
        assert_eq!(decipher("{N}G"), None);
        assert_eq!(decipher("diw"), None);
        // Unknown angle token is left alone (no gloss).
        assert_eq!(decipher("<Nope>"), None);
    }

    #[test]
    fn inline_replaces_notation_with_description() {
        let html = inline("Use `<C-o>` to go back");
        assert!(html.contains("<code>Ctrl-o</code>"), "{html}");
        // A plain code span stays untouched.
        assert!(inline("Press `dd`").contains("<code>dd</code>"));
    }

    #[test]
    fn inline_handles_cheatsheet_entry() {
        // The exact "Jump to line N" keys column from CHEATSHEET.
        let html = inline("`{N}G` or `:{N}<CR>`");
        assert!(html.contains("<code>{N}G</code>"), "{html}");
        assert!(html.contains("<code>:{N} Enter</code>"), "{html}");
    }
}

/// Path of the newest report in `output_dir`, or `None`.
pub fn latest(output_dir: &str) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(output_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("mashless-") && n.ends_with(".html"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    entries.pop()
}
