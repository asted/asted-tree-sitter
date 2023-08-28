use std::{
	collections::HashMap,
	fs,
	net::SocketAddr,
	path::Path,
	sync::{Mutex, RwLock},
};

use anyhow::{Context, Result};
use axum::{
	body::Bytes,
	http::StatusCode,
	response::{IntoResponse, Response},
	routing::get,
	Router,
};
use clap::Parser as ClapParser;
use dashmap::DashMap;
use flatbuffers;
use message_generated::asted::interface::{FileRequest, InitRequest, Request, RequestUnion};
use once_cell::sync::Lazy;
use tree_sitter::Parser;

#[allow(dead_code, unused_imports)]
mod message_generated;
mod tree_serialize;

struct State {
	parser: Mutex<Parser>,
	files: HashMap<Box<Path>, RwLock<tree_sitter::Tree>>,
}

static STATE_MAP: Lazy<DashMap<String, State>> = Lazy::new(|| DashMap::new());

#[derive(Debug)]
enum Error {
	UnknownCommand(String),
	UnknownLanguage(String),
	UnknownFile(String),
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::UnknownCommand(s) => write!(f, "{}", s),
			Error::UnknownLanguage(s) => write!(f, "{}", s),
			Error::UnknownFile(s) => write!(f, "{}", s),
		}
	}
}

impl IntoResponse for Error {
	fn into_response(self) -> Response {
		let status = match self {
			Error::UnknownCommand(_) => StatusCode::BAD_REQUEST,
			Error::UnknownLanguage(_) => StatusCode::BAD_REQUEST,
			Error::UnknownFile(_) => StatusCode::BAD_REQUEST,
		};

		(status, self.to_string()).into_response()
	}
}

impl std::error::Error for Error {}

async fn handle(body: Bytes) -> Result<Response> {
	let req = flatbuffers::root::<Request>(&body).context("Failed to decode request")?;

	let mut state = STATE_MAP.get_mut("global").unwrap();

	match req.request_type() {
		RequestUnion::InitRequest => {
			let req = unsafe { InitRequest::init_from_table(req.request()) };

			match req.lang() {
				"typescript" => {
					state
						.parser
						.lock()
						.unwrap()
						.set_language(tree_sitter_typescript::language_typescript())
						.context("Error loading tree-sitter typescript language")?;
					return Ok("".into_response());
				}
				lang => {
					Err(Error::UnknownLanguage(format!("Unsupported language: {}", lang)).into())
				}
			}
		}
		RequestUnion::FileRequest => {
			let req = unsafe { FileRequest::init_from_table(req.request()) };
			let path = Path::new(req.path());

			if path.is_dir() {
				return Err(Error::UnknownFile(format!("{} is a directory!", path.display())).into());
			}
			if !path.is_file() {
				return Err(Error::UnknownFile(format!("File not found: {}", path.display())).into());
			}

			let text = fs::read_to_string(path).context("Error reading file")?;
			let utf16_text = text.encode_utf16().collect::<Vec<u16>>();

			let tree = {
				let old_tree = state.files.get(path).map(|v| v.read().unwrap());
				state
					.parser
					.lock()
					.unwrap()
					.parse_utf16(&utf16_text, old_tree.as_deref())
					.unwrap()
			};

			let res = tree_serialize::serialize(&utf16_text, &tree);

			state.files.insert(path.into(), RwLock::new(tree));

			Ok(res.into_response())
		}
		_ => Err(
			Error::UnknownCommand("The server does not understand this command!".to_string())
				.into(),
		),
	}
}

async fn handler(body: Bytes) -> Response {
	match handle(body).await {
		Ok(r) => r,
		Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
	}
}

#[derive(ClapParser)]
struct Args {
	/// The host to listen on
	#[arg(short = 'H', long, default_value = "127.0.0.1")]
	host: String,
	/// The port to listen on
	#[arg(short, long, default_value = "44790")]
	port: u16,
}

#[tokio::main]
async fn main() {
	let args = Args::parse();

	STATE_MAP.insert(
		"global".to_string(),
		State {
			parser: Mutex::new(Parser::new()),
			files: HashMap::new(),
		},
	);

	let app = Router::new().route("/", get(handler));

	let addr = match format!("{}:{}", args.host, args.port).parse::<SocketAddr>() {
		Ok(addr) => addr,
		Err(e) => {
			println!("Failed to parse address: {}", e);
			std::process::exit(1);
		}
	};

	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}
