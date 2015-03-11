#![feature(core)]
#![feature(io)]
#![feature(net)]
#![feature(old_path)]

extern crate hyper;
extern crate "rustc-serialize" as rustc_serialize;

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

use std::thread;
use std::process::Command;

#[derive(RustcDecodable)]
pub struct HookConfiguration  {
	hooks: Vec<HookConfig>,
}

#[derive(RustcDecodable)]
pub struct HookConfig {
	name: String,
	action: HookAction,
}

#[derive(RustcDecodable)]
pub struct HookAction  {
  cmd: String,
	parms: Vec<String>,
	pwd: String,
}

#[derive(RustcDecodable)]
pub struct GitHook  {
    before: String,
    after: String,
    repository: Repository,
}

#[derive(RustcDecodable)]
pub struct Repository {
	name: String,
	url: String,
}

pub struct Daemon {
	config: HookConfiguration,
}

impl Daemon {
	fn deploy(&self, hk: &HookConfig) {
		println!("Processing {}", hk.name);
		println!("{:?}", hk.action.cmd );

    let parms = &hk.action.parms;
    let output = Command::new(&hk.action.cmd)
      .args(parms.as_slice())
      .current_dir(&hk.action.pwd)
      .output().unwrap_or_else(|e| {
      panic!("failed to execute process: {}", e)
    });

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
	}
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
							println!("Repository {}", repo_name);
							match self.config.hooks.iter().filter(|&binding| binding.name == repo_name).next() {
								Some(hk) => self.deploy(hk),
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


fn main() {

	let mut json_config = String::new();

	let config_location = &Path::new("config.json");

	match File::open(config_location) {
		Err(err) => panic!("Error during config file read: {:?}. {} {}",
			config_location, err.description(), err.detail().unwrap_or("".to_string())),
		Ok(icf) => {
			let mut config_file = icf;
			config_file.read_to_string(&mut json_config).ok().unwrap()
		},
	};

	let config: HookConfiguration = match decode(json_config.as_slice()) {
		Err(err) => panic!("{}", err),
		Ok(content) => content,
	};

	let d = Daemon{config: config};
	let port = 5000;

	println!("Starting up, listening on port {}", port);
	Server::new(d).listen(IpAddr::new_v4(127, 0, 0, 1), port).unwrap();

}
