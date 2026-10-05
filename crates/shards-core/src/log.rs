//! Where `Log` writes. Lines go to standard output unless the current thread
//! is capturing them ([`capture`]), which is how tests read what a program
//! logged. Instances run on the thread that ticks their mesh, so a capture
//! sees the lines of the meshes ticked inside it.

use std::cell::RefCell;

thread_local! {
  static CAPTURE: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Writes one log line.
pub fn emit(line: String) {
  CAPTURE.with(|c| match &mut *c.borrow_mut() {
    Some(lines) => lines.push(line),
    None => println!("{line}"),
  });
}

/// Runs `f`, returning its result and the lines logged on this thread
/// meanwhile (they are not printed). Captures nest: the inner one takes the
/// lines logged inside it.
pub fn capture<R>(f: impl FnOnce() -> R) -> (R, Vec<String>) {
  /// Restores the outer capture even if `f` panics.
  struct Restore(Option<Option<Vec<String>>>);
  impl Drop for Restore {
    fn drop(&mut self) {
      if let Some(outer) = self.0.take() {
        CAPTURE.with(|c| *c.borrow_mut() = outer);
      }
    }
  }
  let mut restore = Restore(Some(CAPTURE.with(|c| c.borrow_mut().replace(Vec::new()))));
  let result = f();
  let outer = restore.0.take().expect("set above");
  let lines = CAPTURE.with(|c| std::mem::replace(&mut *c.borrow_mut(), outer));
  (result, lines.unwrap_or_default())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_panicking_capture_restores_the_outer_one() {
    let ((), lines) = capture(|| {
      let inner = std::panic::catch_unwind(|| capture(|| panic!("boom")));
      assert!(inner.is_err());
      emit("after".into());
    });
    assert_eq!(lines, ["after"]);
  }
}
