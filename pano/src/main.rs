use std::{process::{Command, Stdio}, io::{Read, Write}, thread, time};
fn main() {
  let cmd = "./chdkptp-r921/chdkptp.sh";
  println!("asdf0");
  let mut child = Command::new(cmd)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .spawn()
    .expect("CHDKPTP Failed to start.");
  println!("asdf1");
  let mut in_pipe = child.stdin.take().unwrap();
  let mut out_pipe = child.stdout.take().unwrap();
  println!("asdf2");
  let mut outbuf = [0; 1000];
  println!("asdf3");
  in_pipe.write("list\n".as_bytes()).unwrap();
  println!("asdf4");
  thread::sleep(time::Duration::from_millis(1000));
  println!("asdf5");
  out_pipe.read(&mut outbuf).unwrap();
  let hold = String::from_utf8(outbuf.to_vec()).unwrap();
  println!("{hold}");
}
