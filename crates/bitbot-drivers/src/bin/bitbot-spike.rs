#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let invocation = bitbot_drivers::parse_invocation(std::env::args().skip(1))?;
    bitbot_drivers::run(
        &invocation.agent,
        &invocation.cwd,
        &invocation.task,
        invocation.resume,
    )
    .await
}
