use std::os::unix::prelude::*;
use std::sync::mpsc::{TryRecvError, Receiver};
use std::convert::AsRef;
use small_logger::runner;

use std::ops::Sub;

use time;

use slack_hook::{Slack, Payload, PayloadTemplate};
use hook_configuration::{HookConfig, HookAction, SlackConfiguration};
use tools::to_string;

#[derive(Clone)]
#[allow(dead_code)]
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
      let deploy_instruction = rx.recv();
      match deploy_instruction {
        Ok(DeployMessage::Deploy(_)) => {
          let mut extra_deploy_instruction;
          while {
            extra_deploy_instruction = rx.try_recv();
            match extra_deploy_instruction {
              Ok(DeployMessage::Deploy(_)) => true,
              Ok(DeployMessage::Exit) => false,
              Err(TryRecvError::Empty) => false,
              Err(TryRecvError::Disconnected) => false
            }
          } {}

          self.deploy(); true
        },
        Ok(DeployMessage::Exit) => false,
        Err(e) => { println!("Error: {}", e); false }
      }
    } {}
    self.log("Stopping deployer.");
  }

  fn deploy(&self) {
    let hk = self.conf.clone();

    self.message(format!("Starting Deploy for {}.", self.name));
    let r = runner::Runner;

    // FIXME: modify small-logger to get duration from inner runner.
    let start_time = time::now();
    let status = r.run(&hk.cmd, hk.parms, "./logs".into() , format!("deployer_{}", self.name), Some(hk.pwd));
    let end_time = time::now();
    let duration = end_time.sub(start_time);

    match status {
      Ok(estatus) => {
        if estatus.success() {
          let log_message = format!(":sunny: {} deployed successfully in {}s.", self.name, duration.num_seconds());
          self.log(log_message.as_ref());
          self.message(log_message);
        } else {
          match estatus.code() {
            Some(exit_code) => {
              self.log(format!("Deploy failed with status {}.", exit_code).as_ref());
              self.message(format!(":umbrella: {} deployed failed.", self.name));
            },
            None => match estatus.signal() {
              Some(signal_value) => self.log(format!("Deploy was interrupted with signal {}.", signal_value).as_ref()),
              None => self.log("This should never happen."),
            }
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
        let slack = Slack::new(conf.webhook_url.as_ref());
        let p = Payload::new(PayloadTemplate::Complete {
          text: Some(message.as_ref()),
          channel: Some(conf.channel.as_ref()),
          username: Some(conf.username.as_ref()),
          icon_url: match conf.icon_url {
            None => None,
            Some(ref s) => Some(s.as_ref()),
          },
          icon_emoji: match conf.icon_emoji {
            None => None,
            Some(ref s) => Some(s.as_ref()),
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
