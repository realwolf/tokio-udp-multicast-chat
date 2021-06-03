/*
   https://github.com/henninglive/tokio-udp-multicast-chat/blob/master/Cargo.toml
   "tokio-udp-multicast-chat"

   tokio
   https://github.com/tokio-rs/tokio/pull/1630

   udp socket split is removed
   should use Arc
   https://docs.rs/tokio/1.6.1/tokio/net/struct.UdpSocket.html
*/

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use clap::{App, Arg};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;
use tokio::{signal, task};

const DEFAULT_USERNAME: &str = "Anonymous";
const DEFAULT_PORT: &str = "50692";
const DEFAULT_MULTICAST: &str = "239.255.42.98";
const IP_ALL: [u8; 4] = [0, 0, 0, 0];

/// Bind socket to multicast address with IP_MULTICAST_LOOP and SO_REUSEADDR Enabled
fn bind_multicast(
    addr: &SocketAddrV4,
    multi_addr: &SocketAddrV4,
) -> Result<std::net::UdpSocket, Box<dyn Error>> {
    use socket2::{Domain, Protocol, Socket, Type};

    assert!(multi_addr.ip().is_multicast(), "Must be multcast address");

    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    socket.set_reuse_address(true)?;
    socket.bind(&socket2::SockAddr::from(*addr))?;
    socket.set_multicast_loop_v4(true)?;
    socket.join_multicast_v4(multi_addr.ip(), addr.ip())?;

    Ok(socket.into())
}

/// Receive bytes from UPD socket and write to stdout until EOF.
async fn receive(rx: Arc<UdpSocket>) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buffer = vec![0u8; 4096];
    let mut stdout = tokio::io::stdout();

    loop {
        let n = rx.recv(&mut buffer[..]).await?;
        if n == 0 {
            break;
        }
        stdout.write_all(&mut buffer[..n]).await?;
    }

    Ok(())
}

/// Transmit bytes from stdin until EOF, Ctrl+D on linux or Ctrl+Z on windows.
async fn transmit(
    tx: Arc<UdpSocket>,
    addr: SocketAddr,
    mut username: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    username.push_str(": ");
    let mut buffer = username.into_bytes();
    let l = buffer.len();
    buffer.resize(4096, 0);

    let mut stdin = tokio::io::stdin();
    loop {
        let n = stdin.read(&mut buffer[l..]).await?;
        if n == 0 {
            break;
        }
        tx.send_to(&mut buffer[..l + n], &addr).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let matches = App::new("Udp Multicast Chat")
        .version("0.1.0")
        .author("Henning Ottesen <henning@live.no>")
        .about("Example UDP multicast CLI chat client using Tokio")
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .takes_value(true)
                .default_value(DEFAULT_PORT)
                .help("Sets UDP port number"),
        )
        .arg(
            Arg::with_name("ip")
                .short("i")
                .long("ip")
                .value_name("IP")
                .takes_value(true)
                .default_value(DEFAULT_MULTICAST)
                .help("Sets multicast IP"),
        )
        .arg(
            Arg::with_name("username")
                .short("u")
                .long("username")
                .value_name("NAME")
                .takes_value(true)
                .default_value(DEFAULT_USERNAME)
                .help("Sets username"),
        )
        .get_matches();

    let username = matches.value_of("username").unwrap().to_owned();

    let port = matches
        .value_of("port")
        .unwrap()
        .parse::<u16>()
        .expect("Invalid port number");

    let addr = SocketAddrV4::new(IP_ALL.into(), port);

    let multi_addr = SocketAddrV4::new(
        matches
            .value_of("ip")
            .unwrap()
            .parse::<Ipv4Addr>()
            .expect("Invalid IP"),
        port,
    );

    println!("Starting server on: {}", addr);
    println!("Multicast address: {}\n", multi_addr);

    let std_socket = bind_multicast(&addr, &multi_addr).expect("Failed to bind multicast socket");

    let socket = UdpSocket::from_std(std_socket).unwrap();
    let r = Arc::new(socket);
    let s = r.clone();

    tokio::select! {
        res = task::spawn(async move { receive(r).await }) => {
            res.map_err(|e| e.into()).and_then(|e| e)
        },
        res = task::spawn(async move { transmit(s, multi_addr.into(), username).await }) => {
            res.map_err(|e| e.into()).and_then(|e| e)
        },
        // You have to press Enter after pressing Ctrl+C for the program to terminate.
        // https://docs.rs/tokio/0.2.21/tokio/io/fn.stdin.html
        res = signal::ctrl_c() => {
            res.map_err(|e| e.into())
        }
    }
}
