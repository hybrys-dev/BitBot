use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args.next().context("uso: bitbot-spike <cartella>")?;
    if args.next().is_some() {
        anyhow::bail!("uso: bitbot-spike <cartella>");
    }
    bitbot_drivers::run(path).await
}
