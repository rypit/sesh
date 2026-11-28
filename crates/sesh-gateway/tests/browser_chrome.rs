//! The browser session against a real Chromium (specs/010): set
//! `SESH_TEST_CHROME` to a Chromium binary to run it (and
//! `SESH_TEST_CHROME_ARGS` for extra flags, e.g. `--no-sandbox` as root). The stack test
//! (`browser_stack`) covers the same in the containers; this one runs in
//! seconds on a developer machine.

use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::{get, post};
use serde_json::json;
use sesh_gateway::browser::egress::{Proxy, Refusals};
use sesh_gateway::browser::session::{Session, read_fresh};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;

#[derive(Default, Clone)]
struct Seen(Arc<Mutex<Vec<String>>>);

async fn site(seen: Seen) -> u16 {
    let app = Router::new()
        .route("/article", get(|| async { Html("<html><head><title>Varden</title><style>p{color:red}</style></head><body><nav>menu</nav><article><h1>The Varden beacon</h1><p>It was first lit in 1873 on Skarvøy.</p><p><a href='/form'>The form</a></p></article></body></html>") }))
        .route("/form", get(|| async { Html("<html><body><form action='/got' method='post'><label for='q'>Search</label><input id='q' name='q'><input type='password' name='pw'><button>Go</button></form><a href='/dl'>Bill</a><a href='/exe'>Setup</a><form action='/got' method='post' enctype='multipart/form-data'><label>Your file <input type='file' name='f'></label></form></body></html>") }))
        .route("/dl", get(|| async { ([(header::CONTENT_TYPE, "application/pdf"), (header::CONTENT_DISPOSITION, "attachment; filename=\"bill.pdf\"")], "%PDF-1.4 fake bill") }))
        .route("/exe", get(|| async { ([(header::CONTENT_TYPE, "application/x-msdownload"), (header::CONTENT_DISPOSITION, "attachment; filename=\"setup.exe\"")], "MZ") }))
        .route("/redirect", get(|| async { Redirect::temporary("http://10.0.0.1/secret") }))
        .route("/sub", get(|| async { Html("<html><body><p>Hello</p><img src='http://192.168.1.1/x.png'></body></html>") }))
        .route("/got", post(|State(s): State<Seen>, body: String| async move { s.0.lock().unwrap().push(body); Html("<p>thanks</p>") }))
        .with_state(seen);
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    port
}

