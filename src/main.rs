use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;

#[derive(Serialize, Deserialize, Debug)]
struct AddressConfig {
    address: String,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct UniverseMappingConfig {
    input: (u16, u16),
    output: (u16, u16),
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

fn read_config_file(file_path: &str) -> std::result::Result<Config, std::io::Error> {
    let mut file = File::open(file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let config: Config = serde_json::from_str(&contents)?;
    println!("{:?}", config);
    Ok(config)
}

// TODO: create Device mapping output threads

use artnet_protocol::*;
use std::net::{ToSocketAddrs, UdpSocket};

fn main() {
    let config = read_config_file("config.json").unwrap();

    {
        let socket =
            UdpSocket::bind(format!("{}:{}", config.listen.address, config.listen.port)).unwrap();

        let broadcast_addr = ("255.255.255.255", 6454)
            .to_socket_addrs()
            .unwrap()
            .next()
            .unwrap();
        socket.set_broadcast(true).unwrap();
        let buff = ArtCommand::Poll(Poll::default()).write_to_buffer().unwrap();
        socket.send_to(&buff, &broadcast_addr).unwrap();

        loop {
            let mut buffer = [0u8; 1024];
            let (length, addr) = socket.recv_from(&mut buffer).unwrap();
            let command = ArtCommand::from_buffer(&buffer[..length]).unwrap();

            println!("Received {:?}", command);

            match command {
                ArtCommand::Poll(poll) => {
                    // This will most likely be our own poll request, as this is broadcast to all devices on the network
                }
                ArtCommand::PollReply(reply) => {
                    // This is an ArtNet node on the network. We can send commands to it like this:
                    let command = ArtCommand::Output(Output {
                        data: vec![1, 2, 3, 4, 5].into(),
                        ..Output::default()
                    });
                    let bytes = command.write_to_buffer().unwrap();

                    socket.send_to(&bytes, &addr).unwrap();
                }
                _ => {}
            }
        }
    } // the socket is closed here
}
