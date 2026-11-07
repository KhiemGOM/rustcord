"""Checks for login_steps in src/ui/e2e/login.rs: the login run starts with no saved token and
signs in every way the mock offers (password, captcha, three MFA methods, a new location, a
re-activated account, register, forgot password, and the QR code through the remote-auth gateway)."""

import json


def body(r):
    try:
        return json.loads(r.get("body") or "{}")
    except ValueError:
        return {}


def checks(run, reqs):
    ok = lambda m: m in run.get("marks_ms", {})
    sent = lambda part: [body(r) for r in reqs if r.get("method") == "POST" and r.get("path", "").endswith(part)]
    logins = sent("/auth/login")
    mfa = lambda kind: sent("/auth/mfa/" + kind)
    registers = sent("/auth/register")
    return {
        "login: the run finished its script": run.get("ok") is True,
        "login: the card drew with Welcome back and the QR column": ok("login_screen"),
        "login: a wrong password put a red error on both fields": ok("login_field_error") and any(b.get("password") == "nope" for b in logins),
        "login: a captcha challenge opened the small window on the virtual discord.com page, the token went back and the login retried with it": ok("login_captcha_opened") and ok("login_captcha_solved")
            and any(r.get("path", "").startswith("/hcaptcha/render") and "sitekey=10000000-ffff-ffff-ffff-000000000001" in r.get("path", "") and "host=discord.com" in r.get("path", "") for r in reqs)
            and any(r.get("path", "").startswith("/hcaptcha/data") and "mock-rqdata" in r.get("path", "") for r in reqs)
            and any(b.get("login") == "captcha@mock.test" and b.get("captcha_key") == "e2e-captcha" for b in logins)
            and any(b.get("login") == "captcha@mock.test" and "captcha_key" not in b for b in logins),
        "login: a wrong authenticator code showed an error, the right one signed in": ok("mfa_shown") and ok("mfa_bad_code") and ok("mfa_totp")
            and [b.get("code") for b in mfa("totp")] == ["000000", "123456"],
        "login: the method switcher led to SMS, which sent a code and signed in": ok("mfa_switcher") and ok("mfa_sms_sent") and ok("mfa_sms")
            and len(sent("/auth/mfa/sms/send")) >= 1 and [b.get("code") for b in mfa("sms")] == ["654321"],
        "login: a backup code waited out its warning and signed in": ok("mfa_backup_warn") and ok("mfa_backup")
            and [b.get("code") for b in mfa("backup")] == ["abcd1234"],
        "login: a new location showed discord's message, and the retry signed in": ok("login_new_location") and ok("login_new_location_ok"),
        "login: a disabled account got its own screen": ok("login_disabled"),
        "login: a scheduled deletion could be re-activated": ok("login_deletion") and ok("login_reactivated")
            and any(b.get("login") == "deleted@mock.test" and b.get("undelete") is True for b in logins),
        "login: forgot password showed the server's error, then the sent screen": ok("forgot_error") and ok("forgot_sent")
            and [b.get("login") for b in sent("/auth/forgot")] == ["unknown@mock.test", "ok@mock.test"],
        "register: an empty form was stopped before the request and listed what it lacked": ok("register_required") and not any(b.get("email") == "" for b in registers),
        "register: the username was checked live, taken and then free": ok("register_name_taken") and ok("register_name_free")
            and {b.get("username") for b in sent("/unique-username/username-attempt-unauthed")} >= {"taken", "e2e_user"},
        "register: a server error landed on its field, the fixed form registered with a birthday and consent": ok("register_field_error") and ok("register_ok")
            and any(b.get("email") == "e2e@mock.test" and b.get("username") == "e2e_user" and b.get("date_of_birth") == "1999-05-17" and b.get("consent") is True for b in registers),
        "qr: the gateway handshake drew a code, showed who scanned it, and exchanged the ticket for a token": ok("qr_shown") and ok("qr_ticket") and ok("qr_login")
            and len(sent("/users/@me/remote-auth/login")) >= 1,
        "login: the passkey link ran the assertion off the UI thread, then signed in with the mock's ticket": ok("passkey_ui_alive") and ok("passkey_login")
            and any(b.get("ticket") == "passkey-ticket" and b.get("credential") == "mock-assertion" for b in sent("/auth/conditional/finish")),
        "login: a cancelled passkey prompt put its message on the login card and sent nothing": ok("passkey_cancelled") and len(sent("/auth/conditional/finish")) == 1,
        "login: the security key method kept the UI alive while it waited, then signed in": ok("mfa_key_ui_alive") and ok("mfa_key") and [b.get("code") for b in mfa("webauthn")] == ["mock-assertion"],
    }
