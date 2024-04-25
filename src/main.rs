use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct AddressConfig {
    address: String,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct UniverseMappingConfig {
    input: (u16,u16),
    output: (u16,u16)
}

#[derive(Serialize, Deserialize, Debug)]
struct DeviceMappingConfig {
    host: AddressConfig,
    universes: Vec<UniverseMappingConfig>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Config {
    listen: AddressConfig,
    mappings: Vec<DeviceMappingConfig>,
}

fn read_config_file(file_path: &str) -> Result<Config,std::io::Error> {
    let mut file = File::open(file_path)?;

    // Read the file content into a string
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Deserialize the JSON string into a Config struct
    let config: Config = serde_json::from_str(&contents)?;

    // Now you can use the config object as needed
    println!("{:?}", config);

    Ok(config)
}

// TODO: create Device mapping output threads

fn main() -> Result<(), Box<dyn std::error::Error>>  {

    let config = match read_config_file("config.json") {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error reading config file: {}", err);
            return Err(Box::new(err));
        }
    };

    {
        let socket = UdpSocket::bind(format!("{}:{}", config.listen.address, config.listen.port))?;

        // Receives a single datagram message on the socket. If `buf` is too small to hold
        // the message, it will be cut off.
        let mut buf = [0; 10];
        let (amt, src) = socket.recv_from(&mut buf)?;

        // Redeclare `buf` as slice of the received data and send reverse data back to origin.
        let buf = &mut buf[..amt];
        buf.reverse();
        socket.send_to(buf, &src)?;
    } // the socket is closed here

    Ok(())
}