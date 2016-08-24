
#[derive(Clone)]
#[derive(RustcDecodable)]
pub struct HookConfiguration {
    pub hooks: Vec<HookConfig>,
    pub slack: Option<SlackConfiguration>,
}

#[derive(Clone)]
#[derive(RustcDecodable)]
pub struct SlackConfiguration {
    pub webhook_url: String,
    pub channel: String,
    pub username: String,
    pub icon_url: Option<String>,
    pub icon_emoji: Option<String>,
}

#[derive(Clone)]
#[derive(RustcDecodable)]
pub struct HookConfig {
    pub name: String,
    pub branch: Option<String>,
    pub action: HookAction,
}

impl HookConfig {
    pub fn worker_name(&self) -> String {
        let b = self.branch.clone().unwrap_or("all".to_string());
        format!("{}-{}", self.name, b)
    }
}

#[derive(Clone)]
#[derive(RustcDecodable)]
pub struct HookAction {
    pub cmd: String,
    pub parms: Vec<String>,
    pub pwd: String,
}
