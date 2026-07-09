//! mashless — Rust core.
//!
//! Spawned by the Lua shim via `jobstart(..., { rpc = true })`, it speaks
//! msgpack-RPC over stdin/stdout. The shim forwards keystrokes, cursor moves
//! and buffer visits; this process owns the session, runs the analyzer, writes
//! the HTML report and opens it in the user's default browser.
//!
//! Protocol
//! --------
//! Notifications (fire-and-forget):
//!   setup(output_dir, min_keys, vmin, hmin, xmin)  — (re)start a session
//!   key(tok, mode, line, col, text)                — one keystroke
//!   cursor(line, col)                              — cursor moved
//!   buf(path)                                      — a file was visited
//!
//! Requests (reply expected):
//!   report(reason)  -> path | ""   — analyze, write, open in browser
//!                                     reason = "exit" (honours min_keys)
//!                                     or "manual" (always writes)
//!   open_latest()   -> path | ""   — open the newest report in the browser

mod analyzer;
mod recorder;
mod report;

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::{create::tokio as create, Handler, Neovim, Value};
use tokio::fs::File;
use tokio::sync::Mutex;

use recorder::{Config, Session};

type Writer = Compat<File>;

struct AppState {
    config: Config,
    session: Session,
}

impl AppState {
    fn new() -> Self {
        Self {
            config: Config::default(),
            session: Session::new(),
        }
    }
}

#[derive(Clone)]
struct NeovimHandler {
    state: Arc<Mutex<AppState>>,
}

fn as_str(v: Option<&Value>) -> String {
    v.and_then(|v| v.as_str()).unwrap_or("").to_string()
}

fn as_i64(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64())
        .or_else(|| v.and_then(|v| v.as_u64()).map(|u| u as i64))
        .unwrap_or(0)
}

/// Open a local file in the user's default browser. Best-effort: any failure
/// (no display, no browser) is swallowed so it never disrupts Neovim's exit.
fn open_in_browser(path: &Path) {
    let url = format!("file://{}", path.to_string_lossy().replace(' ', "%20"));
    let _ = webbrowser::open(&url);
}

#[async_trait]
impl Handler for NeovimHandler {
    type Writer = Writer;

    async fn handle_notify(&self, name: String, args: Vec<Value>, _nvim: Neovim<Writer>) {
        match name.as_str() {
            "setup" => {
                let mut st = self.state.lock().await;
                st.config = Config {
                    output_dir: as_str(args.first()),
                    min_keys: as_i64(args.get(1)),
                    vmin: as_i64(args.get(2)),
                    hmin: as_i64(args.get(3)),
                    xmin: as_i64(args.get(4)),
                };
                // A setup notify marks the true start of a session.
                st.session = Session::new();
            }
            "key" => {
                let tok = as_str(args.first());
                if tok.is_empty() {
                    return;
                }
                let mode = as_str(args.get(1));
                let line = as_i64(args.get(2));
                let col = as_i64(args.get(3));
                let text = args.get(4).and_then(|v| v.as_str()).map(|s| s.to_string());
                let mut st = self.state.lock().await;
                st.session.record_key(tok, &mode, line, col, text);
            }
            "cursor" => {
                let line = as_i64(args.first());
                let col = as_i64(args.get(1));
                let mut st = self.state.lock().await;
                st.session.last_pos = (line, col);
            }
            "buf" => {
                let f = as_str(args.first());
                if !f.is_empty() {
                    let mut st = self.state.lock().await;
                    st.session.files.insert(f);
                }
            }
            _ => {}
        }
    }

    async fn handle_request(
        &self,
        name: String,
        args: Vec<Value>,
        _nvim: Neovim<Writer>,
    ) -> Result<Value, Value> {
        match name.as_str() {
            "report" => {
                let reason = as_str(args.first());
                let st = self.state.lock().await;
                if reason == "exit" && st.session.total_keys < st.config.min_keys as u64 {
                    return Ok(Value::from(""));
                }
                let analysis = analyzer::analyze(&st.session, &st.config);
                match report::write(&st.config.output_dir, &analysis, &st.session) {
                    Ok(path) => {
                        open_in_browser(&path);
                        Ok(Value::from(path.to_string_lossy().into_owned()))
                    }
                    Err(e) => Err(Value::from(format!("mashless: failed to write report: {e}"))),
                }
            }
            "open_latest" => {
                let st = self.state.lock().await;
                match report::latest(&st.config.output_dir) {
                    Some(path) => {
                        open_in_browser(&path);
                        Ok(Value::from(path.to_string_lossy().into_owned()))
                    }
                    None => Ok(Value::from("")),
                }
            }
            _ => Err(Value::from(format!("mashless: unknown request '{name}'"))),
        }
    }
}

#[tokio::main]
async fn main() {
    let handler = NeovimHandler {
        state: Arc::new(Mutex::new(AppState::new())),
    };

    let (nvim, io_handler) = match create::new_parent(handler).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("mashless: failed to attach to Neovim: {e}");
            return;
        }
    };

    // Run until Neovim closes the channel (a normal quit) or errors.
    match io_handler.await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            if !err.is_reader_error() && !err.is_channel_closed() {
                let _ = nvim.err_writeln(&format!("mashless: {err}")).await;
            }
        }
        Err(join) => eprintln!("mashless: io loop join error: {join}"),
    }
}
