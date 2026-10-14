use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .context("uso: bitbot-spike <cartella> <compito>")?;
    let task = args.collect::<Vec<_>>().join(" ");
    if task.trim().is_empty() {
        anyhow::bail!("uso: bitbot-spike <cartella> <compito>");
    }
    bitbot_drivers::run(path, &task).await
}
