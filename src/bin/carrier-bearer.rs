extern crate carrier;
extern crate structopt;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::path::PathBuf;

use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "carrier-bearer")]
struct Options {
    /// The path to the certificate of this bearer.
    #[structopt(long = "certificate", parse(from_os_str))]
    certificate: PathBuf,
    /// The path to the private key of this bearer.
    #[structopt(long = "private_key", parse(from_os_str))]
    private_key: PathBuf,
    /// The port this bearer should listen on.
    #[structopt(long = "listen_port", default_value = "22222")]
    listen_port: u16,
    /// The path to trusted authorities for incoming connections in PEM format(filename: *.pem).
    #[structopt(long = "incoming_con_ca_path", parse(from_os_str))]
    incoming_con_ca_path: PathBuf,
}

fn main() {
    let options = Options::from_args();

    let incoming_con_ca_vec =
        carrier::util::glob_for_certificates(&options.incoming_con_ca_path.display())
            .expect("Globbing for incoming connection certificate authorities(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let builder = carrier::Peer::builder(evt_loop.handle())
        .set_quic_listen_port(options.listen_port)
        .set_certificate_chain_file(options.certificate)
        .set_private_key_file(options.private_key)
        .set_client_ca_cert_files(incoming_con_ca_vec);

    let builder = carrier::builtin_services::register(builder);

    println!("Bearer running (Port: {})", options.listen_port);
    let bearer = builder.build().unwrap();

    bearer.run(&mut evt_loop).unwrap();
}
