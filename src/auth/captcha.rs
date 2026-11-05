use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

pub fn open_url(url: &str) {
    match std::env::var_os("RUSTCORD_E2E") {
        Some(dir) => {
            let _ = std::fs::write(std::path::Path::new(&dir).join("captcha_url"), url);
        }
        None => {
            let _ = open::that_detached(url);
        }
    }
}

fn page(sitekey: &str, rqdata: Option<&str>, secret: &str) -> String {
    let data = rqdata.map(|d| format!("hcaptcha.setData(id, {{ rqdata: {} }});", serde_json::json!(d))).unwrap_or_default();
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>Rustcord captcha</title>
<style>body{{background:#1e1f22;color:#dbdee1;font-family:sans-serif;display:grid;place-items:center;height:100vh;margin:0}}</style>
<div><p id="msg">Solve the captcha to continue logging in to Rustcord.</p><div id="c"></div></div>
<script src="https://js.hcaptcha.com/1/api.js?render=explicit&onload=ready" async defer></script>
<script>
function ready() {{
  var id = hcaptcha.render("c", {{ sitekey: {sitekey}, theme: "dark", callback: function (t) {{
    fetch("/{secret}", {{ method: "POST", body: t }}).then(function () {{ document.getElementById("msg").textContent = "Done. You can close this tab."; }});
  }} }});
  {data}
}}
</script>"#,
        sitekey = serde_json::json!(sitekey),
    )
}

fn serve(mut s: TcpStream, html: &str, secret: &str) -> Option<String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let head_end = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
        match s.read(&mut tmp) {
            Ok(0) | Err(_) => return None,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut first = head.lines().next()?.split_whitespace();
    let (method, path) = (first.next()?, first.next()?);
    let reply = |s: &mut TcpStream, code: &str, ctype: &str, body: &str| {
        let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    };
    if method == "POST" && path == format!("/{secret}") {
        let len: usize = head.lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").and_then(|v| v.trim().parse().ok())).unwrap_or(0);
        while buf.len() < head_end + len {
            match s.read(&mut tmp) {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&tmp[..n]),
            }
        }
        let token = String::from_utf8_lossy(&buf[head_end..]).trim().to_string();
        reply(&mut s, "204 No Content", "text/plain", "");
        return (!token.is_empty()).then_some(token);
    }
    if method == "GET" && path == "/" {
        reply(&mut s, "200 OK", "text/html; charset=utf-8", html);
    } else {
        reply(&mut s, "404 Not Found", "text/plain", "");
    }
    None
}

pub fn solve_captcha(sitekey: &str, rqdata: Option<&str>) -> Option<String> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    listener.set_nonblocking(true).ok()?;
    let secret = {
        use std::hash::{BuildHasher, Hasher};
        format!("{:016x}", std::collections::hash_map::RandomState::new().build_hasher().finish())
    };
    let html = page(sitekey, rqdata, &secret);
    open_url(&format!("http://127.0.0.1:{}/", listener.local_addr().ok()?.port()));
    let end = Instant::now() + Duration::from_secs(300);
    while Instant::now() < end {
        match listener.accept() {
            Ok((s, _)) => {
                let _ = s.set_nonblocking(false);
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                if let Some(t) = serve(s, &html, &secret) {
                    return Some(t);
                }
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    None
}
