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

/// Escape HTML, then render `code` spans and **bold** to tags.
fn inline(s: &str) -> String {
    let escaped = html_escape(s);
    // `code` spans: odd-indexed backtick segments become <code>.
    let mut out = String::new();
    for (i, seg) in escaped.split('`').enumerate() {
        if i % 2 == 1 {
            out.push_str("<code>");
            out.push_str(seg);
            out.push_str("</code>");
        } else {
            out.push_str(seg);
        }
    }
    // **bold**: odd-indexed `**` segments become <strong>.
    let mut bolded = String::new();
    for (i, seg) in out.split("**").enumerate() {
        if i % 2 == 1 {
            bolded.push_str("<strong>");
            bolded.push_str(seg);
            bolded.push_str("</strong>");
        } else {
            bolded.push_str(seg);
        }
    }
    bolded
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

const STYLE: &str = r#"
:root {
  --bg: #fbfbfa; --fg: #24292f; --muted: #57606a; --card: #ffffff;
  --border: #d0d7de; --accent: #0969da; --code-bg: #eff1f3; --code-fg: #cf222e;
  --s1: #57606a; --s2: #bf8700; --s3: #cf222e;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #0d1117; --fg: #e6edf3; --muted: #9198a1; --card: #161b22;
    --border: #30363d; --accent: #4493f8; --code-bg: #1f242c; --code-fg: #ff7b72;
    --s1: #9198a1; --s2: #e3b341; --s3: #ff7b72;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0; background: var(--bg); color: var(--fg);
  font: 16px/1.6 -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
}
.wrap { max-width: 820px; margin: 0 auto; padding: 2.5rem 1.25rem 4rem; }
header h1 { margin: 0 0 .25rem; font-size: 1.9rem; letter-spacing: -.02em; }
header .date { color: var(--muted); font-style: italic; }
h2 { font-size: 1.25rem; margin: 2.5rem 0 1rem; padding-bottom: .4rem; border-bottom: 1px solid var(--border); }
code { background: var(--code-bg); color: var(--code-fg); padding: .12em .4em; border-radius: 5px; font-size: .88em;
  font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
.summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: .75rem; list-style: none; padding: 0; margin: 0; }
.summary li { background: var(--card); border: 1px solid var(--border); border-radius: 10px; padding: .8rem 1rem; }
.summary .k { display: block; color: var(--muted); font-size: .8rem; text-transform: uppercase; letter-spacing: .04em; }
.summary .v { font-size: 1.5rem; font-weight: 650; }
.files { color: var(--muted); font-size: .9rem; margin: 1rem 0 0; }
.files code { font-size: .82em; }
.tip { background: var(--card); border: 1px solid var(--border); border-left-width: 4px; border-radius: 10px; padding: 1rem 1.25rem; margin: 1rem 0; }
.tip.s1 { border-left-color: var(--s1); }
.tip.s2 { border-left-color: var(--s2); }
.tip.s3 { border-left-color: var(--s3); }
.tip h3 { margin: 0 0 .5rem; font-size: 1.1rem; display: flex; align-items: center; gap: .6rem; }
.tip p { margin: .5rem 0; }
.tip ul { margin: .5rem 0; padding-left: 1.3rem; }
.tip li { margin: .25rem 0; }
.badge { font-size: .7rem; font-weight: 700; letter-spacing: .06em; text-transform: uppercase; padding: .18em .55em; border-radius: 999px; color: #fff; white-space: nowrap; }
.badge.s1 { background: var(--s1); }
.badge.s2 { background: var(--s2); }
.badge.s3 { background: var(--s3); }
table { border-collapse: collapse; width: 100%; font-size: .95rem; }
table th, table td { text-align: left; padding: .5rem .75rem; border-bottom: 1px solid var(--border); }
table th { color: var(--muted); font-weight: 600; }
.clean { background: var(--card); border: 1px solid var(--border); border-radius: 10px; padding: 1.25rem; }
footer { margin-top: 3rem; color: var(--muted); font-size: .85rem; border-top: 1px solid var(--border); padding-top: 1rem; }
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
    b.push_str("<header>\n<h1>mashless session report</h1>\n");
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
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>mashless session report</title>\n<style>{}</style>\n</head>\n<body>\n{}</body>\n</html>\n",
        STYLE, b
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
