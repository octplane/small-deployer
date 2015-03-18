use std::io::prelude::*;
use std::os::unix::prelude::*;
use std::io::BufReader;
use std::fmt;
use std::io::{self};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;

use time;

use slackhook::{Slack, Payload, PayloadTemplate};
use hook_configuration::{HookConfig, HookAction};
use tools::to_string;

#[derive(Debug)]
#[derive(Clone)]
enum LogSource {
  StdOut,
  StdErr,
}

#[derive(Debug)]
pub struct TimestampedLine {
  source: LogSource,
  name: String,
  time: time::Tm,
  content: String,
}

impl fmt::Display for TimestampedLine {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_fmt(format_args!("[{}][{:?}] {}", to_string(self.time), self.source, self.content))
    }
}

#[derive(Clone)]
pub enum DeployMessage {
  Deploy(HookConfig),
  Exit
}

pub struct Deployer {
  pub name: String,
  pub conf: HookAction,
  pub slack_url: String,
}

impl Deployer {
  pub fn run(&self, rx: Receiver<DeployMessage>) {
    println!("[{}][{}] Starting deployer.", to_string(time::now()), self.name);
    while{
      match rx.recv() {
        Ok(DeployMessage::Deploy(_)) => { self.deploy(); true },
        Ok(DeployMessage::Exit) => false,
        Err(e) => { println!("Error: {}", e); false }
      }
    } {}
    println!("[{}][{}] Stopping deployer.", to_string(time::now()), self.name);
  }

	fn deploy(&self) {
    let hk = &self.conf;

		println!("[{}][{}] Processing.", to_string(time::now()), self.name);

    let parms = &hk.parms;
    let mut child = match Command::new(&hk.cmd)
      .args(parms.as_slice())
      .current_dir(&hk.pwd)
      .stdin(Stdio::null())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn() {
        Err(why) => panic!("couldn't spawn {}: {}", &hk.cmd, why.to_string()),
        Ok(child) => child,
    };

    //https://github.com/rust-lang/rust/blob/b83b26bacb6371173cdec6bf68c7ffa69f858c84/src/libstd/process.rs
    fn read_timestamped_lines<T: Read + Send + 'static>(stream: Option<T>, name: &str, source: LogSource) -> Receiver<io::Result<Vec<TimestampedLine>>> {
      let (tx, rx) = channel();
      let sname = name.to_string();
      match stream {
        Some(stream) => {
          thread::spawn(move || {
            let mut br = BufReader::with_capacity(64, stream);
            let mut lines: Vec<TimestampedLine> = Vec::new();
            while {
              let mut line = String::new();
              let ok = match br.read_line(&mut line) {
                Ok(0) => false,
                Ok(_) => true,
                Err(e) => {println!("Something went wrong while reading the data: {}", e.to_string()); false}
              };
              if ok {
                let now = time::now();
                lines.push(TimestampedLine{source: source.clone(), name: sname.clone(), time: now, content: line});
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

    let stdout = read_timestamped_lines(child.stdout.take(), self.name.as_slice(), LogSource::StdOut);
    let stderr = read_timestamped_lines(child.stderr.take(), self.name.as_slice(), LogSource::StdErr);

    let status = child.wait();

    let stdout = match stdout.recv() {
      Ok(Ok(s)) => s,
      Ok(Err(e)) => panic!("Stdout IOError {}", e),
      Err(e) => panic!("Stdout RecvError {}", e),
    };

    let stderr = match stderr.recv() {
      Ok(Ok(s)) => s,
      Ok(Err(e)) => panic!("Stderr IOError {}", e),
      Err(e) => panic!("Stderr RecvError {}", e),
    };

    match status {
      Ok(estatus) => {
        if estatus.success() {
          println!("[{}][{}] Deploy completed successfully", to_string(time::now()), self.name);
          self.message(format!(":sunny: {} deployed successfully !", self.name));
        } else {
          match estatus.code() {
            Some(exit_code) => {
              println!("[{}][{}] Deploy failed with status {}.", to_string(time::now()), self.name, exit_code);
              self.message(format!(":umbrella: {} deployed failed.", self.name));
            },
            None => match estatus.signal() {
              Some(signal_value) => println!("[{}][{}] Deploy was interrupted with signal {}.", to_string(time::now()), self.name, signal_value),
              None => println!("[{}][{}] This should never happend.", to_string(time::now()), self.name),
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
  fn message(&self, message: String) {
    // http://www.emoji-cheat-sheet.com/
    let slack = Slack::new(self.slack_url.as_slice());
    let p = Payload::new(PayloadTemplate::Complete {
      text: Some(message.as_slice()),
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
