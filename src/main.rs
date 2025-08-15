use local_ip_address::local_ip;
use std::env::args;
use std::error::Error;
use std::net::IpAddr;
use std::str::FromStr;
use voice_chat::Client;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = args().collect();

    if args.len() <= 1 {
        println!("How to use:\nvoice-chat [MODE] [TARGET] (input device) (output device)");
        println!("MODE:    -s | --server    -> start a server/ listen for connections");
        println!("         -c | --client    -> connect as a client to a server");
        println!("TARGET:  if SERVER  -> Port to listen to (default: 8888)");
        println!("         if CLIENT  -> IP:Port to connect to (e.g. '192.168.121.2:8888')");
        println!("If input and/or output device are not specefied, the default will be used.");
        return Ok(());
    }
    println!(r" _   _       _          _____  _   _   ___ _____ ");
    println!(r"| | | |     (_)        /  __ \| | | | / _ \_   _|");
    println!(r"| | | | ___  _  ___ ___| /  \/| |_| |/ /_\ \| |  ");
    println!(r"| | | |/ _ \| |/ __/ _ \ |    |  _  ||  _  || |  ");
    println!(r"\ \_/ / (_) | | (_|  __/ \__/\| | | || | | || |  ");
    println!(r" \___/ \___/|_|\___\___|\____/\_| |_/\_| |_/\_/  ");

    match args[1].as_str() {
        "-s" | "--server" => {
            println!("Starting server...");
            let ip = local_ip().unwrap_or(IpAddr::from_str("127.0.0.1").unwrap());
            let port: String = args.get(2).unwrap_or(&String::from("8888")).to_string();
            let mut client = Client::new(format!("{ip}:{port}"))?;
            println!("Listening to {}...", client.address);
            smol::block_on(async { client.listen().await })?;
        }
        "-c" | "--client" => {
            println!("Starting client...");
            let address = args
                .get(2)
                .unwrap_or(&String::from("127.0.0.1:8888"))
                .to_string();
            let mut client = Client::new(address)?;
            println!("Trying to connect to {}...", client.address);
            smol::block_on(async { client.connect().await })?;
        }
        _ => {
            eprintln!("Invalid argument '{}'", args[1]);
        }
    }

    Ok(())
}
