#![feature(core)]
#![feature(net)]
#![feature(std_misc)]

extern crate hyper;
extern crate "rustc-serialize" as rustc_serialize;
extern crate time;
extern crate slackhook;

use std::io::prelude::*;
use std::fs::File;
use std::net::IpAddr;
use hyper::Server;
use hyper::server::Request;
use hyper::server::Response;
use hyper::uri::RequestUri;
use hyper::net::Fresh;
use hyper::server::Handler;
use rustc_serialize::json::decode;
use std::path::Path;

use std::sync::{Mutex, Arc};
use std::sync::mpsc::{Sender, channel};

use dispatcher::Dispatcher;
use hook_configuration::HookConfiguration;
use deployer::DeployMessage;

mod hook_configuration;
mod dispatcher;
mod deployer;
mod tools;


#[derive(RustcDecodable)]
pub struct GitHook  {
    // before: String,
    // after: String,
    repository: Repository,
}

#[derive(RustcDecodable)]
pub struct Repository {
	name: String,
	// url: String,
}

pub struct Daemon {
  intercom: Arc<Mutex<Sender<DeployMessage>>>,
  config: HookConfiguration,
}

impl Handler for Daemon {
	fn handle(&self, req: Request, res: Response<Fresh>) {

		let mut s = String::new();
		let mut myreq = req;
		if myreq.uri == RequestUri::AbsolutePath("/hook/".to_string()) {
			match myreq.read_to_string(&mut s) {
				Ok(_) => {
					match decode::<GitHook>(s.as_slice()) {
						Ok(decoded ) => {
							let repo_name = decoded.repository.name;
							match self.config.hooks.iter().filter(|&binding| binding.name == repo_name).next() {
								Some(hk) => {
                  let _ = self.intercom.lock().unwrap().send(DeployMessage::Deploy(hk.clone()));
                },
								None => println!("No hook for {}", repo_name),
							}
						},
						Err(e) => println!("Error while parsing data: {:?}",  e),
					}
				},
				_ => {}
			}
		}

    let mut res = res.start().unwrap();
    res.write_all(b"OK.").unwrap();
    res.end().unwrap();
	}
}


pub fn main() {

	let mut json_config = String::new();

	let config_location = &Path::new("config.json");

	match File::open(config_location) {
		Err(err) => panic!("Error during config file read: {:?}. {}",
			config_location, err.to_string()),
		Ok(icf) => {
			let mut config_file = icf;
			config_file.read_to_string(&mut json_config).ok().unwrap()
		},
	};

	let config: HookConfiguration = match decode(json_config.as_slice()) {
		Err(err) => panic!("{}", err),
		Ok(content) => content,
	};

  let (tx, rx) = channel();

  let dispatcher = Dispatcher{config: config.clone()};
  dispatcher.run(rx);
	let handler = Daemon{config: config, intercom: Arc::new(Mutex::new(tx)) };

	let port = 5000;

	println!("Starting up, listening on port {}", port);
	Server::new(handler).listen(IpAddr::new_v4(127, 0, 0, 1), port).unwrap();

}
