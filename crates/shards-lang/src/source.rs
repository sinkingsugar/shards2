//! Source text and positions. The frontend owns its sources: nothing is
//! registered globally, and every span refers to one [`Source`].

/// A byte range in a source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
  pub start: usize,
  pub end: usize,
}

impl Span {
  pub fn new(start: usize, end: usize) -> Span {
    Span { start, end }
  }

  /// The smallest span covering both.
  pub fn to(self, other: Span) -> Span {
    Span::new(self.start.min(other.start), self.end.max(other.end))
  }
}

/// One source file (or string) with its line index.
pub struct Source {
  pub name: String,
  pub text: String,
  line_starts: Vec<usize>,
}

impl Source {
  pub fn new(name: impl Into<String>, text: impl Into<String>) -> Source {
    let text = text.into();
    let mut line_starts = vec![0];
    line_starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    Source {
      name: name.into(),
      text,
      line_starts,
    }
  }

  /// 1-based line and column (in characters) of a byte offset.
  pub fn line_col(&self, offset: usize) -> (u32, u32) {
    let offset = offset.min(self.text.len());
    let line = match self.line_starts.binary_search(&offset) {
      Ok(i) => i,
      Err(i) => i - 1,
    };
    let start = self.line_starts[line];
    let column = self.text[start..offset].chars().count() + 1;
    (line as u32 + 1, column as u32)
  }

  /// `name:line:column`, for messages that point at another position.
  pub fn position(&self, offset: usize) -> String {
    let (line, column) = self.line_col(offset);
    format!("{line}:{column}")
  }

  pub fn slice(&self, span: Span) -> &str {
    &self.text[span.start..span.end]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn line_and_column_are_one_based_and_count_characters() {
    let s = Source::new("t", "ab\n\u{e9}cd\n");
    assert_eq!(s.line_col(0), (1, 1));
    assert_eq!(s.line_col(3), (2, 1));
    // é is two bytes, one column.
    assert_eq!(s.line_col(5), (2, 2));
    assert_eq!(s.line_col(s.text.len()), (3, 1));
  }
}
