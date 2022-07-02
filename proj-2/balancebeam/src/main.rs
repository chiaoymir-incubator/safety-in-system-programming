mod request;
mod response;

use clap::Parser;
use rand::{Rng, SeedableRng};
use tokio::net::{TcpListener, TcpStream};
use tokio::{stream::StreamExt};
use tokio::sync::Mutex;
use std::time::Duration;
use tokio::time::delay_for;
use async_std::sync::Arc;
use std::io::{Error, ErrorKind};
use std::collections::HashMap;

/// Contains information parsed from the command-line invocation of balancebeam. The Clap macros
/// provide a fancy way to automatically construct a command-line argument parser.
#[derive(Parser, Debug)]
#[clap(about = "Fun with load balancing")]
struct CmdOptions {
    #[clap(short, long, default_value = "0.0.0.0:1100")]
    /// IP/port to bind to
    bind: String,

    #[clap(short, long)]
    /// Upstream host to forward requests to
    upstream: Vec<String>,

    #[clap(long, default_value = "10")]
    /// Perform active health checks on this interval (in seconds)
    active_health_check_interval: usize,

    #[clap(long, default_value = "/")]
    /// Path to send request to for active health checks
    active_health_check_path: String,

    #[clap(long, default_value = "0")]
    /// Maximum number of requests to accept per IP per minute (0 = unlimited)
    max_requests_per_minute: usize,
}

/// Contains information about the state of balancebeam (e.g. what servers we are currently proxying
/// to, what servers have failed, rate limiting counts, etc.)
///
/// You should add fields to this struct in later milestones.
#[derive(Clone)]
struct ProxyState {
    /// How frequently we check whether upstream servers are alive (Milestone 4)
    #[allow(dead_code)]
    active_health_check_interval: usize,
    /// Where we should send requests when doing active health checks (Milestone 4)
    #[allow(dead_code)]
    active_health_check_path: String,
    /// Maximum number of requests an individual IP can make in a minute (Milestone 5)
    #[allow(dead_code)]
    max_requests_per_minute: usize,
    /// Addresses of servers that we are proxying to
    upstream_addresses: Arc<Mutex<Vec<String>>>,
    /// Addresses of servers that are not available
    dead_upstream_addresses: Arc<Mutex<Vec<String>>>,
    /// Count of attemps per window
    count_map: Arc<Mutex<HashMap<String, usize>>>,
}

#[tokio::main]
async fn main() {
    // Initialize the logging library. You can print log messages using the `log` macros:
    // https://docs.rs/log/0.4.8/log/ You are welcome to continue using print! statements; this
    // just looks a little prettier.
    if let Err(_) = std::env::var("RUST_LOG") {
        std::env::set_var("RUST_LOG", "debug");
    }
    pretty_env_logger::init();

    // Parse the command line arguments passed to this program
    let options = CmdOptions::parse();
    if options.upstream.len() < 1 {
        log::error!("At least one upstream server must be specified using the --upstream option.");
        std::process::exit(1);
    }

    // Start listening for connections
    let mut listener = match TcpListener::bind(&options.bind).await {
        Ok(listener) => listener,
        Err(err) => {
            log::error!("Could not bind to {}: {}", options.bind, err);
            std::process::exit(1);
        }
    };
    log::info!("Listening for requests on {}", options.bind);

    // Handle incoming connections
    let state = ProxyState {
        upstream_addresses: Arc::new(Mutex::new(options.upstream)),
        dead_upstream_addresses: Arc::new(Mutex::new(Vec::new())),
        active_health_check_interval: options.active_health_check_interval,
        active_health_check_path: options.active_health_check_path,
        max_requests_per_minute: options.max_requests_per_minute,
        count_map: Arc::new(Mutex::new(HashMap::new())),
    };

    let state_copy = state.clone();
    tokio::spawn(async move {
        loop {
            delay_for(Duration::from_secs(state_copy.active_health_check_interval as u64)).await;
            perform_health_check(&state_copy).await;
        }
    });

    let state_copy = state.clone();
    tokio::spawn(async move {
        loop {
            delay_for(Duration::from_secs(60)).await;
            rate_limiting_refresh(&state_copy).await;
        }
    });

    while let Some(stream) = listener.next().await {
        match stream {
            Ok(mut stream) => {
                // We could short-circuit the process if the client runs out of budget
                if state.max_requests_per_minute > 0 {
                    let state_copy = state.clone();
                    {
                        let mut count_map = state_copy.count_map.lock().await;
                        let ip_addr = stream.peer_addr().unwrap().ip().to_string();
                        if !count_map.contains_key(&ip_addr) {
                            count_map.insert(ip_addr.clone(), 1);
                        } else {
                            if count_map[&ip_addr] >= state_copy.max_requests_per_minute {
                                let response = response::make_http_error(http::StatusCode::TOO_MANY_REQUESTS);
                                response::write_to_stream(&response, &mut stream).await.unwrap();
                                continue;
                            } else {
                                *count_map.get_mut(&ip_addr).unwrap() += 1;
                            }
                        }
                    }
                }
                let state_copy = state.clone();
                // Handle the connection!
                tokio::spawn(async move {
                    handle_connection(stream, &state_copy).await;
                });
            },
            Err(_) => { break; } 
        }
    }

    println!("some error wwwå");
}

