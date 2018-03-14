Korhal Carrier is a decentralized edge access network.
=======================================================

Everything in this repository is work in progress and can not be stated as stable. You can loose your device in the void of the internet.

it's primary purpose is to establish a connection between a node (an IoT device) and a controller (such as a fleet management service)

entities:

- a node
- the ring, consisting of bearers
- a controller

# Running a bearer

First we need to build the docker container:
`docker build -t carrier-bearer .`

After building it, we can run the bearer:
```docker run --rm --name carrier \
       -v $(pwd)/test_certs/:/opt/carrier \
       -e CARRIER_CERT_PATH=/opt/carrier/server.cert.pem \
       -e CARRIER_KEY_PATH=/opt/carrier/server.key.pem \
       -e CARRIER_CLIENT_CA_PATH=/opt/carrier/trusted_client_certs/ \
       --net host \
       carrier-bearer
```

The bearer will listen by default on port `22222`. By defining the environment variable `CARRIER_LISTEN_PORT`,
the bearer can be instructed to listen on another port.

The bearer also requires a certificate/private key. In the example we take the certificate/private key that is
shipped for testing purposes in this repository. YOU SHOULD NEVER USE THAT IN PRODUCTION!

The peers are required to send a certificate that is signed by one of the certificate authorities given in `CARRIER_CLIENT_CA_PATH`
store. The certificate authorities in the store need to be encoded as `PEM` and named `*.pem`.

# Running a peer

Execute the following command:
```CARRIER_CERT_PATH=./test_certs/peer.cert.pem CARRIER_KEY_PATH=./test_certs/peer.key.pem \
   CARRIER_SERVER_CA_PATH=./test_certs/trusted_server_certs \
   CARRIER_CLIENT_CA_PATH=./test_certs/trusted_client_certs \
   CARRIER_SERVER_ADDR=SERVER_ADDR:SERVER_PORT cargo run --release --bin carrier-peer
```

As the bearer, the peer requires a certificate. Here applies the same as for the bearer, never use this certificate/private key
in production!

Carrier supports to create multiple services that can be executed over a Carrier connection. By default, a Carrier peer ships with
`lifeline`. `lifeline` is a service that provides a ssh connection (local running ssh server is required).

# Running lifeline

To test lifeline, you should add the following to your `~/.ssh/config`:
```
Host *.carrier
   StrictHostKeyChecking no
   ProxyCommand PATH_TO_LIFELINE/lifeline $(basename  %h .carrier) CARRIER_SERVER_ADDR:CARRIER_SERVER_PORT OWN_CERTIFICATE OWN_KEY PATH_TO_SERVER_CA PATH_TO_CLIENT_CA
```

The `PATH_TO_SERVER_CA` needs to contain the certificate authorities for connecting to the `carrier-bearer` and the certificate authorities of the
peers.

After you added the snippet to your ssh config, you can execute the following command:
`ssh BF0B90CF27036DA8B3170F4D86D9CC360398B5E9C3A9EB97E72FF57ADE48AB4B.carrier`

That should connect you to your peer with the given public key and give you a ssh connection :)

# License

GPLv3

For comercial licenses and SLAs contact sfx@korhal.io
