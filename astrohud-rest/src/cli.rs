use std::net::SocketAddr;

pub struct Cli {
    pub endpoint: SocketAddr,
}

impl Cli {
    pub fn parse_args() -> Cli {
        let args: Vec<String> = std::env::args().collect();

        let endpoint = match args.as_slice() {
            [_, endpoint] => endpoint.clone(),
            [_] => match std::env::var("PORT") {
                Ok(port) => format!("0.0.0.0:{port}"),
                Err(_) => {
                    eprintln!("Usage: {} <endpoint> (or set PORT)", args[0]);
                    std::process::exit(1);
                }
            },
            _ => {
                eprintln!("Usage: {} <endpoint> (or set PORT)", args[0]);
                std::process::exit(1);
            }
        };

        let endpoint = endpoint.parse::<SocketAddr>().unwrap_or_else(|error| {
            eprintln!("Invalid server endpoint {endpoint:?}: {error}");
            std::process::exit(1);
        });

        Cli { endpoint }
    }
}
