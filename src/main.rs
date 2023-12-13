use clap::{Arg, Command};
use net::{Address, Error, Listener};
use tokio::io::{self, AsyncReadExt, AsyncWriteExt, split};
mod net;

#[tokio::main]
async fn main() {
    let matches = Command::new("vsock-proxy")
        .about("A simple proxy to pipe traffic to/from a vsock connection")
        .arg(
            Arg::new("tcp-source")
                .long("tcp-source")
                .help("The tcp address for the proxy to listen on.")
                .conflicts_with("vsock-source")
                .required(false),
        )
        .arg(
            Arg::new("tcp-destination")
                .long("tcp-dest")
                .help("The tcp address for the proxy to forward to.")
                .conflicts_with("vsock-destination")
                .conflicts_with("tcp-source")
                .required(false),
        )
        .arg(
            Arg::new("vsock-source")
                .long("vsock-source")
                .help("The vsock address for the proxy to listen on.")
                .required(false),
        )
        .arg(
            Arg::new("vsock-destination")
                .long("vsock-dest")
                .help("The vsock address for the proxy to forward to.")
                .conflicts_with("vsock-source")
                .required(false),
        )
        .get_matches();

    let tcp_source = matches.get_one::<String>("tcp-source");
    let vsock_source = matches.get_one::<String>("vsock-source");

    if tcp_source.is_none() && vsock_source.is_none() {
        eprintln!("Error: no source address provided. Either tcp-source or vsock-source must be provided.");
        return;
    }

    let tcp_destination = matches.get_one::<String>("tcp-destination");
    let vsock_destination = matches.get_one::<String>("vsock-destination");

    if tcp_destination.is_none() && vsock_destination.is_none() {
        eprintln!("Error: no destination address provided. Either tcp-destination or vsock-destination must be provided.");
        return;
    }

    let parsed_source_address: Result<Address, Error> = tcp_source
        .map(|tcp_addr| Address::new_tcp_address(tcp_addr.as_str()))
        .or_else(|| vsock_source.map(|vsock_addr| Address::new_vsock_address(vsock_addr.as_str())))
        .expect("Infallible: either tcp or vsock source address must exist.");

    let source_address = match parsed_source_address {
        Ok(source_addr) => source_addr,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };

    let parsed_destination: Result<Address, Error> = tcp_destination
        .map(|tcp_addr| Address::new_tcp_address(tcp_addr))
        .or_else(|| vsock_destination.map(|vsock_addr| Address::new_vsock_address(vsock_addr)))
        .expect("Infallible: either tcp or vsock address must exist");

    let destination_address = match parsed_destination {
        Ok(dest_addr) => dest_addr,
        Err(e) => {
            eprintln!("Error: {e}");
            return;
        }
    };

    let mut source = match source_address.into_listener().await {
        Ok(source_conn) => source_conn,
        Err(e) => {
            eprintln!("Failed to create source connection - {e}");
            return;
        }
    };

    println!("Starting vsock-proxy");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(proxy_task(source_address, destination_address));

}

async fn proxy_task(source_address: Address, destination_address: Address) {
    let mut source = match source_address.into_listener().await {
        Ok(source_conn) => source_conn,
        Err(e) => {
            eprintln!("Failed to create source connection - {e}");
            return;
        }
    };

    loop {
        let mut accepted_conn = match source.accept().await {
            Ok(source_conn) => source_conn,
            Err(e) => {
                eprintln!("Failed to accept incoming connection - {e}");
                continue;
            }
        };

        let mut destination = match destination_address.get_destination_connection().await {
            Ok(dest_conn) => dest_conn,
            Err(e) => {
                eprintln!("Failed to create destination connection - {e}");
                continue;
            }
        };

        tokio::spawn(async move {
            let (mut client_reader, mut client_writer) = io::split(accepted_conn);
            let (mut destination_reader, mut destination_writer) = io::split(destination);

            let client_to_server = io::copy(&mut client_reader, &mut destination_writer);
            let server_to_client = io::copy(&mut destination_reader, &mut client_writer);

            match tokio::try_join!(client_to_server, server_to_client) {
                Ok(_) => (),
                Err(e) => eprintln!("Error during proxy operation: {e}"),
            }
        });
    }
}
