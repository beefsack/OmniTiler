//! `tiler-live-fixture serve|send`: one binary, two roles.
//!
//! `serve` owns the winit event loop on the main thread and accepts JSON-lines
//! clients on a private local endpoint (Unix socket on Linux/macOS,
//! owner-restricted named pipe on Windows). `send` is the thin client: it
//! forwards JSON lines from arguments or stdin and prints reply/event lines.

mod client;
#[cfg(windows)]
mod pipe_win;
mod platform;
mod protocol;
mod render;
mod state;
mod transport;
mod win;

use std::io::BufRead;
#[cfg(any(unix, windows))]
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, atomic::AtomicBool, mpsc};

use state::Server;
use transport::{Endpoint, FrameError, FrameRead};

const SOCKET_NAME: &str = "fixture.sock";

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]),
        Some("send") => send(&args[1..]),
        _ => {
            eprintln!("usage:");
            eprintln!("  tiler-live-fixture serve [--dir PATH] [--socket-name NAME]");
            eprintln!(
                "  tiler-live-fixture send --socket PATH --nonce NONCE [--events N|--follow] [--compact] [JSON...]"
            );
            eprintln!(
                "  tiler-live-fixture send --socket PATH --nonce NONCE --wait-ms N --expect JSON [--compact] [JSON...]"
            );
            2
        }
    }
}

// ---------------------------------------------------------------------------
// serve
// ---------------------------------------------------------------------------

fn serve(args: &[String]) -> i32 {
    let mut dir: Option<PathBuf> = None;
    let mut socket_name = SOCKET_NAME.to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dir" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("serve: --dir needs a path");
                    return 2;
                }
                dir = Some(PathBuf::from(&args[i]));
            }
            "--socket-name" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("serve: --socket-name needs a name");
                    return 2;
                }
                socket_name = args[i].clone();
            }
            other => {
                eprintln!("serve: unknown argument '{other}'");
                return 2;
            }
        }
        i += 1;
    }

    let dir = dir.unwrap_or_else(transport::default_socket_dir);
    let dir_preexisted = dir.exists();
    if let Err(e) = transport::prepare_socket_dir(&dir) {
        eprintln!("serve: cannot prepare socket dir {}: {e}", dir.display());
        return 1;
    }
    let (listener, socket_path) = match transport::bind_private_socket(&dir, &socket_name) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("serve: cannot bind socket: {e}");
            return 1;
        }
    };

    let nonce = match transport::generate_nonce() {
        Ok(nonce) => nonce,
        Err(e) => {
            eprintln!("serve: cannot generate run nonce: {e}");
            cleanup_socket(&socket_path, &dir, dir_preexisted);
            return 1;
        }
    };
    let endpoint = Endpoint { socket_path, nonce };
    let run_tag = transport::generate_run_tag();
    let pid = std::process::id();
    // The main thread owns the server and all windows. IO threads only
    // forward whole request lines and wait for replies outside any state
    // lock; there is no shared Server mutex to deadlock against window
    // events.
    let server = Server::new(pid, run_tag, endpoint.nonce.clone());
    // The endpoint announcement is the only stdout line and the only place the
    // nonce appears. Logs below go to stderr and never carry the nonce.
    println!(
        "{}",
        serde_json::json!({ "socket": endpoint.socket_path, "nonce": endpoint.nonce, "pid": pid })
    );

    let event_loop = match winit::event_loop::EventLoop::<()>::with_user_event().build() {
        Ok(loop_) => loop_,
        Err(e) => {
            eprintln!("serve: event loop failed: {e}");
            cleanup_socket(&endpoint.socket_path, &dir, dir_preexisted);
            return 1;
        }
    };
    let proxy = event_loop.create_proxy();
    let (requests_tx, requests_rx) = mpsc::channel();
    let backend_shared: Arc<OnceLock<String>> = Arc::new(OnceLock::new());
    let shutdown = Arc::new(AtomicBool::new(false));
    let sender = win::MainSender::new(requests_tx, proxy);

    let accept_shutdown = Arc::clone(&shutdown);
    let accept_sender = sender.clone();
    let accept_path = endpoint.socket_path.clone();
    let next_conn = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let accept_conns = Arc::clone(&next_conn);
    let accept_thread = std::thread::spawn(move || {
        accept_loop(
            listener,
            accept_sender,
            accept_shutdown,
            accept_path,
            accept_conns,
        )
    });

    let code = match win::run(event_loop, server, requests_rx, backend_shared, shutdown) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("serve: {e}");
            1
        }
    };
    // Main loop exited: every window is dropped with the App. Join the accept
    // thread (it is already past its loop once shutdown was requested) and
    // remove our own socket file.
    let _ = accept_thread.join();
    cleanup_socket(&endpoint.socket_path, &dir, dir_preexisted);
    code
}

