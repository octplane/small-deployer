#![feature(core)]
#![feature(io)]
#![feature(net)]
#![feature(old_path)]
#![feature(std_misc)]

extern crate hyper;
extern crate "rustc-serialize" as rustc_serialize;
extern crate time;
extern crate slackhook;

use std::io::prelude::*;
use std::os::unix::prelude::*;
use std::io::BufReader;
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
use std::process::Stdio;
use std::process::Command;
use std::collections::HashMap;

use std::sync::{Mutex, Arc};
use std::sync::mpsc::{Receiver, Sender, channel};

use std::io::{self};

use std::fmt;

use slackhook::{Slack, Payload, PayloadTemplate};

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfiguration  {
	hooks: Vec<HookConfig>,
  slack: String,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfig {
	name: String,
	action: HookAction,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookAction  {
  cmd: String,
	parms: Vec<String>,
	pwd: String,
}

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


#[derive(Debug)]
#[derive(Clone)]
enum LogSource {
  StdOut,
  StdErr,
}

#[derive(Debug)]
pub struct TimestampedLine {
  source: LogSource,
  time: time::Tm,
  content: String,
}

impl fmt::Display for TimestampedLine {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_fmt(format_args!("[{:?}][{}] {}", self.source, to_string(self.time), self.content))
    }
}


fn to_string(time: time::Tm) -> String {
    let format = "%Y-%m-%d %T.%f";
    let mut ts = time::strftime(format, &time).ok().unwrap();
    let l = ts.len();
    ts.truncate(l-6);
    ts
}

#[derive(Clone)]
enum DeployMessage {
  Deploy(HookConfig),
  Exit
}

struct Dispatch {
  config: HookConfiguration,
}

impl Dispatch {
  fn run(&self, rx: Receiver<DeployMessage>) {
    let mut to_workers: HashMap<String, Sender<DeployMessage>> = HashMap::new();
    let mut workers: HashMap<String, Deployer> = HashMap::new();
    for conf in &self.config.hooks {
      let worker = Deployer{
        name: conf.name.clone(),
        conf: conf.action.clone(),
        slack_url: (&self).config.slack.clone(),
      };
      workers.insert(conf.name.clone(), worker);
    }

    for (name, worker) in workers.into_iter() {
      let (tx, rx) = channel();
      to_workers.insert(name.clone(), tx);

      thread::spawn(move || {
        worker.run(rx);
      });
    }

    thread::spawn(move || {
      println!("[{}] Starting dispatcher", to_string(time::now()));
      while let Ok(data) = rx.recv() {
        match data {
          DeployMessage::Deploy(hk) => {
            println!("Want to deploy {}", hk.name);
            to_workers.get(&hk.name).unwrap().send(DeployMessage::Deploy(hk));
          },
          DeployMessage::Exit => println!("We should exit"),
        }
      }
      println!("[{}] Stopping dispatcher", to_string(time::now()));
    }).join();

  }
}



struct Deployer {
  name: String,
  conf: HookAction,
  slack_url: String,
}

impl Deployer {
  fn run(&self, rx: Receiver<DeployMessage>) {
    println!("[{}][{}] Starting deployer.", to_string(time::now()), self.name);
    while{
      match rx.recv() {
        Ok(DeployMessage::Deploy(hk)) => { println!("{}", hk.name); true },
        Ok(DeployMessage::Exit) => false,
        Err(e) => { println!("Error: {}", e); false }
      }
    } {}
    println!("[{}][{}] Stopping deployer.", to_string(time::now()), self.name);
  }

	fn deploy(&self, hk: &HookConfig) {
		println!("[{}][{}] Processing.", to_string(time::now()), hk.name);

    let parms = &hk.action.parms;
    let mut child = match Command::new(&hk.action.cmd)
      .args(parms.as_slice())
      .current_dir(&hk.action.pwd)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn() {
        Err(why) => panic!("couldn't spawn {}: {}", &hk.action.cmd, why.to_string()),
        Ok(child) => child,
    };

    //https://github.com/rust-lang/rust/blob/b83b26bacb6371173cdec6bf68c7ffa69f858c84/src/libstd/process.rs
    fn read_timestamped_lines<T: Read + Send + 'static>(stream: Option<T>, source: LogSource) -> Receiver<io::Result<Vec<TimestampedLine>>> {
      let (tx, rx) = channel();
      match stream {
        Some(stream) => {
          thread::spawn(move || {
            let mut br = BufReader::with_capacity(64, stream);
            let mut lines: Vec<TimestampedLine> = Vec::new();
            while {
              let mut line = String::new();
              let read_status = br.read_line(&mut line);
              let ok = line != "";
              if ok {
                let now = time::now();
                lines.push(TimestampedLine{source: source.clone(), time: now, content: line});
              }
              ok
            } {}

            tx.send(Ok(lines)).unwrap();
          });
        }
        None => tx.send(Ok(Vec::<TimestampedLine>::new())).unwrap()
      }
      rx
    }

    let stdout = read_timestamped_lines(child.stdout.take(), LogSource::StdOut);
    let stderr = read_timestamped_lines(child.stderr.take(), LogSource::StdErr);

    let status = child.wait();

    let stdout = match stdout.recv() {
      Ok(Ok(s)) => s,
      Ok(Err(e)) => panic!("IOError {}", e),
      Err(e) => panic!("RecvError {}", e),
    };

    let stderr = match stderr.recv() {
      Ok(Ok(s)) => s,
      Ok(Err(e)) => panic!("IOError {}", e),
      Err(e) => panic!("RecvError {}", e),
    };

    match status {
      Ok(estatus) => {
        if estatus.success() {
          println!("[deploy][{}] Deploy completed successfully", to_string(time::now()));
        } else {
          match estatus.code() {
            Some(exit_code) => println!("[deploy][{}] Deploy aborted with status {}.", to_string(time::now()), exit_code),
            None => match estatus.signal() {
              Some(signal_value) => println!("[deploy][{}] Deploy was interrupted with signal {}.", to_string(time::now()), signal_value),
              None => println!("[deploy][{}] This should never happend.", to_string(time::now())),
            }
          }
          for line in stdout {
            println!("{}", line);
          }
          for line in stderr {
            println!("{}", line);
          }
        }
      },
      Err(e) => println!("An error occured: {:?}",e),
    }
	}
  fn message(&self, message: &str) {
    // http://www.emoji-cheat-sheet.com/
    let slack = Slack::new(self.slack_url.as_slice());
    let p = Payload::new(PayloadTemplate::Complete {
      text: Some(message),
      channel: Some("#deploys"),
      username: Some("Deployr"),
      icon_url: None,
      icon_emoji: Some(":computer:"),
      attachments: None,
      unfurl_links: Some(true),
      link_names: Some(false)
    });

    let res = slack.send(&p);
    match res {
        Ok(()) => println!("ok"),
        Err(x) => println!("ERR: {:?}",x)
    }
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


fn main() {

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

  let dispatcher = Dispatch{config: config.clone()};
  dispatcher.run(rx);
	let handler = Daemon{config: config, intercom: Arc::new(Mutex::new(tx)) };

	let port = 5000;

	println!("Starting up, listening on port {}", port);
	Server::new(handler).listen(IpAddr::new_v4(127, 0, 0, 1), port).unwrap();

}
