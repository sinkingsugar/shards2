//! A small polling host for the reloadable session API.

use std::io::BufRead;
use std::process::ExitCode;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use shards_core::Outcome;
use shards_lang::{Finished, ReloadHost, Session, Source, render};

use crate::{Options, catalog};

const POLL: Duration = Duration::from_millis(100);
const DEFAULT_FRAME: Duration = Duration::from_millis(16);

pub(super) fn watch<H: ReloadHost>(o: &Options) -> Result<ExitCode, String> {
  let (quit, input) = mpsc::channel();
  let signal_quit = quit.clone();
  let signal = shards_io::runtime::runtime().spawn(async move {
    let result = tokio::signal::ctrl_c()
      .await
      .map(|()| false)
      .map_err(|e| e.to_string());
    let _ = signal_quit.send(result);
  });
  std::thread::spawn(move || {
    for line in std::io::stdin().lock().lines() {
      match line {
        Ok(line) if line.trim() == "r" => {
          if quit.send(Ok(true)).is_err() {
            return;
          }
        }
        Ok(line) if line.trim() != "q" => continue,
        _ => break,
      }
    }
    let _ = quit.send(Ok(false));
  });
  eprintln!(
    "watching {}; r restarts; Ctrl-C, q or stdin EOF stops cleanly",
    o.file
  );
  let catalog = catalog();
  let mut session = Session::<H>::new();
  let mut changes = Changes::default();
  let mut poll_at = Instant::now();
  let mut tick_at = poll_at;
  loop {
    let mut restart = false;
    match input.try_recv() {
      Ok(Ok(true)) => restart = true,
      Ok(result) => {
        signal.abort();
        print_finished(session.stop());
        return result.map(|_| ExitCode::SUCCESS);
      }
      Err(mpsc::TryRecvError::Disconnected) => {
        signal.abort();
        print_finished(session.stop());
        return Err("watch shutdown channel disconnected".into());
      }
      Err(mpsc::TryRecvError::Empty) => {}
    }
    let now = Instant::now();
    if restart || now >= poll_at {
      let read = std::fs::read_to_string(&o.file).map_err(|e| e.to_string());
      let change = if restart {
        changes.previous = Some(read.clone());
        changes.attempted = Some(read.clone());
        Some(read)
      } else {
        changes.observe(read)
      };
      if let Some(change) = change {
        match change {
          Ok(text) => {
            let source = Source::new(&o.file, text);
            let result = if restart {
              session.reload(source, &catalog, &o.defines)
            } else {
              session.reload_preserving(source, &catalog, &o.defines)
            };
            match result {
              Ok(finished) => {
                print_finished(finished);
                eprintln!(
                  "{}: {}",
                  o.file,
                  if restart {
                    "restarted"
                  } else {
                    "reloaded (nested edits apply at the next call boundary)"
                  }
                );
                tick_at = Instant::now();
              }
              Err((source, diagnostics)) => {
                for d in diagnostics {
                  eprint!("{}", render(&d, &source));
                }
                eprintln!("{}: edit rejected; previous execution retained", o.file);
              }
            }
          }
          Err(e) => eprintln!("{}: {e}; previous execution retained", o.file),
        }
      }
      poll_at = Instant::now() + POLL;
    }
    if Instant::now() >= tick_at {
      print_finished(session.tick());
      let frame = session.frame_interval().unwrap_or(DEFAULT_FRAME);
      tick_at = Instant::now().checked_add(frame).unwrap_or(poll_at);
    }
    // File polling and quitting stay responsive even at very low script FPS.
    let delay = poll_at
      .min(tick_at)
      .saturating_duration_since(Instant::now());
    std::thread::sleep(delay.min(Duration::from_millis(10)));
  }
}

fn print_finished(finished: Vec<Finished>) {
  for Finished { wire, outcome } in finished {
    match outcome {
      Outcome::Completed(v) => println!("{wire}: {v}"),
      Outcome::Stopped => println!("{wire}: stopped"),
      Outcome::Cancelled => println!("{wire}: cancelled"),
      Outcome::Failed(e) => eprintln!(
        "{wire}: failed: {e}; stopped until the next successful reload or session restart"
      ),
    }
  }
}

/// Compare contents, not timestamps, to notice atomic saves and same-size
/// edits. Require two identical samples before trying a revision; remember
/// rejected text too, so it is diagnosed once until the contents change.
#[derive(Default)]
struct Changes {
  previous: Option<Result<String, String>>,
  attempted: Option<Result<String, String>>,
}

impl Changes {
  fn observe(&mut self, read: Result<String, String>) -> Option<Result<String, String>> {
    let stable = self.previous.as_ref() == Some(&read);
    self.previous = Some(read.clone());
    if stable && self.attempted.as_ref() != Some(&read) {
      self.attempted = Some(read.clone());
      Some(read)
    } else {
      None
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn changes_wait_for_stability_and_retry_after_read_errors() {
    let mut changes = Changes::default();
    let a = Ok("41".to_string());
    let b = Ok("42".to_string());
    let missing = Err("missing".to_string());
    assert!(changes.observe(a.clone()).is_none());
    // A partial write is never submitted.
    assert!(changes.observe(b.clone()).is_none());
    assert_eq!(changes.observe(b.clone()), Some(b.clone()));
    assert!(changes.observe(b.clone()).is_none());
    assert!(changes.observe(missing.clone()).is_none());
    assert_eq!(changes.observe(missing.clone()), Some(missing));
    assert!(changes.observe(b.clone()).is_none());
    assert_eq!(changes.observe(b.clone()), Some(b));
    assert!(changes.observe(a.clone()).is_none());
    assert_eq!(changes.observe(a.clone()), Some(a));
  }
}
