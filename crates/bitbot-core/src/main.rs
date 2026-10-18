//! Binario `bitbotd`: apre l'endpoint predefinito e serve finché non
//! arriva l'interruzione.

use bitbot_core::{serve, socket::SocketEndpoint};

fn main() {
    let endpoint = SocketEndpoint::default_endpoint();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime di tokio");
    let result = runtime.block_on(async {
        let listener = match bitbot_core::listen(&endpoint) {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("bitbotd: impossibile aprire {}: {err}", endpoint.display());
                std::process::exit(1);
            }
        };
        println!("bitbotd in ascolto su {}", endpoint.display());
        tokio::select! {
            result = serve(listener) => result,
            _ = tokio::signal::ctrl_c() => Ok(()),
        }
    });
    if let Err(err) = result {
        eprintln!("bitbotd: {err}");
        std::process::exit(1);
    }
}
