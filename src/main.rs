use std::sync::mpsc;
use std::time::Duration;

use crate::client::{Client, ClientTrait};
use crate::server::{Server, ServerTrait};

// Do not modify the address constant
const DEFAULT_SERVER_ADDRESS: &'static str = "127.0.0.1";
const TIMEOUT: Duration = Duration::from_secs(1);
const DEFAULT_TOTAL_CLIENTS: usize = 50;
const DEFAULT_TOTAL_MESSAGES_PER_CLIENT: usize = 50;

#[tokio::main]
async fn main() {
    let (port, seed, total_clients, total_messages_per_client) = get_args();

    println!(
        "Starting the {} client(s), each client sending {} messages, with initial seed: {}",
        total_clients, total_messages_per_client, seed
    );

    let addr = format!("{}:{}", DEFAULT_SERVER_ADDRESS, port);

    let svr_addr = addr.clone();
    let (tx, rx) = mpsc::channel();
    // Start the server in another thread
    tokio::spawn(async move {
        Server.start_server(svr_addr, tx).await;
    });

    let client_addr = addr.clone();
    // Start a client in another thread, this is in charge of creating the clients and sending
    // the necessary messages/tasks
    let client = tokio::spawn(async move {
        match rx.recv_timeout(TIMEOUT) {
            Ok(Ok(_)) => {
                Client.start_client(seed, total_clients, total_messages_per_client, client_addr)
            }
            Ok(Err(e)) => {
                eprintln!("Server fails to start because: {}", e);
                return;
            }
            Err(e) => {
                eprintln!("Timeout: unable to get server status: {}", e);
                return;
            }
        }
    });

    // Block until the all clients finish
    client.await.unwrap();
}

fn get_args() -> (u16, u64, usize, usize) {
    let mut args = std::env::args().skip(1);

    (
        args.next()
            .map(|a| a.parse().expect("invalid port number"))
            .unwrap(),
        args.next()
            .map(|a| a.parse().expect("invalid u64 for seed"))
            .unwrap_or_else(|| rand::Rng::gen(&mut rand::thread_rng())),
        args.next()
            .map(|a| a.parse().expect("invalid usize for total clients"))
            .unwrap_or_else(|| DEFAULT_TOTAL_CLIENTS),
        args.next()
            .map(|a| {
                a.parse()
                    .expect("invalid usize for total messages per client")
            })
            .unwrap_or_else(|| DEFAULT_TOTAL_MESSAGES_PER_CLIENT),
    )
}

mod client;
mod server;
mod task;
