use std::{collections::HashMap, io::Read, path};

use anyhow::{Context, Result};
use nanomsg::{Protocol, Socket};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tree_sitter::{Language, Parser};

#[derive(Deserialize)]
enum Command {
	INIT,
	FILE,
}

#[derive(Deserialize)]
struct Request {
	command: Command,
	args: HashMap<String, Value>,
}

#[derive(Serialize)]
struct Response<'a> {
	error: Option<&'a str>,
}

struct State {
	parser: Parser,
	file: Option<File>,
}

struct File {
	path: String,
	tree: Option<tree_sitter::Tree>,
}

fn create_socket() -> Result<Socket> {
	let mut socket = Socket::new(Protocol::Rep).context("Creating ASTEd Tree-sitter socket")?;
	socket
		.bind("ipc:///tmp/asted-server.ipc")
		.context("Binding ASTEd Tree-sitter socket")?;
	Ok(socket)
}

fn get_command(socket: &mut Socket) -> Result<Request> {
	let mut buf = String::new();
	socket
		.read_to_string(&mut buf)
		.context("Reading from ASTEd Tree-sitter socket")?;
	serde_json::from_str(&buf).context("Parsing command JSON")
}

fn get_file(path: &str) -> File {
	File {
		path: path.to_string(),
		tree: None,
	}
}

fn handle(socket: &mut Socket, state_map: &mut HashMap<String, State>) -> Result<()> {
	let req = get_command(socket)?;

	let mut state = state_map.get_mut("global").unwrap();

	match req.command {
		Command::INIT => match &req.args["lang"] {
			Value::String(s) => match s.as_str() {
				"typescript" => {
					state
						.parser
						.set_language(tree_sitter_typescript::language_typescript())
						.expect("Error loading tree-sitter typescript language");
					return Ok(());
				}
				_ => {
					socket
						.zc_write(
							&serde_json::to_vec(&Response {
								error: Some("unknown language"),
							})
							.unwrap(),
						)
						.expect("Error writing to ASTEd socket.");
					return Err(anyhow::anyhow!("Unknown language"));
				}
			},
			_ => {
				socket
					.zc_write(
						&serde_json::to_vec(&Response {
							error: Some("lang should be string"),
						})
						.unwrap(),
					)
					.expect("Error writing to ASTEd socket.");
				return Err(anyhow::anyhow!("Non-string language"));
			}
		},
		Command::FILE => match &req.args["file"] {
			Value::String(s) => {
				state.file = Some(get_file(&s));
			}
			_ => {
				socket
					.zc_write(
						&serde_json::to_vec(&Response {
							error: Some("unknown language"),
						})
						.unwrap(),
					)
					.expect("Error writing to ASTEd socket.");
				return Err(anyhow::anyhow!("Unknown language"));
			}
		},
		_ => {
			socket
				.zc_write(
					&serde_json::to_vec(&Response {
						error: Some("unknown command"),
					})
					.unwrap(),
				)
				.expect("Error writing to ASTEd socket.");
			return Err(anyhow::anyhow!("non-lang command"));
		}
	}

	Ok(())
}

fn main() {
	let mut socket = create_socket().expect("Error listening on ASTEd socket.");

	let mut state_map = HashMap::new();

	loop {
		match handle(&mut socket, &mut state_map) {
			Ok(_) => {}
			Err(e) => println!("Error handling command: {}", e),
		}
	}
}
