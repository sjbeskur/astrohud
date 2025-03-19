pub struct Cli {
    pub ip_address: String,
    pub port: u16,
}

impl Cli{
    pub fn parse_args() -> Cli {
        let args: Vec<String> = std::env::args().collect();

        if args.len() != 2 {
            eprintln!("Usage: {} <endpoint>", args[0]);
            std::process::exit(1);
        }
        let endpoint = args[1].to_string();
        let parts = endpoint.split(":").collect::<Vec<&str>>();
        let port = parts[1].parse::<u16>().unwrap();
        let ip = parts[0].to_string();

        Cli {
            ip_address: ip,
            port: port,
        }

    }
}