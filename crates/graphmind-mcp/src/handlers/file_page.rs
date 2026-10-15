//! Paging for `gm_file`: one call returns a window of the file small enough for MCP clients to
//! pass straight to the model, and says where the next window starts.

use serde_json::Value;

use crate::formatting::floor_char_boundary;

/// Most bytes of file content `gm_file` returns per call. MCP clients may save a tool result over
/// their output limit to a file and give the model a message naming the path instead, which costs
/// an extra read of an oversized file. Common limits: 25,000 tokens by default, 50,000 characters,
/// and a warning above 10,000 tokens. 40,000 bytes is about 10-13k
/// tokens of source code, so ordinary pages may still draw that warning, but every page stays
/// under the 50,000-character threshold, and CJK text (3 bytes per character, about 13,000
/// characters) stays under the 25,000-token default.
pub(crate) const MAX_FILE_BYTES: usize = 40_000;

/// Where a `gm_file` page starts and how many lines it may hold, from the tool arguments.
pub(crate) struct PageRequest {
    /// 1-based line the page starts on.
    offset: usize,
    /// 1-based byte column within the `offset` line the page starts at.
    column: usize,
    /// Most lines to return; `None` for no line limit (the byte cap still applies).
    limit: Option<usize>,
}

impl PageRequest {
    /// Reads `offset`, `column` and `limit` (0 = no line limit). A present value that isn't a
    /// non-negative integer, or a 0 `offset` or `column`, is an error naming the parameter, so a
    /// caller that sends `"801"` gets told instead of silently getting page 1 again.
    pub(crate) fn from_args(args: &Value) -> Result<Self, String> {
        Ok(Self {
            offset: uint_arg(args, "offset", Some("line number"))?.unwrap_or(1),
            column: uint_arg(args, "column", Some("byte column within the offset line"))?.unwrap_or(1),
            limit: uint_arg(args, "limit", None)?.filter(|&n| n > 0),
        })
    }
}

/// The optional integer argument `name`, `None` when absent or null. With `one_based` (what the
/// number counts), 0 is rejected too.
fn uint_arg(args: &Value, name: &str, one_based: Option<&str>) -> Result<Option<usize>, String> {
    let value = match args.get(name) {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let n = value
        .as_u64()
        .ok_or_else(|| format!("Invalid parameter {name}: expected a non-negative integer, got {value}"))?;
    if let (0, Some(what)) = (n, one_based) {
        return Err(format!("Invalid parameter {name}: 0; it is a 1-based {what}, so the first is 1"));
    }
    Ok(Some(usize::try_from(n).unwrap_or(usize::MAX)))
}

/// One window of a file, as returned by [`page`].
#[derive(Debug)]
pub(crate) struct Page<'a> {
    /// The file content in the window, byte-for-byte.
    pub(crate) text: &'a str,
    /// First line in the window (1-based).
    pub(crate) first: usize,
    /// Last line in the window, whole or in part.
    pub(crate) last: usize,
    /// Lines in the file.
    pub(crate) total: usize,
    /// Where the next window starts, as (line, byte column), both 1-based; `None` when this one
    /// reaches the end of the file.
    pub(crate) next: Option<(usize, usize)>,
}