/// Remove the socket file this run created, plus the directory when this run
/// created it. Never touches preexisting user directories beyond our file.
/// On Windows the endpoint is a kernel pipe name (no file to remove); only
/// the run directory lifecycle applies.
fn cleanup_socket(socket_path: &std::path::Path, dir: &std::path::Path, dir_preexisted: bool) {
    let _ = std::fs::remove_file(socket_path);
    if !dir_preexisted {
        let _ = std::fs::remove_dir(dir);
    }
}

/// Accept connections until shutdown. Each connection gets a reader thread
/// and a writer thread sharing one event queue, so async subscription events
/// and request replies serialize onto the endpoint in arrival order.
#[cfg(unix)]
fn accept_loop(
    listener: transport::Listener,
    sender: win::MainSender,
    shutdown: Arc<AtomicBool>,
    socket_path: PathBuf,
    next_conn: Arc<std::sync::atomic::AtomicU64>,
) {
    use std::sync::atomic::Ordering;
    // Blocking accept: no accept polling. Shutdown wakes the loop with a
    // single dummy connection made by the thread serving `shutdown`.
    if let Err(e) = listener.set_nonblocking(false) {
        eprintln!("serve: cannot configure listener: {e}");
        return;
    }
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let sender = sender.clone();
                let shutdown = Arc::clone(&shutdown);
                let socket_path = socket_path.clone();
                let conn = next_conn.fetch_add(1, Ordering::Relaxed);
                std::thread::spawn(move || {
                    serve_connection(stream, sender, shutdown, &socket_path, conn);
                });
            }
            // Backoff on transient accept errors only; the steady state blocks
            // in accept() and wakes via a real or dummy connection.
            Err(e) => {
                eprintln!("serve: accept failed: {e}");
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// Accept connections until shutdown (Windows named-pipe variant).
/// Same threading shape as Unix: blocking accept, dummy client wakes the
/// loop after `shutdown`.
#[cfg(windows)]
fn accept_loop(
    listener: transport::Listener,
    sender: win::MainSender,
    shutdown: Arc<AtomicBool>,
    socket_path: PathBuf,
    next_conn: Arc<std::sync::atomic::AtomicU64>,
) {
    use std::sync::atomic::Ordering;
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match listener.accept() {
            Ok(stream) => {
                let sender = sender.clone();
                let shutdown = Arc::clone(&shutdown);
                let socket_path = socket_path.clone();
                let conn = next_conn.fetch_add(1, Ordering::Relaxed);
                std::thread::spawn(move || {
                    serve_connection(stream, sender, shutdown, &socket_path, conn);
                });
            }
            Err(e) => {
                eprintln!("serve: accept failed: {e}");
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn accept_loop(
    _: transport::Listener,
    _: win::MainSender,
    _: Arc<AtomicBool>,
    _: PathBuf,
    _: Arc<std::sync::atomic::AtomicU64>,
) {
}

#[cfg(any(unix, windows))]
fn serve_connection(
    stream: transport::Stream,
    sender: win::MainSender,
    shutdown: Arc<AtomicBool>,
    socket_path: &std::path::Path,
    conn: u64,
) {
    use std::sync::atomic::Ordering;
    let reader_stream = match transport::try_clone_stream(&stream) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("serve: connection setup failed: {e}");
            return;
        }
    };
    let (event_tx, event_rx) = mpsc::channel::<String>();
    let writer = std::thread::spawn(move || {
        let mut stream = stream;
        for line in event_rx {
            if transport::write_line(&mut stream, &line).is_err() {
                break;
            }
        }
    });

    let mut reader = BufReader::new(reader_stream);
    let mut scratch = Vec::new();
    loop {
        // Shutdown is set by the main thread itself when it handles
        // `shutdown`; the check here only stops reading once the loop is
        // going away. No state lock is held while waiting for the reply.
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match transport::read_bounded_line(&mut reader, &mut scratch) {
            Ok(FrameRead::Eof) => break,
            Ok(FrameRead::Line(line)) => {
                let Some(outcome) = sender.submit_request(line, conn, &event_tx) else {
                    break;
                };
                if event_tx.send(outcome.reply).is_err() {
                    break;
                }
                if outcome.shutting_down {
                    // Wake the accept loop: the main thread already flagged
                    // shutdown and will exit its event loop after replying.
                    sender.wake();
                    let _ = transport::connect(socket_path);
                    break;
                }
            }
            Err(FrameError::TooLong) => {
                let reply =
                    crate::protocol::Reply::fail("", "frame exceeds 1 MiB; connection closed");
                let _ = event_tx.send(crate::protocol::reply_to_string(&reply));
                break;
            }
            Err(_) => break,
        }
    }
    sender.submit_disconnect(conn);
    drop(event_tx);
    let _ = writer.join();
}

#[cfg(not(any(unix, windows)))]
fn serve_connection(
    _: transport::Stream,
    _: win::MainSender,
    _: Arc<AtomicBool>,
    _: &std::path::Path,
    _: u64,
) {
}

// ---------------------------------------------------------------------------
// send
// ---------------------------------------------------------------------------

fn send(args: &[String]) -> i32 {
    let mut socket: Option<PathBuf> = None;
    let mut nonce = String::new();
    let mut events: Option<u64> = None;
    let mut follow = false;
    let mut compact = false;
    let mut wait_ms: Option<u64> = None;
    let mut expect_raw: Option<String> = None;
    let mut payloads: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--socket" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("send: --socket needs a path");
                    return 2;
                }
                socket = Some(PathBuf::from(&args[i]));
            }
            "--nonce" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("send: --nonce needs a value");
                    return 2;
                }
                nonce = args[i].clone();
            }
            "--events" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("send: --events needs a count");
                    return 2;
                }
                match args[i].parse() {
                    Ok(n) => events = Some(n),
                    Err(_) => {
                        eprintln!("send: --events needs a numeric count");
                        return 2;
                    }
                }
            }
            "--follow" => follow = true,
            "--compact" => compact = true,
            "--wait-ms" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("send: --wait-ms needs a millisecond count");
                    return 2;
                }
                match args[i].parse() {
                    Ok(n) => wait_ms = Some(n),
                    Err(_) => {
                        eprintln!("send: --wait-ms needs a numeric millisecond count");
                        return 2;
                    }
                }
            }
            "--expect" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("send: --expect needs a JSON object");
                    return 2;
                }
                expect_raw = Some(args[i].clone());
            }
            other if other.starts_with("--") => {
                eprintln!("send: unknown argument '{other}'");
                return 2;
            }
            other => payloads.push(other.to_string()),
        }
        i += 1;
    }
    let Some(socket) = socket else {
        eprintln!("send: --socket is required");
        return 2;
    };
    if nonce.is_empty() {
        eprintln!("send: --nonce is required");
        return 2;
    }
    if wait_ms.is_some() != expect_raw.is_some() {
        eprintln!("send: --wait-ms and --expect must be given together");
        return 2;
    }
    if wait_ms.is_some() && (events.is_some() || follow) {
        eprintln!("send: --wait-ms cannot be combined with --events or --follow");
        return 2;
    }
    if payloads.is_empty() {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) if !line.trim().is_empty() => payloads.push(line),
                Ok(_) => {}
                Err(e) => {
                    eprintln!("send: stdin failed: {e}");
                    return 1;
                }
            }
        }
    }
    if payloads.is_empty() {
        eprintln!("send: no JSON lines given (arguments or stdin)");
        return 2;
    }
    if let (Some(ms), Some(raw)) = (wait_ms, expect_raw) {
        if ms == 0 {
            eprintln!("send: --wait-ms must be positive");
            return 2;
        }
        let expect = match client::parse_expect(&raw) {
            Ok(expect) => expect,
            Err(e) => {
                eprintln!("send: {e}");
                return 2;
            }
        };
        match client::send_wait(&socket, &nonce, &payloads, ms, &expect, compact) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("send: {e}");
                1
            }
        }
    } else {
        match client::send_lines(&socket, &nonce, &payloads, events, follow, compact) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("send: {e}");
                1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Full framed round trip over a real socket pair with the fake host: no
    /// event loop is initialized, but framing, auth, dispatch, replies and
    /// subscription events all travel as bytes.
    #[cfg(unix)]
    #[test]
    fn socket_dispatch_round_trip_without_event_loop() {
        use crate::state::test_host::FakeHost;
        use std::io::BufReader;
        use std::os::unix::net::UnixStream;

        let (client, server_stream) = UnixStream::pair().expect("socket pair");
        let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
        let mut host = FakeHost::x11();
        let (event_tx, event_rx) = mpsc::channel::<String>();
        const CONN: u64 = 1;

        // Writer thread mirrors serve_connection: one queue carries replies
        // and broadcast events onto the socket in arrival order.
        let writer_stream = server_stream.try_clone().expect("clone");
        let writer = std::thread::spawn(move || {
            let mut stream = writer_stream;
            for line in event_rx {
                if transport::write_line(&mut stream, &line).is_err() {
                    break;
                }
            }
        });

        let handler = std::thread::spawn(move || {
            let mut reader = BufReader::new(server_stream);
            let mut scratch = Vec::new();
            while let Ok(FrameRead::Line(line)) =
                transport::read_bounded_line(&mut reader, &mut scratch)
            {
                let reply = server.handle_line(&mut host, &line, CONN, Some(&event_tx));
                if event_tx.send(reply).is_err() {
                    break;
                }
                if server.is_shutting_down() {
                    break;
                }
            }
        });

        let mut client = client;
        let mut reader = BufReader::new(client.try_clone().expect("clone"));
        let mut scratch = Vec::new();
        let mut send = |line: &str| transport::write_line(&mut client, line).expect("write");
        let mut recv =
            || match transport::read_bounded_line(&mut reader, &mut scratch).expect("read") {
                FrameRead::Line(line) => line,
                FrameRead::Eof => panic!("unexpected EOF"),
            };
        let mut recv_json =
            || serde_json::from_str::<serde_json::Value>(&recv()).expect("reply JSON");

        // Wrong nonce: rejected, server untouched.
        send(r#"{"v":1,"request_id":"bad","nonce":"wrong","op":"list"}"#);
        assert_eq!(recv_json()["error"], "bad nonce");

        // Spawn two windows in one request.
        send(
            r#"{"v":1,"request_id":"s1","nonce":"secret","op":"spawn","windows":[{"id":"a","size_px":[640,480],"description":"first"},{"id":"b","mode":"maximized"}]}"#,
        );
        let reply = recv_json();
        assert_eq!(reply["ok"], true);
        assert_eq!(
            reply["result"]["windows"][0]["snapshot"]["requested"]["description"],
            "first"
        );
        assert_eq!(
            reply["result"]["windows"][1]["snapshot"]["requested"]["mode"],
            "maximized"
        );

        // Batch: ordered set/get/close across both ids.
        send(
            r#"{"v":1,"request_id":"b1","nonce":"secret","op":"batch","items":[{"op":"set","ids":["a","b"],"patch":{"decorations":false}},{"op":"get","ids":["a","ghost"]}] }"#,
        );
        let reply = recv_json();
        assert_eq!(reply["ok"], false);
        assert_eq!(
            reply["result"]["items"][0]["windows"][0]["fields"][0]["field"],
            "decorations"
        );
        assert!(
            reply["result"]["items"][1]["windows"][1]["error"]
                .as_str()
                .unwrap()
                .contains("unknown")
        );

        // Subscribe, then observe the creation event for a new window.
        send(r#"{"v":1,"request_id":"sub","nonce":"secret","op":"subscribe"}"#);
        assert_eq!(recv_json()["event"], "subscribed");
        let reply = recv_json();
        assert_eq!(reply["result"]["subscribed"], serde_json::json!(["all"]));
        send(r#"{"v":1,"request_id":"s2","nonce":"secret","op":"spawn","windows":[{"id":"c"}]}"#);
        // Broadcasts interleave with replies; clients tell them apart by shape.
        let event = recv_json();
        assert_eq!(event["event"], "created");
        assert_eq!(event["id"], "c");
        assert_eq!(recv_json()["ok"], true);

        // Shutdown ends the session; closing first would not have.
        send(r#"{"v":1,"request_id":"end","nonce":"secret","op":"shutdown"}"#);
        let reply = recv_json();
        assert_eq!(reply["result"]["shutting_down"], true);

        drop(client);
        handler.join().expect("handler thread");
        writer.join().expect("writer thread");
    }

    /// Main-thread ownership without locks: one dispatcher thread owns the
    /// server and the fake host, while request threads wait outside any state
    /// lock. A host event arriving between requests serializes on the same
    /// queue instead of deadlocking against a held mutex (the old
    /// `serve_connection` held `server.lock()` across a synchronous host
    /// round trip while window events locked the same server).
    #[test]
    fn dispatcher_interleaves_requests_and_host_events_without_locks() {
        use crate::state::{HostEvent, test_host::FakeHost};

        enum FakeJob {
            Request {
                line: String,
                conn: u64,
                events_tx: mpsc::Sender<String>,
                reply_tx: mpsc::SyncSender<String>,
            },
            HostEvent {
                id: String,
                event: HostEvent,
            },
        }

        let (jobs_tx, jobs_rx) = mpsc::channel::<FakeJob>();
        let (event_tx, _event_rx) = mpsc::channel::<String>();
        let dispatcher = std::thread::spawn(move || {
            let mut server = Server::new(4242, "run1".to_string(), "secret".to_string());
            let mut host = FakeHost::x11();
            while let Ok(job) = jobs_rx.recv() {
                match job {
                    FakeJob::Request {
                        line,
                        conn,
                        events_tx,
                        reply_tx,
                    } => {
                        let reply = server.handle_line(&mut host, &line, conn, Some(&events_tx));
                        let _ = reply_tx.send(reply);
                    }
                    FakeJob::HostEvent { id, event } => {
                        server.note_host_event(&id, event);
                    }
                }
            }
        });

        let submit = |line: &str| {
            let (tx, rx) = mpsc::sync_channel(1);
            jobs_tx
                .send(FakeJob::Request {
                    line: line.to_string(),
                    conn: 1,
                    events_tx: event_tx.clone(),
                    reply_tx: tx,
                })
                .expect("dispatcher alive");
            // Blocking wait holds no state lock: the dispatcher owns all
            // state on its own thread.
            rx.recv().expect("dispatcher reply")
        };

        let out = submit(
            r#"{"v":1,"request_id":"s","nonce":"secret","op":"spawn","windows":[{"id":"a"}]}"#,
        );
        assert!(out.contains("\"ok\":true"));

        // A request and a host event race on the same dispatcher queue from
        // different threads. Either sequential order is valid; both must
        // complete (no lock cycle) and the window must stay coherent.
        let jobs_tx_a = jobs_tx.clone();
        let event_tx_a = event_tx.clone();
        let request_thread = std::thread::spawn(move || {
            let (tx, rx) = mpsc::sync_channel(1);
            jobs_tx_a
                .send(FakeJob::Request {
                    line: r#"{"v":1,"request_id":"set","nonce":"secret","op":"set","ids":["a"],"patch":{"size_px":[800,600]}}"#.to_string(),
                    conn: 1,
                    events_tx: event_tx_a,
                    reply_tx: tx,
                })
                .expect("send request");
            rx.recv().expect("request reply")
        });
        let jobs_tx_b = jobs_tx.clone();
        let event_thread = std::thread::spawn(move || {
            jobs_tx_b
                .send(FakeJob::HostEvent {
                    id: "a".to_string(),
                    event: HostEvent::Resized {
                        size_px: [800, 600],
                        scale_factor: 1.0,
                    },
                })
                .expect("send host event");
        });
        let reply = request_thread.join().expect("request thread");
        event_thread.join().expect("event thread");
        assert!(reply.contains("\"ok\":true"));

        // After both jobs drain, a reconciling get observes the settled size.
        let out = submit(r#"{"v":1,"request_id":"g","nonce":"secret","op":"get","ids":["a"]}"#);
        let reply: serde_json::Value = serde_json::from_str(&out).expect("reply JSON");
        assert_eq!(
            reply["result"]["windows"][0]["snapshot"]["observed"]["size_px"],
            serde_json::json!([800, 600])
        );
        assert_eq!(
            reply["result"]["windows"][0]["snapshot"]["settled"],
            serde_json::json!(true)
        );

        drop(jobs_tx);
        drop(event_tx);
        dispatcher.join().expect("dispatcher thread");
    }
}
