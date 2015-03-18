use std::collections::HashMap;
use std::thread;

use std::sync::mpsc::{Receiver, Sender, channel};
use time;

use hook_configuration::HookConfiguration;
use deployer::{DeployMessage, Deployer};
use tools::to_string;

pub struct Dispatcher {
  pub config: HookConfiguration,
}

impl Dispatcher {
  pub fn run(&self, rx: Receiver<DeployMessage>) {
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
    });

  }
}
