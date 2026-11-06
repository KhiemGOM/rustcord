use std::sync::Arc;

use super::App;
use crate::net::Tag;

type Undo = Arc<dyn Fn(&mut App) + Send + Sync>;

/// What to do when a request the user made comes back: the toast for each way it can end, and
/// how to take back what was changed on screen before it was known to work.
#[derive(Clone, Default)]
pub struct Act {
    say: Option<String>,
    ok: Option<String>,
    undo: Option<Undo>,
}

impl std::fmt::Debug for Act {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Act({:?})", self.say)
    }
}

impl PartialEq for Act {
    fn eq(&self, o: &Act) -> bool {
        self.say == o.say && self.ok == o.ok
    }
}

impl Tag {
    /// Failure shows `say`; discord's own words for the action, or the API's message when it has none.
    pub fn fail(say: impl Into<String>) -> Tag {
        Tag::Act(Act { say: Some(say.into()), ..Default::default() })
    }

    /// Failure shows the API's message.
    pub fn reason() -> Tag {
        Tag::Act(Act::default())
    }

    /// Failure shows `say` and runs `undo`, which puts back what the click had already changed.
    pub fn undo(say: impl Into<String>, undo: impl Fn(&mut App) + Send + Sync + 'static) -> Tag {
        Tag::Act(Act { say: Some(say.into()), undo: Some(Arc::new(undo)), ..Default::default() })
    }

    /// The same, with no words of discord's for the failure.
    pub fn rollback(undo: impl Fn(&mut App) + Send + Sync + 'static) -> Tag {
        Tag::Act(Act { undo: Some(Arc::new(undo)), ..Default::default() })
    }

    /// Success says `ok`; failure shows the API's message.
    pub fn ok(ok: impl Into<String>) -> Tag {
        Tag::Act(Act { ok: Some(ok.into()), ..Default::default() })
    }

    /// Success and failure each have a toast, as for a kick or a ban.
    pub fn done(ok: impl Into<String>, say: impl Into<String>) -> Tag {
        Tag::Act(Act { say: Some(say.into()), ok: Some(ok.into()), undo: None })
    }
}

impl App {
    pub fn acted(&mut self, a: &Act, res: Result<(), String>) {
        match res {
            Ok(()) => {
                if let Some(t) = &a.ok {
                    self.toast(t.clone());
                }
            }
            Err(e) => {
                if let Some(u) = &a.undo {
                    u(self);
                }
                match &a.say {
                    Some(s) => {
                        self.toast(s.clone());
                        self.toast_detail = Some(e);
                    }
                    None => self.toast(e),
                }
            }
        }
    }
}

/// Undo for a server settings page that patched its own copy of the data: drop it so the page asks again.
pub fn stale(key: &'static str) -> impl Fn(&mut App) + Send + Sync + 'static {
    move |app| {
        if let Some(s) = &mut app.server {
            s.data.remove(key);
        }
    }
}
