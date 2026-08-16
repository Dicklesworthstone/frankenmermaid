p = '/data/projects/frankenmermaid/crates/fm-parser/src/mermaid_parser.rs'
s = open(p).read()

# 1. import DetectionMethod
old = '''use crate::{
    DetectedType, FlowchartBatchParseRef, FlowchartBatchPrefix, ParseResult, ParserConfig,'''
new = '''use crate::{
    DetectedType, DetectionMethod, FlowchartBatchParseRef, FlowchartBatchPrefix, ParseResult,
    ParserConfig,'''
assert s.count(old) == 1
s = s.replace(old, new)

# 2. mask the mistyped header before dispatch
old = '''    parse_init_directives(content, &mut builder);

    match diagram_type {'''
new = '''    parse_init_directives(content, &mut builder);

    // A MISTYPED DIAGRAM HEADER MUST NOT BECOME A NODE (bd-ec1t).
    //
    // `flowchat LR` fuzzy-matches to `flowchart`, so detection already knows the line is a
    // mis-typed diagram-type declaration and says so -- "Fuzzy match: possible typo in diagram type
    // declaration" is pushed into the warnings a few lines above. The family parsers then saw a
    // line that is not an exact `flowchart`/`graph` header, fell through to the generic node
    // fallback, and interned it: the rendered SVG contained a box captioned `flowchat_LR`. The two
    // behaviours contradicted each other -- one said "this is a header you typed wrong", the other
    // said "this is a node you asked for".
    //
    // The guard keys on the FUZZY-MATCH SIGNAL rather than on a keyword list, which is what keeps
    // it from eating real content. A headerless diagram whose genuine first line is a node is
    // detected by content heuristics or falls back, never by `FuzzyKeyword`, so it is untouched;
    // and a correct header is `ExactKeyword`, so `flowchart LR` with a node called `graph` still
    // declares `graph`.
    //
    // The line is BLANKED, not removed, so every subsequent line keeps its number and every span
    // the parse records stays correct. Doing this at the single dispatch site rather than inside
    // the flowchart item loop fixes every diagram family at once -- the fallback that interned the
    // line is not flowchart-specific.
    //
    // The mistyped header's trailing direction (`LR` here) is discarded with it. That loses
    // nothing: a direction is only read off a header the parser RECOGNISED, so `flowchat LR` never
    // set one anyway, and mermaid rejects the whole input.
    let masked;
    let content = if detection.method == DetectionMethod::FuzzyKeyword {
        match first_significant_line_range(content) {
            Some((start, end)) => {
                masked = blank_range(content, start, end);
                masked.as_str()
            }
            None => content,
        }
    } else {
        content
    };

    match diagram_type {'''
assert s.count(old) == 1
s = s.replace(old, new)

# 3. helpers, beside the existing first_significant_line
old = '''pub fn first_significant_line(input: &str) -> Option<&str> {'''
new = '''/// Byte range of the line [`first_significant_line`] returns, in `input`.
///
/// Same skip rules -- blank lines, `%%` comments and `%%{...}%%` init directives -- so the two
/// cannot disagree about which line is the header. Returns the range of the RAW line, not of its
/// trimmed contents, so blanking it removes the indentation too.
fn first_significant_line_range(input: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for raw in input.split_inclusive('\\n') {
        let line_len = raw.len();
        let line = trim_fast(raw);
        if !line.is_empty()
            && !is_comment(line)
            && !line.starts_with("%%{")
            && !line.ends_with("}%%")
        {
            // Exclude the trailing newline so the line count is unchanged.
            let end = offset + raw.trim_end_matches(['\\r', '\\n']).len();
            return Some((offset, end));
        }
        offset += line_len;
    }
    None
}

/// `input` with `start..end` replaced by spaces, preserving byte length and line numbering.
fn blank_range(input: &str, start: usize, end: usize) -> String {
    let mut out = String::with_capacity(input.len());
    out.push_str(&input[..start]);
    out.extend(std::iter::repeat_n(' ', end - start));
    out.push_str(&input[end..]);
    out
}

pub fn first_significant_line(input: &str) -> Option<&str> {'''
assert s.count(old) == 1
s = s.replace(old, new)
open(p, 'w').write(s)
print('typo-header guard installed')
