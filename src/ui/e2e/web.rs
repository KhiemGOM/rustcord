use super::Run;

const PLAN: &str = crate::webwin::NITRO_MONTHLY;

impl Run {
    /// A purchase opens the checkout window on its billing page, a second one moves it, Escape
    /// closes it and takes the temporary profile with it.
    pub(super) fn web_steps(&mut self) {
        let at = |path: &str| crate::webwin::shown().is_some_and(|u| u.ends_with(path));
        match self.step {
            900 => {
                crate::webwin::subscribe(PLAN, false);
                self.next(0);
            }
            901 => {
                if crate::webwin::shown().is_some_and(|u| u.ends_with("/robots.txt")) {
                    self.fail("the checkout window showed the sign-in step".into());
                }
                if at(&format!("/billing/premium/subscribe?plan_id={PLAN}&gift=false")) {
                    self.mark("web_open");
                    if crate::webwin::profiles_left() > 0 {
                        self.mark("web_profile_made");
                    }
                    crate::webwin::boost(1000);
                    self.next(0);
                }
            }
            902 => {
                if at("/billing/guild-subscriptions/purchase?guild_id=1000") {
                    self.mark("web_navigate");
                    crate::webwin::press_escape();
                    self.next(0);
                }
            }
            903 => {
                if !crate::webwin::is_open() {
                    self.mark("web_closed");
                    self.mark("web_escape");
                    self.next(0);
                }
            }
            _ => {
                if crate::webwin::profiles_left() == 0 {
                    self.mark("web_profile_gone");
                    self.step = 3200;
                } else if self.step_at.elapsed().as_secs() > 8 {
                    self.step = 3200;
                }
            }
        }
    }
}
