fn main() {
    println!("cargo:rerun-if-changed=assets/discord.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/discord.ico");
        res.compile().expect("couldn't embed the icon");
    }
}
