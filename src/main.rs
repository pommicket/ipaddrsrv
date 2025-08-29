pub mod argparse;

use std::collections::HashMap;
use std::io::{self, prelude::*};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};
use std::thread::sleep;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Flag {
	Help,
	Version,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Param {
	Port,
	Address,
	Timeout,
	MaxConnections,
}

fn print_version() {
	println!(
		"{} v. {}",
		env!("CARGO_PKG_NAME"),
		env!("CARGO_PKG_VERSION")
	);
}

fn print_doc() {
	print_version();
	println!("{}", include_str!("../README.txt"));
}

#[derive(Debug)]
struct ConnectionSettings {
	timeout: u32,
	buffer: [u8; 1024],
}

const SLEEP_MS_PER_ROUND: u32 = 50;

impl ConnectionSettings {
	/// handle this connection as much as possible *without blocking*
	#[must_use]
	fn handle_connection(&mut self, addr: IpAddr, conn: &mut Connection) -> bool {
		let buffer = &mut self.buffer;
		conn.age_ms += SLEEP_MS_PER_ROUND;
		if conn.age_ms > self.timeout * 1000 {
			return false;
		}
		if conn.state < 4 {
			let n = match conn.stream.read(buffer) {
				Ok(n) => n,
				Err(e) if e.kind() == io::ErrorKind::WouldBlock => 0,
				Err(e) => {
					eprintln!("WARNING: error reading from connection: {e}");
					return false;
				}
			};
			for &c in &buffer[..n] {
				if c != b'\r' && c != b'\n' {
					continue;
				}
				let expected = if conn.state % 2 == 0 { b'\r' } else { b'\n' };
				if c == expected {
					conn.state += 1;
					if conn.state == 4 {
						break;
					}
				}
			}
		}
		if conn.state == 4 {
			// 128B is more than long enough for our response
			write!(
				&mut buffer[..128],
				"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\n{addr}\n\0"
			)
			.expect("?? writing address to buffer failed??");
			let length = buffer[..128]
				.iter()
				.position(|&c| c == 0)
				.expect("?? no null byte even though we just wrote one??");
			match conn
				.stream
				.write(&buffer[usize::from(conn.written)..length])
			{
				Ok(n) => {
					conn.written += n as u8;
					if conn.written >= length as u8 {
						// hooray! done sending response
						return false;
					}
				}
				Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
					// keep connection around
				}
				Err(e) => {
					eprintln!("WARNING: error writing to connection: {e}");
					return false;
				}
			}
		}
		true
	}
}

#[derive(Debug)]
struct Connection {
	stream: TcpStream,
	/// rough time connection has been around in milliseconds
	age_ms: u32,
	/// number of characters out of \r\n\r\n (end of HTTP headers) that we have
	state: u8,
	/// number of characters of address which have been written
	written: u8,
}

fn try_main() -> Result<(), Box<dyn std::error::Error>> {
	let args = argparse::parse_args(
		&HashMap::from_iter([("--help", Flag::Help), ("--version", Flag::Version)]),
		&HashMap::from([
			("--port", Param::Port),
			("--address", Param::Address),
			("--timeout", Param::Timeout),
			("--max-connections", Param::MaxConnections),
		]),
	)?;
	if args.is_set(Flag::Help) {
		print_doc();
		return Ok(());
	}
	if args.is_set(Flag::Version) {
		print_version();
		return Ok(());
	}
	let addr = args.get(Param::Address).unwrap_or("0.0.0.0");
	let port = args.get(Param::Port).unwrap_or("80");
	let port = port
		.parse::<u16>()
		.map_err(|_| format!("Invalid port: {port}"))?;
	let timeout = args.get(Param::Timeout).unwrap_or("15");
	let timeout = timeout
		.parse::<u32>()
		.map_err(|_| format!("Invalid timeout: {timeout}"))?;
	let max_connections = args.get(Param::MaxConnections).unwrap_or("32");
	let max_connections = max_connections
		.parse::<u32>()
		.map_err(|_| format!("Invalid max connections: {max_connections}"))?;

	let listener = TcpListener::bind((addr, port))
		.map_err(|e| format!("Couldn't bind on {addr}:{port}: {e}"))?;

	let interrupted = Arc::new(AtomicBool::new(false));
	let handler_interrupted = interrupted.clone();
	if let Err(e) = ctrlc::set_handler(move || {
		handler_interrupted.store(true, Ordering::Relaxed);
	}) {
		eprintln!("Warning: Couldn't set SIGINT/TERM handler: {e}");
	}
	listener
		.set_nonblocking(true)
		.map_err(|e| format!("Couldn't set socket to non-blocking: {e}"))?;

	let was_interrupted = || interrupted.load(Ordering::Relaxed);
	let mut connections: HashMap<IpAddr, Connection> = HashMap::new();
	let mut settings = ConnectionSettings {
		timeout,
		buffer: [0; 1024],
	};

	'outer: while !was_interrupted() {
		sleep(Duration::from_millis(SLEEP_MS_PER_ROUND.into())); // don't busy loop
		while (connections.len() as u32) < max_connections {
			match listener.accept() {
				Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
					if was_interrupted() {
						break 'outer;
					}
					break;
				}
				Err(e) => eprintln!("Warning: accept() failed: {e}"),
				Ok((stream, source_addr)) => {
					if let Err(e) = stream.set_nonblocking(true) {
						eprintln!(
							"WARNING: dropping connection because set_nonblocking failed: {e}"
						);
						continue;
					}
					// if there was another connection from this address, it gets
					// unceremoniously dropped. too bad.
					connections.insert(
						source_addr.ip(),
						Connection {
							stream,
							age_ms: 0,
							written: 0,
							state: Default::default(),
						},
					);
				}
			}
		}
		connections.retain(|addr, conn| settings.handle_connection(*addr, conn));
	}
	Ok(())
}

fn main() -> ExitCode {
	if let Err(e) = try_main() {
		eprintln!("Error: {e}");
		return ExitCode::FAILURE;
	}
	ExitCode::SUCCESS
}
