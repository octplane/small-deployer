use time;

pub fn to_string(ti: time::Tm) -> String {
    let format = "%Y-%m-%d %T.%f";
    let mut ts = time::strftime(format, &ti).ok().unwrap();
    let l = ts.len();
    ts.truncate(l - 6);
    ts
}
