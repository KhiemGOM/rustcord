//! The login screens against the mock (RUSTCORD_E2E_LOGIN): no saved token, every way in.

use egui::{Context, Event, Rect};
use std::io::{Read, Write};
use std::time::Duration;

use super::Run;
use crate::auth::Qr;
use crate::ui::App;
use crate::ui::login::Page;

const SITEKEY: &str = "10000000-ffff-ffff-ffff-000000000001";

fn solve(url: &str, token: &str) -> Option<bool> {
    let addr = url.trim_start_matches("http://").trim_end_matches('/');
    let talk = |req: String| -> Option<String> {
        let mut s = std::net::TcpStream::connect(addr).ok()?;
        s.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
        s.write_all(req.as_bytes()).ok()?;
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        Some(out)
    };
    let page = talk(format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"))?;
    let secret = page.split("fetch(\"/").nth(1)?.split('"').next()?;
    talk(format!("POST /{secret} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{token}", token.len()))?;
    Some(page.contains(SITEKEY) && page.contains("mock-rqdata"))
}

fn scan() {
    let base = crate::api::base().trim_start_matches("http://");
    let (addr, path) = base.split_once('/').map(|(a, p)| (a, format!("/{p}"))).unwrap_or((base, String::new()));
    if let Ok(mut s) = std::net::TcpStream::connect(addr) {
        let _ = write!(s, "POST {path}/mock/scan HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
}

impl Run {
    fn rect(app: &App, key: &str) -> Option<Rect> {
        app.login.rects.get(key).copied()
    }

    fn press(&mut self, app: &App, key: &str) -> bool {
        let Some(r) = Self::rect(app, key) else { return false };
        self.click(r.center());
        true
    }

    fn type_into(&mut self, app: &App, key: &str, text: &str) -> bool {
        if !self.press(app, key) {
            return false;
        }
        self.queue.push_back(vec![Event::Text(text.into())]);
        true
    }

    fn sign_in_as(&mut self, app: &mut App, who: &str) {
        app.login.login = who.into();
        app.login.password = "hunter2".into();
        self.press(app, "submit");
    }

    fn signed_in(&mut self, app: &mut App, mark: &str) -> bool {
        if app.token.as_deref() != Some("mock-token") {
            return false;
        }
        self.mark(mark);
        app.logout();
        self.next(300);
        true
    }

    fn error_has(app: &App, field: &str, part: &str) -> bool {
        app.login.errs.iter().any(|(f, m)| f == field && m.contains(part))
    }

    pub(super) fn login_steps(&mut self, app: &mut App, ctx: &Context) {
        let has = |app: &App, keys: &[&str]| keys.iter().all(|k| Self::rect(app, k).is_some());
        let on_login = |app: &App| app.login.page == Page::Login && has(app, &["login", "password", "submit"]);
        let on_mfa = |app: &App| app.login.page == Page::Mfa;
        let step = self.step;
        match step {
            1500 if on_login(app) => {
                self.mark("login_screen");
                self.shot(ctx, "login.png");
                self.next(200);
            }
            1501 if self.type_into(app, "login", "wrong@mock.test") => self.next(100),
            1502 if self.type_into(app, "password", "nope") => self.next(100),
            1503 if self.press(app, "submit") => self.next(300),
            1504 if Self::error_has(app, "login", "invalid") && Self::error_has(app, "password", "invalid") => {
                self.mark("login_field_error");
                self.shot(ctx, "login_error.png");
                self.next(100);
            }
            1505 if on_login(app) => {
                self.sign_in_as(app, "captcha@mock.test");
                self.next(300);
            }
            1506 if app.login.captcha => {
                let url = std::fs::read_to_string(self.dir.join("captcha_url")).unwrap_or_default();
                if url.is_empty() {
                    return;
                }
                self.mark("login_captcha_opened");
                if solve(&url, "e2e-captcha") == Some(true) {
                    self.mark("login_captcha_page");
                }
                self.next(200);
            }
            1507 => {
                self.signed_in(app, "login_captcha_solved");
            }
            1508 if on_login(app) => {
                self.sign_in_as(app, "mfa@mock.test");
                self.next(300);
            }
            1509 if on_mfa(app) && Self::rect(app, "code").is_some() => {
                self.mark("mfa_shown");
                self.shot(ctx, "mfa.png");
                self.type_into(app, "code", "000000");
                self.next(100);
            }
            1510 if self.press(app, "confirm") => self.next(300),
            1511 if app.login.mfa.err.is_some() => {
                self.mark("mfa_bad_code");
                app.login.mfa.code.clear();
                self.next(100);
            }
            1512 if self.type_into(app, "code", "123456") => self.next(100),
            1513 if self.press(app, "confirm") => self.next(300),
            1514 => {
                self.signed_in(app, "mfa_totp");
            }
            1515 if on_login(app) => {
                self.sign_in_as(app, "mfa@mock.test");
                self.next(300);
            }
            1516 if on_mfa(app) && self.press(app, "other") => self.next(200),
            1517 if app.login.mfa.kind == "select" && Self::rect(app, "sms").is_some() => {
                self.mark("mfa_switcher");
                self.shot(ctx, "mfa_select.png");
                self.press(app, "sms");
                self.next(200);
            }
            1518 if app.login.mfa.phone.is_some() && Self::rect(app, "code").is_some() => {
                self.mark("mfa_sms_sent");
                self.type_into(app, "code", "654321");
                self.next(100);
            }
            1519 if self.press(app, "confirm") => self.next(300),
            1520 => {
                self.signed_in(app, "mfa_sms");
            }
            1521 if on_login(app) => {
                self.sign_in_as(app, "mfa@mock.test");
                self.next(300);
            }
            1522 if on_mfa(app) && self.press(app, "other") => self.next(200),
            1523 if app.login.mfa.kind == "select" && self.press(app, "backup") => self.next(200),
            1524 if app.login.mfa.kind == "backup_warn" => {
                self.mark("mfa_backup_warn");
                self.shot(ctx, "mfa_backup.png");
                self.next(10_800);
            }
            1525 if self.press(app, "ack") => self.next(200),
            1526 if app.login.mfa.kind == "backup" && self.type_into(app, "code", "abcd-1234") => self.next(100),
            1527 if self.press(app, "confirm") => self.next(300),
            1528 => {
                self.signed_in(app, "mfa_backup");
            }
            1529 if on_login(app) => {
                self.sign_in_as(app, "newloc@mock.test");
                self.next(300);
            }
            1530 if Self::error_has(app, "login", "New login location detected") => {
                self.mark("login_new_location");
                self.shot(ctx, "login_new_location.png");
                self.press(app, "submit");
                self.next(300);
            }
            1531 => {
                self.signed_in(app, "login_new_location_ok");
            }
            1532 if on_login(app) => {
                self.sign_in_as(app, "disabled@mock.test");
                self.next(300);
            }
            1533 if matches!(app.login.page, Page::Disabled(_)) => {
                self.mark("login_disabled");
                self.shot(ctx, "login_disabled.png");
                self.press(app, "back");
                self.next(200);
            }
            1534 if on_login(app) => {
                self.sign_in_as(app, "deleted@mock.test");
                self.next(300);
            }
            1535 if app.login.page == Page::Deletion && self.press(app, "reactivate") => {
                self.mark("login_deletion");
                self.next(300);
            }
            1536 => {
                self.signed_in(app, "login_reactivated");
            }
            1537 if on_login(app) && self.press(app, "forgot") => self.next(200),
            1538 if app.login.page == Page::Forgot && has(app, &["login", "submit"]) => {
                app.login.login = "unknown@mock.test".into();
                self.press(app, "submit");
                self.next(300);
            }
            1539 if Self::error_has(app, "login", "No account") => {
                self.mark("forgot_error");
                app.login.login = "ok@mock.test".into();
                self.press(app, "submit");
                self.next(300);
            }
            1540 if app.login.page == Page::Sent && self.press(app, "submit") => {
                self.mark("forgot_sent");
                self.next(200);
            }
            1541 if on_login(app) && self.press(app, "register") => self.next(600),
            1542 if app.login.page == Page::Register && has(app, &["email", "submit"]) => {
                self.press(app, "submit");
                self.next(200);
            }
            1543 if ["email", "username", "password", "date_of_birth", "consent"].iter().all(|f| app.login.errs.iter().any(|(e, _)| e == f)) => {
                self.mark("register_required");
                self.shot(ctx, "register_errors.png");
                app.login.errs.clear();
                self.type_into(app, "email", "taken@mock.test");
                self.next(100);
            }
            1544 if self.type_into(app, "username", "taken") => self.next(100),
            1545 if app.login.reg.taken() == Some(true) => {
                self.mark("register_name_taken");
                self.shot(ctx, "register.png");
                app.login.reg.username.clear();
                self.type_into(app, "username", "e2e_user");
                self.next(100);
            }
            1546 if app.login.reg.taken() == Some(false) => {
                self.mark("register_name_free");
                let r = &mut app.login.reg;
                (r.password, r.month, r.day, r.year) = ("hunter2hunter2".into(), 5, 17, 1999);
                self.press(app, "consent");
                self.next(200);
            }
            1547 if self.press(app, "submit") => self.next(300),
            1548 if Self::error_has(app, "email", "already registered") => {
                self.mark("register_field_error");
                app.login.reg.email = "e2e@mock.test".into();
                self.press(app, "submit");
                self.next(300);
            }
            1549 => {
                self.signed_in(app, "register_ok");
            }
            1550 if matches!(app.login.qr.state, Qr::Ready(_)) && Self::rect(app, "qr").is_some() => {
                self.mark("qr_shown");
                self.shot(ctx, "qr.png");
                scan();
                self.next(200);
            }
            1551 if matches!(app.login.qr.state, Qr::Ticket { .. }) && Self::rect(app, "qr_user").is_some() => {
                self.mark("qr_ticket");
                self.shot(ctx, "qr_ticket.png");
                self.next(200);
            }
            1552 => {
                self.signed_in(app, "qr_login");
            }
            1553 if on_login(app) && self.press(app, "passkey") => self.next(500),
            1554 if app.login.busy => {
                self.mark("passkey_ui_alive");
                self.shot(ctx, "passkey_busy.png");
                self.next(100);
            }
            1555 => {
                self.signed_in(app, "passkey_login");
            }
            1556 if on_login(app) => {
                unsafe { std::env::set_var("RUSTCORD_FAKE_PASSKEY", "cancel") };
                self.press(app, "passkey");
                self.next(2200);
            }
            1557 if app.login.err.as_deref() == Some("Cancelled.") => {
                unsafe { std::env::set_var("RUSTCORD_FAKE_PASSKEY", "mock-assertion") };
                self.mark("passkey_cancelled");
                self.shot(ctx, "passkey_cancelled.png");
                self.next(100);
            }
            1558 if on_login(app) => {
                self.sign_in_as(app, "key@mock.test");
                self.next(300);
            }
            1559 if on_mfa(app) && app.login.mfa.kind == "webauthn" && self.press(app, "webauthn") => self.next(500),
            1560 if app.login.mfa.busy => {
                self.mark("mfa_key_ui_alive");
                self.next(100);
            }
            1561 => {
                self.signed_in(app, "mfa_key");
            }
            1562 if self.pending_shots == 0 => self.finish(ctx),
            _ => {}
        }
    }
}
