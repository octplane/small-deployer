#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfiguration  {
  pub hooks: Vec<HookConfig>,
  pub slack: Option<SlackConfiguration>,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct SlackConfiguration {
  pub webhook_url: String,
  pub channel: String,
  pub username: String,
  pub icon_url: Option<String>,
  pub icon_emoji: Option<String>,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfig {
  pub name: String,
  pub branch: Option<String>,
  pub action: HookAction,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookAction  {
  pub cmd: String,
  pub parms: Vec<String>,
  pub pwd: String,
}
