use std::io::prelude::*;
use std::os::unix::prelude::*;
use std::io::BufReader;
use std::fmt;
use std::io::{self};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread;

use std::ops::Sub;

use time;

use slackhook::{Slack, Payload, PayloadTemplate};
use hook_configuration::{HookConfig, HookAction, SlackConfiguration};
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
        fmt.write_fmt(format_args!("[{}][{}][{:?}] {}", to_string(self.time), self.name, self.source, self.content))
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
  pub slack: Option<SlackConfiguration>,
}

impl Deployer {
  pub fn run(&self, rx: Receiver<DeployMessage>) {
    self.log("Starting deployer.");
    while{
      match rx.recv() {
        // FIXME try_recv to exhaust the deploy queue
        Ok(DeployMessage::Deploy(_)) => { self.deploy(); true },
        Ok(DeployMessage::Exit) => false,
        Err(e) => { println!("Error: {}", e); false }
      }
    } {}
    self.log("Stopping deployer.");
  }

	fn deploy(&self) {
    let hk = &self.conf;

		self.log("Processing.");

    let parms = &hk.parms;

    let start_time = time::now();
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
    let end_time = time::now();

    let duration = end_time.sub(start_time);

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
          // self.log("Deploy completed successfully");
          // let lines = stdout.into_iter().map(|log_lines| log_lines.to_string());
          // let so = lines.collect::<Vec<String>>();

          self.message(format!(":sunny: {} deployed successfully in {}s.", self.name, duration.num_seconds()));
        } else {
          match estatus.code() {
            Some(exit_code) => {
              self.log(format!("Deploy failed with status {}.", exit_code).as_slice());
              self.message(format!(":umbrella: {} deployed failed.", self.name));
            },
            None => match estatus.signal() {
              Some(signal_value) => self.log(format!("Deploy was interrupted with signal {}.", signal_value).as_slice()),
              None => self.log("This should never happen."),
            }
          }
          self.log("Content of stdout:");
          for line in stdout {
            println!("{}", line);
          }
          self.log("Content of stderr:");
          for line in stderr {
            println!("{}", line);
          self.log("End of trace.");

          }
        }
      },
      Err(e) => println!("An error occured: {:?}",e),
    }
	}

  fn log(&self, info: &str) {
    println!("[{}][{}][system] {}", to_string(time::now()), self.log_name(), info);
  }

  fn log_name(&self) -> String {
    format!("deployer-{}", self.name)
  }

  fn message(&self, message: String) {
    match self.slack {
      Some(ref conf) => {
        // http://www.emoji-cheat-sheet.com/
        let slack = Slack::new(conf.webhook_url.as_slice());
        let p = Payload::new(PayloadTemplate::Complete {
          text: Some(message.as_slice()),
          channel: Some(conf.channel.as_slice()),
          username: Some(conf.username.as_slice()),
          icon_url: match conf.icon_url {
            None => None,
            Some(ref s) => Some(s.as_slice()),
          },
          icon_emoji: match conf.icon_emoji {
            None => None,
            Some(ref s) => Some(s.as_slice()),
          },
          attachments: None,
          unfurl_links: Some(true),
          link_names: Some(false)
        });

        match slack.send(&p) {
            Ok(()) => self.log("Sent notification to slack."),
            Err(x) => println!("ERR: {:?}",x)
        }
      },
      None => {}
    }

  }
}
