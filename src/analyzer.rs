//! Turns a recorded session into structured findings about inefficient
//! motions. Direct port of `mashless.analyzer`.

use crate::recorder::{Config, KeyEntry, Session};

fn vertical(tok: &str) -> bool {
    matches!(tok, "j" | "k" | "<Down>" | "<Up>")
}
fn horizontal(tok: &str) -> bool {
    matches!(tok, "l" | "h" | "<Right>" | "<Left>")
}
fn wordwise(tok: &str) -> bool {
    matches!(tok, "w" | "b" | "e" | "W" | "B" | "E")
}

pub struct VRun {
    pub tok: String,
    pub count: i64,
    pub dist: i64,
}

pub struct HRun {
    pub count: i64,
    pub ch: Option<char>,
}

pub struct Countable {
    pub count: i64,
}

pub struct WordRun {
    pub count: i64,
}

/// Structured findings. The report renders this directly.
#[derive(Default)]
pub struct Analysis {
    pub vertical: Vec<VRun>,
    pub horizontal: Vec<HRun>,
    pub x_runs: Vec<Countable>,
    pub dd_runs: Vec<Countable>,
    pub word_runs: Vec<WordRun>,
    pub undo_runs: Vec<Countable>,
    pub normal_arrows: i64,
    pub insert_arrows: i64,
    pub total_keys: i64,
    pub wasted: i64,
    pub efficiency: i64,
}

/// A maximal run of consecutive identical tokens. `s`/`e` are inclusive
/// indices into the keylog.
struct Run {
    tok: String,
    count: i64,
    s: usize,
    e: usize,
}

/// Group the key stream into runs of consecutive identical tokens.
fn find_runs(keylog: &[KeyEntry]) -> Vec<Run> {
    let mut runs = Vec::new();
    let n = keylog.len();
    let mut i = 0;
    while i < n {
        let tok = &keylog[i].tok;
        let mut j = i;
        while j + 1 < n && keylog[j + 1].tok == *tok {
            j += 1;
        }
        runs.push(Run {
            tok: tok.clone(),
            count: (j - i + 1) as i64,
            s: i,
            e: j,
        });
        i = j + 1;
    }
    runs
}

pub fn analyze(session: &Session, cfg: &Config) -> Analysis {
    let kl = &session.keylog;
    let runs = find_runs(kl);

    let mut a = Analysis {
        insert_arrows: session.insert_arrows as i64,
        total_keys: session.total_keys as i64,
        ..Default::default()
    };

    // Cursor position immediately after a run finishes.
    let end_pos = |run: &Run| -> (i64, i64) {
        match kl.get(run.e + 1) {
            Some(nxt) => (nxt.line, nxt.col),
            None => session.last_pos,
        }
    };

    for r in &runs {
        let tok = r.tok.as_str();

        if vertical(tok) {
            let is_arrow = tok == "<Up>" || tok == "<Down>";
            if is_arrow {
                a.normal_arrows += r.count;
            }
            if r.count >= cfg.vmin {
                let (el, _) = end_pos(r);
                let dist = (el - kl[r.s].line).abs();
                a.vertical.push(VRun {
                    tok: tok.to_string(),
                    count: r.count,
                    dist,
                });
                // An optimal jump costs ~3 keys ({count}{motion}); rest is waste.
                a.wasted += (r.count - 3).max(0);
            }
        } else if horizontal(tok) {
            let is_arrow = tok == "<Left>" || tok == "<Right>";
            if is_arrow {
                a.normal_arrows += r.count;
            }
            if r.count >= cfg.hmin {
                let (_, ec) = end_pos(r);
                // Character the run landed on, if it's a useful f/t target.
                let ch = kl[r.e].text.as_ref().and_then(|t| {
                    if ec < 0 {
                        return None;
                    }
                    t.get(ec as usize..).and_then(|s| s.chars().next())
                });
                let ch = ch.filter(|&c| c != ' ');
                a.horizontal.push(HRun {
                    count: r.count,
                    ch,
                });
                a.wasted += (r.count - 2).max(0);
            }
        } else if tok == "x" {
            if r.count >= cfg.xmin {
                a.x_runs.push(Countable { count: r.count });
                a.wasted += (r.count - 2).max(0);
            }
        } else if tok == "d" {
            // A literal `dd` is two `d` tokens; `dw`/`dj` leave a lone `d`.
            let dds = r.count / 2;
            if dds >= 2 {
                a.dd_runs.push(Countable { count: dds });
                a.wasted += (dds - 1).max(0);
            }
        } else if wordwise(tok) {
            if r.count >= 5 {
                a.word_runs.push(WordRun { count: r.count });
                a.wasted += (r.count - 3).max(0);
            }
        } else if tok == "u" && r.count >= 4 {
            a.undo_runs.push(Countable { count: r.count });
        }
    }

    let denom = a.total_keys.max(1);
    let eff = ((1.0 - a.wasted as f64 / denom as f64) * 100.0 + 0.5).floor() as i64;
    a.efficiency = eff.clamp(0, 100);

    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorder::{Config, KeyEntry, Session};

    fn sess(toks: &[&str]) -> Session {
        let mut s = Session::new();
        for t in toks {
            s.total_keys += 1;
            s.keylog.push(KeyEntry { tok: t.to_string(), line: 1, col: 0, text: None });
        }
        s
    }

    #[test]
    fn x_run_flagged() {
        let s = sess(&["x", "x", "x", "x", "x"]);
        let a = analyze(&s, &Config::default());
        assert_eq!(a.x_runs.len(), 1, "expected one x run");
        assert_eq!(a.x_runs[0].count, 5);
    }

    #[test]
    fn mixed_runs() {
        let toks = ["j","j","j","j","l","l","l","l","l","x","x","x","w","w","w","w","w"];
        let s = sess(&toks);
        let a = analyze(&s, &Config::default());
        assert_eq!(a.vertical.len(), 1);
        assert_eq!(a.horizontal.len(), 1);
        assert_eq!(a.x_runs.len(), 1);
        assert_eq!(a.word_runs.len(), 1);
    }
}
