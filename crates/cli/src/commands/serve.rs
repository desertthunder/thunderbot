use owo_colors::OwoColorize;
use tnbot_core::Settings;

pub async fn run(settings: &Settings, dry_run: bool) -> anyhow::Result<()> {
    if dry_run {
        println!("{}", "Running in dry-run mode (no posts will be made)".yellow());
    }

    println!("{}", "Starting daemon...".green().bold());
    super::jetstream::listen(None, settings.bot.did.clone(), None).await
}
