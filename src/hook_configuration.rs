#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfiguration  {
  pub hooks: Vec<HookConfig>,
  pub slack: String,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookConfig {
  pub name: String,
  pub action: HookAction,
}

#[derive(RustcDecodable)]
#[derive(Clone)]
pub struct HookAction  {
  pub cmd: String,
  pub parms: Vec<String>,
  pub pwd: String,
}