#[tokio::test]
#[ignore = "needs SESH_TEST_CHROME"]
async fn the_session_drives_a_real_browser_through_the_egress_proxy() {
    let Ok(chrome) = std::env::var("SESH_TEST_CHROME") else {
        return;
    };
    let seen = Seen::default();
    let port = site(seen.clone()).await;
    let refusals = Refusals::default();
    let pl = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", pl.local_addr().unwrap());
    tokio::spawn(
        Proxy {
            agent: "a".into(),
            allow_hosts: vec!["localhost".into()],
            refusals: refusals.clone(),
        }
        .serve(pl),
    );
    let profile = std::env::temp_dir().join(format!("sesh-chrome-{}", std::process::id()));
    let cdp_port = 19222 + (std::process::id() % 500) as u16;
    let mut child = tokio::process::Command::new(chrome)
        .args([
            "--headless=new",
            &format!("--remote-debugging-port={cdp_port}"),
            &format!("--user-data-dir={}", profile.display()),
            &format!("--proxy-server={proxy}"),
            "--proxy-bypass-list=<-loopback>",
            "--no-first-run",
            "--window-size=1280,800",
            "about:blank",
        ])
        .args(
            std::env::var("SESH_TEST_CHROME_ARGS")
                .unwrap_or_default()
                .split_whitespace(),
        )
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut ws = None;
    for _ in 0..50 {
        if let Ok(u) = sesh_gateway::browser::cdp::ws_url("127.0.0.1", cdp_port).await {
            ws = Some(u);
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let ws = ws.expect("chrome started");
    let refs = Arc::new(sesh_gateway::refs::Refs::default());
    let s = Session::open("a", "ana", &ws, refs.clone(), Duration::from_secs(3600))
        .await
        .unwrap();
    let t = Duration::from_secs(15);
    let base = format!("http://localhost:{port}");

    // read: a fresh context, Markdown of the main content only.
    let page = read_fresh(&s.cdp, &proxy, &format!("{base}/article"), 50_000, t)
        .await
        .unwrap();
    let md = page["markdown"].as_str().unwrap();
    assert!(
        md.contains("# The Varden beacon") && md.contains("1873"),
        "{md}"
    );
    assert!(!md.contains("menu") && !md.contains("color:red"), "{md}");

    // A redirect to the home network is refused by the proxy.
    refusals.take("a");
    let _ = read_fresh(&s.cdp, &proxy, &format!("{base}/redirect"), 50_000, t).await;
    let r = refusals.take("a");
    assert!(!r.is_empty() && r.iter().all(|h| h == "10.0.0.1"), "{r:?}");

    // open, snapshot: numbered elements, values never shown.
    s.navigate(&format!("{base}/form"), t).await.unwrap();
    let snap = s.run("snapshot", json!({})).await.unwrap();
    let els = snap["elements"].as_array().unwrap();
    let find = |k: &str, l: &str| {
        els.iter()
            .find(|e| e["kind"] == k && e["label"].as_str().unwrap_or("").contains(l))
            .unwrap_or_else(|| panic!("{k} {l}: {snap}"))["n"]
            .as_u64()
            .unwrap()
    };
    let q = find("text field", "Search");
    s.run("focus", json!({"n": q})).await.unwrap();
    s.insert_text("lighthouse secret").await.unwrap();
    let pw = els.iter().find(|e| e["kind"] == "password field").unwrap()["n"]
        .as_u64()
        .unwrap();
    s.run("focus", json!({"n": pw})).await.unwrap();
    s.insert_text("hunter2").await.unwrap();
    let snap = s.run("snapshot", json!({})).await.unwrap();
    let text = snap.to_string();
    assert!(
        !text.contains("lighthouse secret") && !text.contains("hunter2"),
        "values never shown: {text}"
    );
    assert!(text.contains("\"filled\":true"), "{text}");
    let go = find("submit button", "Go");
    let t_go = s.run("target", json!({"n": go})).await.unwrap();
    assert_eq!(t_go["submits"], true, "the gateway gates this click");

    // A download is kept by the gateway; a disallowed one is refused.
    let bill = find("link", "Bill");
    let tb = s.run("target", json!({"n": bill})).await.unwrap();
    s.click_at(tb["x"].as_f64().unwrap(), tb["y"].as_f64().unwrap())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (dl, refused) = s.take_downloads();
    assert_eq!(dl.len(), 1, "{dl:?} {refused:?}");
    assert_eq!(
        (dl[0].filename.as_str(), dl[0].mime.as_str()),
        ("bill.pdf", "application/pdf")
    );
    let a = refs.resolve(&dl[0].r#ref, "a").unwrap();
    assert!(
        refs.resolve(&dl[0].r#ref, "b").is_none(),
        "another agent's ref"
    );
    match &a.source {
        sesh_gateway::refs::Source::Download { bytes } => {
            assert_eq!(bytes.as_slice(), b"%PDF-1.4 fake bill")
        }
        other => panic!("{other:?}"),
    }
    let snap = s.run("snapshot", json!({})).await.unwrap();
    let exe = snap["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["label"] == "Setup")
        .unwrap()["n"]
        .as_u64()
        .unwrap();
    let te = s.run("target", json!({"n": exe})).await.unwrap();
    s.click_at(te["x"].as_f64().unwrap(), te["y"].as_f64().unwrap())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let (dl, refused) = s.take_downloads();
    assert!(
        dl.is_empty() && refused.iter().any(|r| r.contains("isn't allowed")),
        "{dl:?} {refused:?}"
    );

    // Upload: the gateway's script puts the file in the field.
    let snap = s.run("snapshot", json!({})).await.unwrap();
    let f = snap["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "file field")
        .unwrap()["n"]
        .as_u64()
        .unwrap();
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(b"%PDF-1.4 upload me");
    s.run(
        "set_file",
        json!({"n": f, "name": "u.pdf", "type": "application/pdf", "b64": b64}),
    )
    .await
    .unwrap();
    let snap = s.run("snapshot", json!({})).await.unwrap();
    let fe = snap["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "file field")
        .unwrap()
        .clone();
    assert_eq!(fe["filled"], true, "{fe}");

    // A subresource on the home network is blocked; the page still loads.
    refusals.take("a");
    s.navigate(&format!("{base}/sub"), t).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let r = refusals.take("a");
    assert!(!r.is_empty() && r.iter().all(|h| h == "192.168.1.1"), "{r:?}");

    // Live view frames.
    let mut frames = s.frames.subscribe();
    s.add_viewer().await.unwrap();
    s.navigate(&format!("{base}/article"), t).await.unwrap();
    let f = tokio::time::timeout(Duration::from_secs(10), frames.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&f[..2], &[0xff, 0xd8], "a JPEG frame");
    s.remove_viewer().await;

    // Takeover makes the agent wait, hand-back releases it.
    s.take_over("g1");
    let e = s
        .wait_for_agent(Duration::from_millis(300), Duration::from_secs(900))
        .await
        .unwrap_err();
    assert!(e.contains("the person is in control"), "{e}");
    assert!(s.hand_back("g1"));
    s.wait_for_agent(Duration::from_millis(300), Duration::from_secs(900))
        .await
        .unwrap();

    let _ = child.kill().await;
    let _ = std::fs::remove_dir_all(profile);
}
