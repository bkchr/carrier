Korhal Carrier is a decentralized edge access network.
=======================================================

Everything in this repository is work in progress and can not be stated as stable. You can loose your device in the void of the internet.

it's primary purpose is to establish a connection between a node (an IoT device) and a controller (such as a fleet management service)

entities:

- a node
- the ring, consisting of bearers
- a controller

# Running the server

First we need to build the docker container:
`docker build -t carrier-server .`

After building it, we can run the server:
```docker run --rm --name carrier \
       -v $(pwd)/src/bin/:/opt/carrier \
       -e CARRIER_CERT_PATH=/opt/carrier/cert.pem \
       -e CARRIER_KEY_PATH=/opt/carrier/key.pem \
       -e CARRIER_TRUSTED_CLIENT_CERTS_PATH=/opt/carrier/client_certs/ \
       -net host \
       carrier-server
```

The server will listen by default on port `22222`. By defining the environment variable `CARRIER_LISTEN_PORT`,
the server can be instructed to listen on another port.

The server also requires a certificate/private key. In the example we take the certificate/private key that is
shipped for testing purposes in this repository. YOU SHOULD NEVER USE THAT IN PRODUCTION!

The peers are required to send a certificate that is signed by one of the certificates given in the `CARRIER_TRUSTED_CLIENT_CERTS_PATH`
store. The certificates in the store need to be encoded as `PEM` and named `*.pem`.

# Running a peer

Execute the following command:
`CARRIER_CERT_PATH=./src/bin/cert.pem CARRIER_KEY_PATH=./src/bin/key.pem CARRIER_SERVER_ADDR=SERVER_ADDR:SERVER_PORT CARRIER_PEER_NAME=peer cargo run --release --bin carrier-peer`

As the server, the peer requires a certificate. Here applies the same as for the server, never use this certificate/private key
in production!

The environment variable `CARRIER_PEER_NAME` sets the name of this peer. The name is required to talk to this device over Carrier.

Carrier supports to create multiple services that can be executed over a Carrier connection. By default, a Carrier peer ships with
`lifeline`. `lifeline` is a service that provides a ssh connection (local running ssh server is required).

# Running lifeline

To test lifeline, you should add the following to your `~/.ssh/config`:
```
Host *.carrier
   StrictHostKeyChecking no
   ProxyCommand PATH_TO_LIFELINE/lifeline $(basename  %h .carrier) CARRIER_SERVER_ADDR:CARRIER_SERVER_PORT
```

After you added the snippet to your ssh config, you can execute the following command:
`ssh peer.carrier` 

That should connect you to your peer named "peer" and give you a ssh connection :)

# License

GPLv3

For comercial licenses and SLAs contact sfx@korhal.io
