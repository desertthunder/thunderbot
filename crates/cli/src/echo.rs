use owo_colors::OwoColorize;

pub fn success(msg: &str) {
    println!("{} {}", "✓".green(), msg);
}

pub fn error(msg: &str) {
    eprintln!("{} {}", "✗".red(), msg);
}

pub fn info(msg: &str) {
    println!("{} {}", "ℹ".blue(), msg);
}

pub fn warn(msg: &str) {
    eprintln!("{} {}", "⚠".yellow(), msg);
}

pub fn header(msg: &str) {
    println!("\n{}\n", msg.bold().cyan());
}

pub fn ok(msg: &str) -> anyhow::Result<()> {
    success(msg);
    Ok(())
}
