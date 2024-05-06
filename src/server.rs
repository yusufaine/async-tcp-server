use std::collections::HashMap;
use std::error::Error;
use std::sync::{mpsc, Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, Semaphore};

use crate::task::{Task, TaskType};

pub trait ServerTrait {
    async fn start_server(
        &self,
        address: String,
        tx: mpsc::Sender<Result<(), Box<dyn Error + Send>>>,
    );
}

pub struct Server;

impl ServerTrait for Server {
    async fn start_server(
        &self,
        address: String,
        tx: mpsc::Sender<Result<(), Box<dyn Error + Send>>>,
    ) {
        println!("Starting the server");
        let listener = TcpListener::bind(address).await;

        match listener {
            // Listener successfully bounded to the address
            Ok(_) => tx.send(Ok(())).unwrap(),

            // Error binding to the address
            Err(e) => {
                println!("here {}", e);
                tx.send(Err(Box::new(e))).unwrap();
                return;
            }
        }

        let listener = listener.unwrap();
        let cache: Arc<RwLock<HashMap<String, Option<u8>>>> = Arc::new(RwLock::new(HashMap::new()));
        let task_semaphore = Arc::new(Semaphore::new(40));

        // Iterate over incoming connections, each stream, which represents a client, is currently
        // handled by the same thread
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let cache = cache.clone();
                    let task_semaphore = task_semaphore.clone();
                    tokio::spawn(async move {
                        Self::handle_connection(cache, task_semaphore, stream).await;
                    });
                }
                Err(e) => {
                    eprintln!("Unable to accept connection due to: {}", e);
                    return;
                }
            }
        }
    }
}

impl Server {
    async fn handle_connection(
        cache: Arc<RwLock<HashMap<String, Option<u8>>>>,
        task_semaphore: Arc<Semaphore>,
        stream: TcpStream,
    ) {
        use tokio::sync::mpsc;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (r_stream, mut w_stream) = stream.into_split();

        let rx_handler = tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    Some(r) => w_stream.write_u8(r).await.unwrap(),
                    None => return, // exit if the receiver is closed
                }
            }
        });

        let mut buf_reader = BufReader::new(r_stream);
        loop {
            let mut buf = String::new();
            match buf_reader.read_line(&mut buf).await {
                Ok(0) => {
                    // Drop the sender to close the receiver
                    drop(tx);
                    break;
                }
                Ok(_) => {
                    // check if cache contains value for the key
                    if let Some(value) = cache.read().await.get(&buf) {
                        tx.send(*value).unwrap();
                        continue;
                    }
                    let cache = cache.clone();
                    let task_semaphore = task_semaphore.clone();
                    let tx = tx.clone();

                    // Spawn in a separate thread to handle multiple requests concurrently
                    // Assumes that the loop will not be blocked by a single request
                    tokio::spawn(async move {
                        let k = buf.clone();
                        let result = Self::get_task_value(task_semaphore, buf).await;
                        cache.write().await.insert(k, result);
                        tx.send(result).unwrap();
                        return;
                    });
                }
                Err(e) => {
                    eprintln!("Unable to get command due to: {}", e);
                    return;
                }
            }
        }

        // Wait for the receiver to finish writing
        rx_handler.await.unwrap();
    }

    async fn get_task_value(task_semaphore: Arc<Semaphore>, buf: String) -> Option<u8> {
        let try_parse = || async {
            let numbers: Vec<&str> = buf.trim().split(':').collect();
            let task_type = numbers.first().unwrap().parse::<u8>()?;
            let seed = numbers.last().unwrap().parse::<u64>()?;

            match TaskType::from_u8(task_type) {
                Some(parsed_task) => {
                    return Ok::<(u8, u64, TaskType), Box<dyn std::error::Error>>((
                        task_type,
                        seed,
                        parsed_task,
                    ));
                }
                None => return Err("Invalid task type".into()),
            }
        };

        let (task_type, seed, parsed_task) = match try_parse().await {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Unable to parse task due to: {}", e);
                return None;
            }
        };

        match parsed_task {
            TaskType::CpuIntensiveTask => {
                let _permit = task_semaphore.acquire().await.unwrap();
                return Some(Task::execute_async(task_type, seed).await);
            }
            _ => return Some(Task::execute_async(task_type, seed).await),
        }
    }
}
