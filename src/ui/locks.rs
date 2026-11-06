use std::process::Command;

#[test]
fn no_closure_reaches_back_into_the_egui_context() {
    let root = env!("CARGO_MANIFEST_DIR");
    let ran = ["python", "python3", "py"].iter().find_map(|py| Command::new(py).arg("e2e/locks.py").current_dir(root).output().ok());
    let out = ran.expect("python is needed to run e2e/locks.py");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