async fn connect_to_upstream(state: &ProxyState) -> Result<TcpStream, std::io::Error> {
    loop {
        // connect to random upstream
        let mut rng = rand::rngs::StdRng::from_entropy(); 
        let mut upstream_addresses = state.upstream_addresses.lock().await;
        if upstream_addresses.len() == 0 {
            return Err(Error::new(ErrorKind::Other, "empty upstream available"));
        }
        let upstream_idx = rng.gen_range(0, upstream_addresses.len());
        let upstream_ip = &upstream_addresses[upstream_idx];
        match TcpStream::connect(upstream_ip).await {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                log::error!("Failed to connect to upstream {}: {}", upstream_ip, err);
            },
        }

        // TODO: implement failover (milestone 3)
        // update dead upstream addresses and remove it from upstream addresses
        let mut dead_upstream_addresses = state.dead_upstream_addresses.lock().await;
        // log::error!("{:?}, {:?}", dead_upstream_addresses, upstream_addresses);
        let addr = upstream_addresses[upstream_idx].clone();
        dead_upstream_addresses.push(addr);
        upstream_addresses.remove(upstream_idx);
    }
}

async fn send_response(client_conn: &mut TcpStream, response: &http::Response<Vec<u8>>) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("{} <- {}", client_ip, response::format_response_line(&response));
    if let Err(error) = response::write_to_stream(&response, client_conn).await {
        log::warn!("Failed to send response to client: {}", error);
        return;
    }
}

async fn handle_connection(mut client_conn: TcpStream, state: &ProxyState) {
    let client_ip = client_conn.peer_addr().unwrap().ip().to_string();
    log::info!("Connection received from {}", client_ip);

    // Open a connection to a random destination server
    let mut upstream_conn = match connect_to_upstream(state).await {
        Ok(stream) => stream,
        Err(_error) => {
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
    };
    let upstream_ip = upstream_conn.peer_addr().unwrap().ip().to_string();

    // The client may now send us one or more requests. Keep trying to read requests until the
    // client hangs up or we get an error.
    loop {
        // Read a request from the client
        let mut request = match request::read_from_stream(&mut client_conn).await {
            Ok(request) => request,
            // Handle case where client closed connection and is no longer sending requests
            Err(request::Error::IncompleteRequest(0)) => {
                log::debug!("Client finished sending requests. Shutting down connection");
                return;
            }
            // Handle I/O error in reading from the client
            Err(request::Error::ConnectionError(io_err)) => {
                log::info!("Error reading request from client stream: {}", io_err);
                return;
            }
            Err(error) => {
                log::debug!("Error parsing request: {:?}", error);
                let response = response::make_http_error(match error {
                    request::Error::IncompleteRequest(_)
                    | request::Error::MalformedRequest(_)
                    | request::Error::InvalidContentLength
                    | request::Error::ContentLengthMismatch => http::StatusCode::BAD_REQUEST,
                    request::Error::RequestBodyTooLarge => http::StatusCode::PAYLOAD_TOO_LARGE,
                    request::Error::ConnectionError(_) => http::StatusCode::SERVICE_UNAVAILABLE,
                });
                send_response(&mut client_conn, &response).await;
                continue;
            }
        };
        log::info!(
            "{} -> {}: {}",
            client_ip,
            upstream_ip,
            request::format_request_line(&request)
        );

        // Add X-Forwarded-For header so that the upstream server knows the client's IP address.
        // (We're the ones connecting directly to the upstream server, so without this header, the
        // upstream server will only know our IP, not the client's.)
        request::extend_header_value(&mut request, "x-forwarded-for", &client_ip);

        // Forward the request to the server
        if let Err(error) = request::write_to_stream(&request, &mut upstream_conn).await {
            log::error!("Failed to send request to upstream {}: {}", upstream_ip, error);
            let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
            send_response(&mut client_conn, &response).await;
            return;
        }
        log::debug!("Forwarded request to server");

        // Read the server's response
        let response = match response::read_from_stream(&mut upstream_conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                let response = response::make_http_error(http::StatusCode::BAD_GATEWAY);
                send_response(&mut client_conn, &response).await;
                return;
            }
        };
        // Forward the response to the client
        send_response(&mut client_conn, &response).await;
        log::debug!("Forwarded response to client");
    }
}

async fn perform_health_check(state: &ProxyState) {
    let mut dead_upstream_addresses = state.dead_upstream_addresses.lock().await;
    let mut upstream_addresses = state.upstream_addresses.lock().await;
    // merge two vector into 1
    dead_upstream_addresses.append(&mut upstream_addresses);
    upstream_addresses.clear();
    let mut new_dead_addresses = Vec::new();

    for addr in dead_upstream_addresses.iter() {
        // try to connect to a upstream server
        let mut conn = match TcpStream::connect(addr).await {
            Ok(stream) => stream,
            Err(err) => {
                log::error!("Failed to connect to upstream {}: {}", addr, err);
                new_dead_addresses.push(true);
                continue;
            },
        };

        // build a request to health check path
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(state.active_health_check_path.clone())
            .header("Host", addr)
            .body(Vec::<u8>::new())
            .unwrap();

        if let Err(error) = request::write_to_stream(&request, &mut conn).await {
            log::error!("Failed to send request to upstream {}: {}", addr, error);
            new_dead_addresses.push(true);
            continue;
        }

        // Read the server's response
        let response = match response::read_from_stream(&mut conn, request.method()).await {
            Ok(response) => response,
            Err(error) => {
                log::error!("Error reading response from server: {:?}", error);
                new_dead_addresses.push(true);
                return;
            }
        };

        // the response is errorneous
        if response.status().as_u16() > 400 {
            new_dead_addresses.push(true);
        } else {
            new_dead_addresses.push(false);
            upstream_addresses.push(addr.to_string());
        }
    }

    let mut iter = new_dead_addresses.iter();
    dead_upstream_addresses.retain(|_| *iter.next().unwrap());
    println!("{:?}, {:?}", upstream_addresses, dead_upstream_addresses);
}

async fn rate_limiting_refresh(state: &ProxyState) {
    let mut count_map = state.count_map.lock().await;
    count_map.clear();
}
