use df_test_protocol::PROTOCOL_VERSION;

fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "help".into());
    match command.as_str() {
        "doctor" => doctor(),
        "--version" | "-V" | "version" => {
            println!("DragonForge Test Lab {}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            println!("DragonForge Test Lab");
            println!("Usage: dragonforge-test-lab <doctor|version>");
        }
    }
}

fn doctor() {
    println!("DragonForge Test Lab doctor");
    println!("version={}", env!("CARGO_PKG_VERSION"));
    println!("protocol_version={PROTOCOL_VERSION}");
    println!("os={}", std::env::consts::OS);
    println!("arch={}", std::env::consts::ARCH);
    println!("phase=0");
    println!("status=foundation_ready");
}
