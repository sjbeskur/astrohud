pub struct Cli {
    pub endpoint: String,
    pub image_path: String,
}

impl Cli{
    pub fn parse_args() -> Cli {
        let args: Vec<String> = std::env::args().collect();

        if args.len() != 3 {
            eprintln!("Usage: {} <image_path>", args[0]);
            std::process::exit(1);
        }
           // ip_address:port
        
        Cli {
            endpoint: args[1].to_string(),        
            image_path:args[2].to_string(),
        }

    }
}