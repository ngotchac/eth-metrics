use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::io::{Error, ErrorKind};

use libc::{kill, SIGTERM, SIGKILL};

pub struct ChildGuard {
    child: Option<Child>,
    terminated: Arc<AtomicBool>,
}

impl ChildGuard {
    pub fn new(child: Child) -> Self {
        ChildGuard {
            child: Some(child),
            terminated: Arc::new(AtomicBool::from(false)),
        }
    }

	pub fn check(&mut self) -> Result<(), Error> {
		let exit_status = {
			let child = match self.child {
				Some(ref mut child) => child,
				None => return Ok(()),
			};

			match child.try_wait() {
				Ok(None) => return Ok(()),
				Err(e) => return Err(e),
				Ok(Some(exit_status)) => exit_status,
			}
		};

		let child = ::std::mem::replace(&mut self.child, None);
		let output = child.unwrap().wait_with_output()?;
		println!("{}", String::from_utf8_lossy(&output.stderr));
		Err(Error::new(ErrorKind::Other, format!("Process exited unexpectedly with status {}", exit_status)))
	}

    pub fn terminate(&mut self) {
        if self.terminated.load(Ordering::SeqCst) {
            return;
        }

		let child = match self.child {
			Some(ref mut child) => child,
			None => return,
		};

        let child_id = child.id();
        let terminated = self.terminated.clone();
        unsafe {
            kill(child_id as i32, SIGTERM);
        }

        thread::spawn(move || {
            thread::sleep(Duration::from_secs(10));
            if !terminated.load(Ordering::SeqCst) {
                println!("Force-kill the process.");
                unsafe {
                    kill(child_id as i32, SIGKILL);
                }
            }
        });

        child.wait().expect("Could not wait for process end.");
        self.terminated.store(true, Ordering::SeqCst);
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
		if !self.terminated.load(Ordering::SeqCst) {
			let child = match self.child {
				Some(ref child) => child,
				None => return,
			};

			let child_id = child.id();
			println!("Force-kill the process.");
			unsafe {
				kill(child_id as i32, SIGKILL);
			}
		}
    }
}
