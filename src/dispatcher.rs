use std::collections::HashMap;
use std::thread;

use std::sync::mpsc::{Receiver, Sender, channel};
use time;
use std::convert::AsRef;

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

            let worker = Deployer {
                name: conf.worker_name(),
                conf: conf.action.clone(),
                slack: (&self).config.slack.clone(),
            };
            workers.insert(conf.worker_name(), worker);
        }

        for (name, worker) in workers.into_iter() {
            let (tx, rx) = channel();
            to_workers.insert(name.clone(), tx);

            thread::spawn(move || {
                worker.run(rx);
            });
        }

        self.log("Starting dispatcher.");
        while let Ok(data) = rx.recv() {
            match data {
                DeployMessage::Deploy(hk) => {
                    let wname = hk.worker_name();
                    self.log(format!("Want to deploy {}.", wname).as_ref());
                    match to_workers.get(&wname).unwrap().send(DeployMessage::Deploy(hk)) {
                        Err(e) => {
                            println!("[{}][system] Send to deployer {} failed: {}.",
                                     to_string(time::now()),
                                     wname,
                                     e.to_string())
                        }
                        _ => {}
                    }
                }
                DeployMessage::Exit => println!("We should exit."),
            }
        }
        self.log("Stopping dispatcher.");
    }

    fn name(&self) -> String {
        "dispatcher".to_string()
    }

    fn log(&self, info: &str) {
        println!("[{}][{}][system] {}",
                 to_string(time::now()),
                 self.name(),
                 info);
    }
}
