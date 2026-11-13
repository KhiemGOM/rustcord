use std::process::Command;

#[test]
fn no_closure_reaches_back_into_the_egui_context() {
    check("e2e/locks.py");
}

#[test]
fn every_put_moves_the_cursor_past_its_box() {
    check("e2e/puts.py");
}

fn check(script: &str) {
    let root = env!("CARGO_MANIFEST_DIR");
    let ran = ["python", "python3", "py"].iter().find_map(|py| Command::new(py).arg(script).current_dir(root).output().ok());
    let out = ran.unwrap_or_else(|| panic!("python is needed to run {script}"));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
