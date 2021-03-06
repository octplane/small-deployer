#![feature(custom_attribute, custom_derive, plugin)]

extern crate hyper;
extern crate time;
extern crate slack_hook;
extern crate rustc_serialize;
extern crate small_logger;

use std::io::prelude::*;
use std::fs::File;
use std::thread;
use std::convert::AsRef;

use hyper::server::Server;
use hyper::server::Request;
use hyper::server::Response;
use hyper::uri::RequestUri;
use hyper::net::Fresh;
use hyper::server::Handler;
use std::path::Path;

use std::sync::{Mutex, Arc};
use std::sync::mpsc::{Sender, channel};

use rustc_serialize::json::Json;
use rustc_serialize::json;

use dispatcher::Dispatcher;
use hook_configuration::HookConfiguration;
use deployer::DeployMessage;

mod hook_configuration;
mod dispatcher;
mod deployer;
mod tools;

pub struct GitHook {
    content: Json,
}

impl GitHook {
    fn repository_name(&self) -> &str {
        let path = ["repository", "name"];
        self.content.find_path(&path).unwrap().as_string().unwrap()
    }
    fn reference(&self) -> &str {
        self.content.find("ref").unwrap().as_string().unwrap()
    }
    fn branch(&self) -> &str {
        self.reference().trim_left_matches("refs/heads/")
    }
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
                    println!("Got payload {}", s);
                    let decode = Json::from_str(s.as_ref());

                    match decode {
                        Ok(decoded) => {
                            let gh = GitHook { content: decoded };

                            let repo_name = gh.repository_name();
                            let branch = gh.branch();

                            match self.config
                                .hooks
                                .iter()
                                .filter(|&binding| if repo_name == binding.name {
                                    match binding.branch.clone() {
                                        Some(target_branch) => branch == target_branch,
                                        None => true,
                                    }
                                } else {
                                    false
                                })
                                .next() {
                                Some(hk) => {
                                    let _ = self.intercom
                                        .lock()
                                        .unwrap()
                                        .send(DeployMessage::Deploy(hk.clone()));
                                }
                                None => println!("No hook for {}/{}", repo_name, branch),
                            }
                        }
                        Err(e) => {
                            println!("Error while parsing http: {:?}", e);
                            println!("{}", s);
                        }
                    }
                }
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
        Err(err) => {
            panic!("Error during config file read: {:?}. {}",
                   config_location,
                   err.to_string())
        }
        Ok(icf) => {
            let mut config_file = icf;
            config_file.read_to_string(&mut json_config).ok().unwrap()
        }
    };

    let config: HookConfiguration = match json::decode(json_config.as_ref()) {
        Err(err) => {
            println!("Error while parsing config file:");
            println!("{}", err);
            println!("{}", json_config);
            panic!("Sorry.");
        }
        Ok(content) => content,
    };

    let (tx, rx) = channel();

    let dispatcher = Dispatcher { config: config.clone() };
    thread::spawn(move || {
        dispatcher.run(rx);
    });
    let handler = Daemon {
        config: config,
        intercom: Arc::new(Mutex::new(tx)),
    };

    let port = 5000;
    println!("Starting up, listening on port {}.", port);
    let _ = Server::http(format!("127.0.0.1:{}", port).as_str()).unwrap().handle(handler);

}