/// The window of `content` that starts at `req`'s line and column and holds at most `req`'s line
/// limit and at most `max_bytes` bytes. It ends after the last whole line that fits; when the
/// first line alone doesn't fit (minified code), it ends inside that line on a char boundary and
/// `next` points to the column to resume from. Errors when the start is past the end of the file
/// or of its line, or inside a multi-byte character.
pub(crate) fn page<'a>(content: &'a str, req: &PageRequest, max_bytes: usize) -> Result<Page<'a>, String> {
    // A window smaller than one character couldn't make progress.
    debug_assert!(max_bytes >= 4, "max_bytes must hold any UTF-8 character");
    let total = content.bytes().filter(|&b| b == b'\n').count()
        + usize::from(!content.is_empty() && !content.ends_with('\n'));
    let first = req.offset;
    if first > total.max(1) {
        return Err(format!("offset={first} is past the end of the file ({total} lines)"));
    }
    let mut lines = content.split_inclusive('\n');
    let line_start: usize = lines.by_ref().take(first - 1).map(str::len).sum();
    let line_len = lines.next().map_or(0, str::len);
    let skip = req.column - 1;
    if skip > 0 && skip >= line_len {
        return Err(format!("column={} is past the end of line {first} ({line_len} bytes)", req.column));
    }
    let start = line_start + skip;
    if !content.is_char_boundary(start) {
        return Err(format!("column={} falls inside a multi-byte character", req.column));
    }

    let (mut end, mut last, mut next) = (start, first - 1, None);
    for line in content[start..].split_inclusive('\n').take(req.limit.unwrap_or(usize::MAX)) {
        if end - start + line.len() > max_bytes {
            if last < first {
                end = floor_char_boundary(content, start + max_bytes);
                last = first;
                next = Some((first, end - line_start + 1));
            }
            break;
        }
        end += line.len();
        last += 1;
    }
    if next.is_none() && last < total {
        next = Some((last + 1, 1));
    }
    Ok(Page { text: &content[start..end], first, last, total, next })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(offset: usize, column: usize, limit: Option<usize>) -> PageRequest {
        PageRequest { offset, column, limit }
    }

    /// Reads the whole file page by page, following `next`, and returns the pages.
    fn read_all<'a>(content: &'a str, limit: Option<usize>, max_bytes: usize) -> Vec<Page<'a>> {
        let mut pages = Vec::new();
        let (mut line, mut column) = (1, 1);
        loop {
            let p = page(content, &req(line, column, limit), max_bytes).unwrap();
            assert!(p.text.len() <= max_bytes, "page over the cap: {:?}", p.text);
            let next = p.next;
            pages.push(p);
            match next {
                Some(n) => (line, column) = n,
                None => return pages,
            }
            assert!(pages.len() <= content.len() + 1, "paging doesn't progress");
        }
    }

    #[test]
    fn a_small_file_is_one_page_without_next() {
        let p = page("a\nb\n", &req(1, 1, None), 100).unwrap();
        assert_eq!((p.text, p.first, p.last, p.total, p.next), ("a\nb\n", 1, 2, 2, None));
    }

    #[test]
    fn a_large_file_is_cut_after_a_whole_line() {
        // 10 lines of 10 bytes, 35-byte window: 3 whole lines fit.
        let content: String = (0..10).map(|i| format!("line{i:05}\n")).collect();
        let p = page(&content, &req(1, 1, None), 35).unwrap();
        assert_eq!((p.text, p.last, p.total, p.next), (&content[..30], 3, 10, Some((4, 1))));
        let p = page(&content, &req(4, 1, None), 35).unwrap();
        assert_eq!((&p.text[..9], p.first, p.next), ("line00003", 4, Some((7, 1))));
    }

    #[test]
    fn a_line_longer_than_the_window_continues_by_column() {
        let content = format!("{}\nnext\n", "x".repeat(25));
        let p = page(&content, &req(1, 1, None), 10).unwrap();
        assert_eq!((p.text, p.last, p.next), ("x".repeat(10).as_str(), 1, Some((1, 11))));
        let p = page(&content, &req(1, 11, None), 10).unwrap();
        assert_eq!((p.text, p.next), ("x".repeat(10).as_str(), Some((1, 21))));
        // The rest of the line fits, "next\n" doesn't: cut after the line.
        let p = page(&content, &req(1, 21, None), 10).unwrap();
        assert_eq!((p.text, p.first, p.last, p.next), ("xxxxx\n", 1, 1, Some((2, 1))));
        let p = page(&content, &req(2, 1, None), 10).unwrap();
        assert_eq!((p.text, p.next), ("next\n", None));
    }

    #[test]
    fn multibyte_text_is_cut_on_a_char_boundary_within_the_byte_cap() {
        let content = "日本語".repeat(20_000);
        let p = page(&content, &req(1, 1, None), MAX_FILE_BYTES).unwrap();
        assert!(p.text.len() <= MAX_FILE_BYTES);
        assert_eq!(p.text.len(), MAX_FILE_BYTES / 3 * 3);
        assert_eq!(p.next, Some((1, p.text.len() + 1)));
        // 3-byte characters in a 10-byte window: 9 bytes, then resume after them.
        let p = page("日本語日本語", &req(1, 1, None), 10).unwrap();
        assert_eq!((p.text, p.next), ("日本語", Some((1, 10))));
        let p = page("日本語日本語", &req(1, 10, None), 10).unwrap();
        assert_eq!((p.text, p.next), ("日本語", None));
    }

    #[test]
    fn an_empty_file_is_one_empty_page() {
        let p = page("", &req(1, 1, None), 10).unwrap();
        assert_eq!((p.text, p.last, p.total, p.next), ("", 0, 0, None));
        assert!(page("", &req(2, 1, None), 10).unwrap_err().contains("past the end"));
    }

    #[test]
    fn limit_caps_lines_and_zero_means_no_limit() {
        let content = "1\n2\n3\n4\n";
        let p = page(content, &req(2, 1, Some(2)), 100).unwrap();
        assert_eq!((p.text, p.first, p.last, p.next), ("2\n3\n", 2, 3, Some((4, 1))));
        let unlimited = PageRequest::from_args(&json!({ "limit": 0 })).unwrap();
        assert_eq!(page(content, &unlimited, 100).unwrap().text, content);
    }

    #[test]
    fn a_long_last_line_ends_without_next() {
        let content = format!("a\n{}", "x".repeat(15));
        let p = page(&content, &req(2, 1, None), 10).unwrap();
        assert_eq!((p.text.len(), p.last, p.total, p.next), (10, 2, 2, Some((2, 11))));
        let p = page(&content, &req(2, 11, None), 10).unwrap();
        assert_eq!((p.text, p.last, p.next), ("xxxxx", 2, None));
    }

    #[test]
    fn a_file_without_a_trailing_newline_pages_to_its_last_line() {
        let content = "aaaa\nbbbb\ncccc";
        let p = page(content, &req(1, 1, None), 10).unwrap();
        assert_eq!((p.text, p.last, p.total, p.next), ("aaaa\nbbbb\n", 2, 3, Some((3, 1))));
        let p = page(content, &req(3, 1, None), 10).unwrap();
        assert_eq!((p.text, p.last, p.next), ("cccc", 3, None));
    }

    #[test]
    fn reading_every_page_reproduces_the_file() {
        // Short lines, a CJK line over the window, short multibyte lines (two fit in 16
        // characters, not in 16 bytes), a line whose newline alone spills into the next window, a
        // long last line without a newline.
        let content = format!(
            "a\nbb\n{}\n日本語é\n日本語é\n{}\nccc\n{}",
            "日本語é".repeat(7),
            "y".repeat(16),
            "z".repeat(40)
        );
        for limit in [None, Some(1), Some(3)] {
            let pages = read_all(&content, limit, 16);
            let joined: String = pages.iter().map(|p| p.text).collect();
            assert_eq!(joined, content, "limit {limit:?}");
            assert_eq!(pages.last().unwrap().last, 8);
        }
    }

    #[test]
    fn a_start_past_the_line_or_inside_a_character_is_an_error() {
        assert!(page("a\nb\n", &req(3, 1, None), 10).unwrap_err().contains("offset=3"));
        assert!(page("abc\n", &req(1, 5, None), 10).unwrap_err().contains("past the end of line 1"));
        assert!(page("日本\n", &req(1, 2, None), 10).unwrap_err().contains("multi-byte"));
    }

    #[test]
    fn parameters_must_be_non_negative_integers_and_offset_column_1_based() {
        for (name, bad) in [
            ("offset", json!("801")),
            ("offset", json!(801.0)),
            ("limit", json!(-5)),
            ("limit", json!("10")),
            ("column", json!(1.5)),
            ("offset", json!(0)),
            ("column", json!(0)),
        ] {
            let err = PageRequest::from_args(&json!({ name: bad })).err().unwrap();
            assert!(err.starts_with(&format!("Invalid parameter {name}:")), "{name}={bad}: {err}");
        }
        let r = PageRequest::from_args(&json!({ "offset": 801, "column": null })).unwrap();
        assert_eq!((r.offset, r.column, r.limit), (801, 1, None));
    }
}
