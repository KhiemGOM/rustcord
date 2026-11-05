use super::Run;

impl Run {
    /// A purchase opens the checkout window on the page, a second one moves it, and it closes.
    pub(super) fn web_steps(&mut self) {
        let at = |path: &str| crate::webwin::shown().is_some_and(|u| u.ends_with(path));
        match self.step {
            900 => {
                crate::webwin::checkout("/store");
                self.next(0);
            }
            901 => {
                if at("/store") {
                    self.mark("web_open");
                    crate::webwin::checkout("/shop");
                    self.next(0);
                }
            }
            902 => {
                if at("/shop") {
                    self.mark("web_navigate");
                    crate::webwin::close();
                    self.next(0);
                }
            }
            _ => {
                if !crate::webwin::is_open() {
                    self.mark("web_closed");
                    self.step = 100;
                }
            }
        }
    }
}
