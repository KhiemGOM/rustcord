//! `discord://` links, and one running Rustcord answering for all of them: a second launch hands
//! its link to the first, through a pipe named for the data directory, and exits.

use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::windows::named_pipe::ServerOptions;
use tokio::time::timeout;

use super::Msg;
use crate::state::Id;

#[derive(Clone, PartialEq, Debug)]
pub enum Link {
    Focus,
    Channel { guild: Id, channel: Id, message: Id },
    Invite(String),
    Template(String),
}

fn code(s: &str) -> Option<String> {
    Some(s).filter(|c| !c.is_empty() && c.len() <= 64 && c.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).map(str::to_string)
}

/// `discord://-/channels/…`, `discord:///app`, and the older `discord://invite/…` where the
/// first path part sits where the host would be.
pub fn parse(url: &str) -> Option<Link> {
    let rest = url.get(..10).filter(|p| p.eq_ignore_ascii_case("discord://")).and(url.get(10..))?;
    let rest = rest.split(['?', '#']).next()?;
    let mut parts = rest.split('/');
    let host = parts.next()?;
    let mut segs = Vec::new();
    if !matches!(host, "-" | "" | "discord.com" | "ptb.discord.com" | "canary.discord.com" | "discordapp.com") {
        segs.push(host);
    }
    segs.extend(parts.filter(|p| !p.is_empty()));
    let id = |s: &str| s.parse::<Id>().ok().filter(|n| *n > 0);
    Some(match segs.as_slice() {
        ["channels", g, c, rest @ ..] => {
            let guild = if *g == "@me" { 0 } else { id(g)? };
            Link::Channel { guild, channel: id(c)?, message: rest.first().and_then(|m| id(m)).unwrap_or(0) }
        }
        ["invite", c, ..] => Link::Invite(code(c)?),
        ["template", c, ..] => Link::Template(code(c)?),
        _ => Link::Focus,
    })
}

fn key() -> String {
    let dir = crate::cache::dir().to_string_lossy().to_lowercase();
    let h = dir.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}")
}

fn pipe() -> String {
    format!(r"\\.\pipe\rustcord-open-{}", key())
}

/// False when another Rustcord on this data directory already runs; the link this one was
/// started with has then been passed to it.
pub fn first_instance() -> bool {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name = crate::winsys::wide(&format!("Local\\Rustcord-{}", key()));
    // held until the process ends, which is what releases it
    let held = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if held.is_null() || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
        return true;
    }
    forward();
    false
}

fn forward() {
    use std::io::Write;
    let url = std::env::args().skip(1).find(|a| parse(a).is_some()).unwrap_or_else(|| "discord:///app".into());
    for _ in 0..50 {
        if let Ok(mut p) = std::fs::OpenOptions::new().write(true).open(pipe()) {
            let _ = p.write_all(url.as_bytes());
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn register() {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.display();
    let set = |key: &str, name: Option<&str>, value: &str| {
        let (key, value) = (crate::winsys::wide(key), crate::winsys::wide(value));
        let name = name.map(crate::winsys::wide);
        unsafe {
            RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()), REG_SZ, value.as_ptr().cast(), (value.len() * 2) as u32);
        }
    };
    set("Software\\Classes\\discord", None, "URL:Discord Protocol");
    set("Software\\Classes\\discord", Some("URL Protocol"), "");
    set("Software\\Classes\\discord\\DefaultIcon", None, &format!("\"{exe}\",0"));
    set("Software\\Classes\\discord\\shell\\open\\command", None, &format!("\"{exe}\" \"%1\""));
}

pub async fn serve() {
    if !super::testing() {
        register();
    }
    if let Some(l) = std::env::args().skip(1).find_map(|a| parse(&a)) {
        super::send(Msg::Open(l));
    }
    let path = pipe();
    let Ok(mut server) = ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&path) else { return };
    loop {
        if server.connect().await.is_err() {
            return;
        }
        let Ok(next) = ServerOptions::new().reject_remote_clients(true).create(&path) else { return };
        let io = std::mem::replace(&mut server, next);
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = timeout(Duration::from_secs(3), io.take(2048).read_to_end(&mut buf)).await;
            if let Some(l) = parse(String::from_utf8_lossy(&buf).trim()) {
                super::send(Msg::Open(l));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_discord_hands_out() {
        let ch = |guild, channel, message| Some(Link::Channel { guild, channel, message });
        assert_eq!(parse("discord://-/channels/1000/2011"), ch(1000, 2011, 0));
        assert_eq!(parse("discord:///channels/@me/3000/77?x=1"), ch(0, 3000, 77));
        assert_eq!(parse("DISCORD://discord.com/channels/1/2/3"), ch(1, 2, 3));
        assert_eq!(parse("discord://-/invite/aBc-12"), Some(Link::Invite("aBc-12".into())));
        assert_eq!(parse("discord://invite/abc"), Some(Link::Invite("abc".into())));
        assert_eq!(parse("discord://-/template/tpl9"), Some(Link::Template("tpl9".into())));
        assert_eq!(parse("discord:///app"), Some(Link::Focus));
        assert_eq!(parse("discord://-/library"), Some(Link::Focus));
    }

    #[test]
    fn what_isnt_ours_or_isnt_sane_is_dropped() {
        assert_eq!(parse("https://discord.com/channels/1/2"), None);
        assert_eq!(parse("discord://-/invite/../etc"), None);
        assert_eq!(parse("discord://-/channels/x/2"), None);
        assert_eq!(parse("discord://-/channels/1/0"), None);
        assert_eq!(parse(""), None);
    }
}
