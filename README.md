# Art-Net Hub

Art-Net proxy server, which listens art-net packages and re-routes universes to other
hosts with rate limiter and optional custom protocols for delivering universes to final device.

With custom protocol ESP32 can for example read all universes of frame in single UPD
packet.

## Getting started

    cargo build
    cargo run

## Configuration

    {
        "listen": {
            "address": "0.0.0.0",
            "port": 6454
        },
        "mappings": [
            {
            "host": { "address": "192.168.0.11", "port": 6454 },
            "//": "Mapping which universes will be realayed here and what how they are mapped",
            "universes": [{ "input": [16, 31], "output": [0, 15] }]
            }
        ]
    }

## TODO:

- Art-Net server protocol to receive artnet data and broadcast available devices
- UDP threads for each device to pass universes to final controllers.
-
