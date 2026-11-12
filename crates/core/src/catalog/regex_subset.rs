// Copyright 2024 Google LLC
// Copyright 2026 A2UI Rust contributors
// SPDX-License-Identifier: Apache-2.0
//! A2UI portable regex parser and static guard, ported from the official
//! `python/a2ui_core/src/a2ui/core/common/regex_subset.py` reference.
//!
//! Only the parsed subset is translated to the matching engine. Character
//! classes use explicit scalar ranges (not the engine's Unicode shorthand).
//! Static safety checks are supplemented by engine backtracking/size limits.

use std::fmt::Write;

type Ranges = Vec<(u32, u32)>;
const MAX_CP: u32 = 0x10ffff;
const DIGITS: &[(u32, u32)] = &[(0x30, 0x39)];
const WORD: &[(u32, u32)] = &[(0x30, 0x39), (0x41, 0x5a), (0x5f, 0x5f), (0x61, 0x7a)];
const SPACE: &[(u32, u32)] = &[
    (9, 13),
    (32, 32),
    (0xa0, 0xa0),
    (0x1680, 0x1680),
    (0x2000, 0x200a),
    (0x2028, 0x2029),
    (0x202f, 0x202f),
    (0x205f, 0x205f),
    (0x3000, 0x3000),
    (0xfeff, 0xfeff),
];
const LINES: &[(u32, u32)] = &[(10, 10), (13, 13), (0x2028, 0x2029)];

#[derive(Clone, Debug)]
enum Node {
    Empty,
    Set(Ranges),
    Group(Box<Node>),
    Lookahead(bool, Box<Node>),
    Anchor(char),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        child: Box<Node>,
        min: u32,
        max: Option<u32>,
        lazy: bool,
    },
}

fn normalize(ranges: impl IntoIterator<Item = (u32, u32)>) -> Ranges {
    let mut split = Vec::new();
    for (lo, hi) in ranges {
        if lo > hi {
            continue;
        }
        if hi < 0xd800 || lo > 0xdfff {
            split.push((lo, hi));
        } else {
            if lo < 0xd800 {
                split.push((lo, 0xd7ff));
            }
            if hi > 0xdfff {
                split.push((0xe000, hi));
            }
        }
    }
    split.sort_unstable();
    let mut out: Ranges = Vec::new();
    for (lo, hi) in split {
        if let Some(last) = out.last_mut() {
            if lo <= last.1 + 1 {
                last.1 = last.1.max(hi);
                continue;
            }
        }
        out.push((lo, hi));
    }
    out
}

fn complement(ranges: &[(u32, u32)]) -> Ranges {
    let mut out = Vec::new();
    let mut next = 0;
    for (lo, hi) in normalize(ranges.iter().copied()) {
        if next < lo {
            out.push((next, lo - 1));
        }
        next = hi + 1;
    }
    if next <= MAX_CP {
        out.push((next, MAX_CP));
    }
    normalize(out)
}

fn overlap(a: &[(u32, u32)], b: &[(u32, u32)]) -> bool {
    a.iter()
        .any(|&(lo, hi)| b.iter().any(|&(l, h)| lo <= h && l <= hi))
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
}
impl Parser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }
    fn next(&mut self) -> Result<char, String> {
        let c = self.peek().ok_or("UNSUPPORTED_SYNTAX: unexpected end")?;
        self.pos += 1;
        Ok(c)
    }
    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn require(&mut self, c: char) -> Result<(), String> {
        if self.eat(c) {
            Ok(())
        } else {
            Err("UNSUPPORTED_SYNTAX: unclosed or malformed expression".into())
        }
    }
    fn disjunction(&mut self) -> Result<Node, String> {
        let mut branches = vec![self.alternative()?];
        while self.eat('|') {
            branches.push(self.alternative()?);
        }
        Ok(if branches.len() == 1 {
            branches.remove(0)
        } else {
            Node::Alt(branches)
        })
    }
    fn alternative(&mut self) -> Result<Node, String> {
        let mut nodes = Vec::new();
        while self.peek().is_some_and(|c| c != '|' && c != ')') {
            nodes.push(self.term()?);
        }
        Ok(match nodes.len() {
            0 => Node::Empty,
            1 => nodes.remove(0),
            _ => Node::Concat(nodes),
        })
    }
    fn term(&mut self) -> Result<Node, String> {
        let child = self.atom()?;
        if !self.quantifier_start() {
            return Ok(child);
        }
        if matches!(child, Node::Anchor(_) | Node::Lookahead(..)) {
            return Err("UNSUPPORTED_SYNTAX: quantified assertion".into());
        }
        let (min, max) = match self.next()? {
            '*' => (0, None),
            '+' => (1, None),
            '?' => (0, Some(1)),
            '{' => {
                let min = self.decimal()?;
                if self.eat('}') {
                    (min, Some(min))
                } else {
                    self.require(',')?;
                    if self.eat('}') {
                        (min, None)
                    } else {
                        let max = self.decimal()?;
                        self.require('}')?;
                        if max < min {
                            return Err("UNSUPPORTED_SYNTAX: reversed quantifier bounds".into());
                        }
                        (min, Some(max))
                    }
                }
            }
            _ => unreachable!(),
        };
        let lazy = self.eat('?');
        if self.quantifier_start() {
            return Err("UNSUPPORTED_SYNTAX: chained or possessive quantifier".into());
        }
        Ok(Node::Repeat {
            child: Box::new(child),
            min,
            max,
            lazy,
        })
    }
    fn quantifier_start(&self) -> bool {
        matches!(self.peek(), Some('*' | '+' | '?' | '{'))
    }
    fn decimal(&mut self) -> Result<u32, String> {
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err("UNSUPPORTED_SYNTAX: malformed quantifier".into());
        }
        let mut n = 0u32;
        while let Some(c) = self.peek().filter(|c| c.is_ascii_digit()) {
            self.pos += 1;
            n = n
                .checked_mul(10)
                .and_then(|n| n.checked_add(c as u32 - '0' as u32))
                .filter(|n| *n <= 65535)
                .ok_or("UNSUPPORTED_SYNTAX: quantifier exceeds 65535")?;
        }
        Ok(n)
    }
    fn atom(&mut self) -> Result<Node, String> {
        Ok(match self.next()? {
            '^' => Node::Anchor('^'),
            '$' => Node::Anchor('$'),
            '.' => Node::Set(complement(LINES)),
            '[' => self.class()?,
            '\\' => self.escape(false)?,
            '(' => {
                let kind = if self.eat('?') {
                    match self.next()? {
                        ':' => ':',
                        '=' => '=',
                        '!' => '!',
                        _ => return Err("UNSUPPORTED_SYNTAX: group extension".into()),
                    }
                } else {
                    ':'
                };
                let child = Box::new(self.disjunction()?);
                self.require(')')?;
                if kind == ':' {
                    Node::Group(child)
                } else {
                    Node::Lookahead(kind == '!', child)
                }
            }
            '{' | '}' | ']' | '*' | '+' | '?' => {
                return Err("UNSUPPORTED_SYNTAX: unescaped metacharacter".into());
            }
            c => Node::Set(vec![(c as u32, c as u32)]),
        })
    }
    fn hex(&mut self, count: usize) -> Result<u32, String> {
        let mut value = 0;
        for _ in 0..count {
            let c = self.next()?;
            value = value * 16
                + c.to_digit(16)
                    .ok_or("UNSUPPORTED_SYNTAX: hexadecimal escape")?;
        }
        Ok(value)
    }
    fn escape(&mut self, in_class: bool) -> Result<Node, String> {
        let c = self.next()?;
        let ranges = match c {
            'd' => Some(DIGITS.to_vec()),
            'D' => Some(complement(DIGITS)),
            'w' => Some(WORD.to_vec()),
            'W' => Some(complement(WORD)),
            's' => Some(SPACE.to_vec()),
            'S' => Some(complement(SPACE)),
            _ => None,
        };
        if let Some(r) = ranges {
            return Ok(Node::Set(r));
        }
        if (c == 'b' || c == 'B') && !in_class {
            return Ok(Node::Anchor(c));
        }
        let value = match c {
            'b' if in_class => 8,
            't' => 9,
            'n' => 10,
            'v' => 11,
            'f' => 12,
            'r' => 13,
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => 0,
            'x' => self.hex(2)?,
            'u' => {
                if self.eat('{') {
                    let start = self.pos;
                    let mut n = 0u32;
                    while self.peek().is_some_and(|c| c != '}') {
                        let digit = self
                            .next()?
                            .to_digit(16)
                            .ok_or("UNSUPPORTED_SYNTAX: Unicode escape")?;
                        n = n
                            .checked_mul(16)
                            .and_then(|n| n.checked_add(digit))
                            .filter(|n| *n <= MAX_CP)
                            .ok_or("UNSUPPORTED_SYNTAX: Unicode range")?;
                    }
                    if self.pos == start {
                        return Err("UNSUPPORTED_SYNTAX: empty Unicode escape".into());
                    }
                    self.require('}')?;
                    n
                } else {
                    self.hex(4)?
                }
            }
            c if "^$\\.*+?()[]{}|/".contains(c) || (in_class && c == '-') => c as u32,
            _ => return Err("UNSUPPORTED_SYNTAX: unsupported escape".into()),
        };
        if (0xd800..=0xdfff).contains(&value) {
            return Err("UNSUPPORTED_SYNTAX: surrogate escape".into());
        }
        Ok(Node::Set(vec![(value, value)]))
    }
    fn class_atom(&mut self) -> Result<(Option<u32>, Ranges), String> {
        if self.eat('\\') {
            let start = self.pos;
            let node = self.escape(true)?;
            let Node::Set(r) = node else {
                return Err("UNSUPPORTED_SYNTAX: class anchor".into());
            };
            // Only a literal escape can be an endpoint; even a one-character
            // shorthand set would not be a literal endpoint in the reference.
            let is_class = self.chars.get(start).is_some_and(|c| "dDwWsS".contains(*c));
            let single = if !is_class && r.len() == 1 && r[0].0 == r[0].1 {
                Some(r[0].0)
            } else {
                None
            };
            return Ok((single, r));
        }
        if self.peek() == Some('[') || (self.peek() == Some('&') && self.peek_next() == Some('&')) {
            return Err("UNSUPPORTED_SYNTAX: nested/intersection class".into());
        }
        let cp = self.next()? as u32;
        Ok((Some(cp), vec![(cp, cp)]))
    }
    fn class(&mut self) -> Result<Node, String> {
        let negated = self.eat('^');
        let mut ranges = Vec::new();
        while self.peek().is_some_and(|c| c != ']') {
            let (first, rs) = self.class_atom()?;
            if self.peek() == Some('-') && self.peek_next() != Some(']') {
                self.pos += 1;
                if self.peek() == Some('-') {
                    return Err("UNSUPPORTED_SYNTAX: class subtraction".into());
                }
                let (last, _) = self.class_atom()?;
                let (Some(lo), Some(hi)) = (first, last) else {
                    return Err("UNSUPPORTED_SYNTAX: class range endpoint".into());
                };
                if lo > hi {
                    return Err("UNSUPPORTED_SYNTAX: reversed class range".into());
                }
                ranges.push((lo, hi));
            } else {
                ranges.extend(rs);
            }
        }
        self.require(']')?;
        if ranges.is_empty() {
            return Err("UNSUPPORTED_SYNTAX: empty class".into());
        }
        let ranges = normalize(ranges);
        Ok(Node::Set(if negated {
            complement(&ranges)
        } else {
            ranges
        }))
    }
}

impl Node {
    fn children(&self) -> &[Node] {
        match self {
            Self::Group(c) | Self::Lookahead(_, c) | Self::Repeat { child: c, .. } => {
                std::slice::from_ref(c.as_ref())
            }
            Self::Concat(v) | Self::Alt(v) => v,
            _ => &[],
        }
    }
    fn unwrap(&self) -> &Self {
        match self {
            Self::Group(c) => c.unwrap(),
            _ => self,
        }
    }
    fn nullable(&self) -> bool {
        match self {
            Self::Empty | Self::Anchor(_) | Self::Lookahead(..) => true,
            Self::Set(_) => false,
            Self::Repeat { min: 0, .. } => true,
            Self::Alt(v) => v.iter().any(Self::nullable),
            _ => self.children().iter().all(Self::nullable),
        }
    }
    fn first(&self) -> Ranges {
        match self {
            Self::Set(r) => r.clone(),
            Self::Group(c) => c.first(),
            Self::Repeat { max: Some(0), .. } => vec![],
            Self::Repeat { child, .. } => child.first(),
            Self::Alt(v) => normalize(v.iter().flat_map(Self::first)),
            Self::Concat(v) => {
                let mut out = Vec::new();
                for n in v {
                    out.extend(n.first());
                    if !n.nullable() {
                        break;
                    }
                }
                normalize(out)
            }
            _ => vec![],
        }
    }
    fn chars(&self) -> Ranges {
        match self {
            Self::Set(r) => r.clone(),
            Self::Lookahead(..) | Self::Repeat { max: Some(0), .. } => vec![],
            _ => normalize(self.children().iter().flat_map(Self::chars)),
        }
    }
    fn has_unbounded(&self) -> bool {
        matches!(self, Self::Repeat { max: None, .. })
            || self.children().iter().any(Self::has_unbounded)
    }
    fn nested(&self) -> bool {
        (matches!(self, Self::Repeat { child, max, .. } if max.is_none_or(|n| n > 1) && child.has_unbounded()))
            || self.children().iter().any(Self::nested)
    }
    fn overlapping_alternation(&self) -> bool {
        if let Self::Repeat {
            child, max: None, ..
        } = self
        {
            if let Self::Alt(branches) = child.unwrap() {
                for (i, a) in branches.iter().enumerate() {
                    if branches[i + 1..]
                        .iter()
                        .any(|b| overlap(&a.first(), &b.first()))
                    {
                        return true;
                    }
                }
            }
        }
        self.children().iter().any(Self::overlapping_alternation)
    }
    fn adjacent_overlap(&self) -> bool {
        if let Self::Concat(v) = self {
            for (i, x) in v.iter().enumerate() {
                if let Self::Repeat {
                    child, max: None, ..
                } = x.unwrap()
                {
                    let chars = child.chars();
                    for y in &v[i + 1..] {
                        if let Self::Repeat {
                            child, max: None, ..
                        } = y.unwrap()
                        {
                            if overlap(&chars, &child.chars()) {
                                return true;
                            }
                        }
                        if !y.nullable() {
                            break;
                        }
                    }
                }
            }
        }
        self.children().iter().any(Self::adjacent_overlap)
    }
    fn translate(&self, out: &mut String) {
        match self {
            Self::Empty => {}
            Self::Set(r) => {
                if r.is_empty() {
                    out.push_str("[^\\x{0}-\\x{10ffff}]");
                    return;
                }
                out.push('[');
                for &(lo, hi) in r {
                    let _ = write!(out, "\\x{{{lo:x}}}");
                    if hi != lo {
                        let _ = write!(out, "-\\x{{{hi:x}}}");
                    }
                }
                out.push(']');
            }
            Self::Anchor(c) => out.push_str(match c {
                '^' => "\\A",
                '$' => "\\z",
                'b' => r"(?:(?<=[A-Za-z0-9_])(?![A-Za-z0-9_])|(?<![A-Za-z0-9_])(?=[A-Za-z0-9_]))",
                'B' => r"(?:(?<=[A-Za-z0-9_])(?=[A-Za-z0-9_])|(?<![A-Za-z0-9_])(?![A-Za-z0-9_]))",
                _ => unreachable!(),
            }),
            Self::Group(c) => {
                out.push_str("(?:");
                c.translate(out);
                out.push(')');
            }
            Self::Lookahead(neg, c) => {
                out.push_str(if *neg { "(?!" } else { "(?=" });
                c.translate(out);
                out.push(')');
            }
            Self::Concat(v) => {
                for c in v {
                    c.translate(out);
                }
            }
            Self::Alt(v) => {
                out.push_str("(?:");
                for (i, c) in v.iter().enumerate() {
                    if i > 0 {
                        out.push('|');
                    }
                    c.translate(out);
                }
                out.push(')');
            }
            Self::Repeat {
                child,
                min,
                max,
                lazy,
            } => {
                out.push_str("(?:");
                child.translate(out);
                out.push(')');
                match max {
                    Some(max) if max == min => {
                        let _ = write!(out, "{{{min}}}");
                    }
                    Some(max) => {
                        let _ = write!(out, "{{{min},{max}}}");
                    }
                    None => {
                        let _ = write!(out, "{{{min},}}");
                    }
                }
                if *lazy {
                    out.push('?');
                }
            }
        }
    }
}

/// Evaluates an A2UI portable regex. Invalid/unsafe patterns are errors; an
/// overlong input is `None`, which callers map to a failed validation result.
pub(super) fn regex_test(pattern: &str, value: &str) -> Result<Option<bool>, String> {
    if pattern.chars().count() > 256 {
        return Err("PATTERN_TOO_LONG".into());
    }
    let mut parser = Parser {
        chars: pattern.chars().collect(),
        pos: 0,
    };
    let tree = parser.disjunction()?;
    if parser.peek().is_some() {
        return Err("UNSUPPORTED_SYNTAX: unmatched closing parenthesis".into());
    }
    if tree.nested() {
        return Err("NESTED_QUANTIFIER".into());
    }
    if tree.overlapping_alternation() {
        return Err("OVERLAPPING_ALTERNATION".into());
    }
    if tree.adjacent_overlap() {
        return Err("ADJACENT_OVERLAP".into());
    }
    let mut translated = String::new();
    tree.translate(&mut translated);
    let regex = fancy_regex::RegexBuilder::new(&translated)
        .backtrack_limit(100_000)
        .delegate_size_limit(2 * 1024 * 1024)
        .delegate_dfa_size_limit(2 * 1024 * 1024)
        .build()
        .map_err(|e| format!("UNSUPPORTED_SYNTAX: {e}"))?;
    if value.chars().count() > 4096 {
        return Ok(None);
    }
    regex
        .is_match(value)
        .map(Some)
        .map_err(|e| format!("REGEX_RESOURCE_LIMIT: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonportable_and_unsafe_syntax() {
        for p in [
            r"(a)\1", "(?<=a)b", "(?<n>a)", "(?i)a", r"\p{L}", "[a&&b]", "[a--b]", r"\a", r"\uD800",
        ] {
            assert!(
                regex_test(p, "aaa")
                    .unwrap_err()
                    .contains("UNSUPPORTED_SYNTAX"),
                "{p}"
            );
        }
        for p in ["(a+)+$", "(a*){2,5}"] {
            assert_eq!(regex_test(p, "aaa"), Err("NESTED_QUANTIFIER".into()));
        }
        for p in ["(a|aa)+", "([ab]|b)*"] {
            assert_eq!(regex_test(p, "aaa"), Err("OVERLAPPING_ALTERNATION".into()));
        }
        for p in ["a*a+", "a*(?:b?)a+"] {
            assert_eq!(regex_test(p, "aaa"), Err("ADJACENT_OVERLAP".into()));
        }
    }
    #[test]
    fn portable_character_semantics() {
        for (p, s, expected) in [
            (r"^\d+$", "١", false),
            (r"^\w+$", "é", false),
            (r"\bélan\b", "élan", false),
            (r"^\s$", "\u{feff}", true),
            (r"^\s$", "\u{85}", false),
            ("^.$", "\u{2028}", false),
            ("^.$", "🦀", true),
            ("a$", "a\n", false),
            (r"\u{1F980}", "🦀", true),
            (r"[\b]", "\u{8}", true),
        ] {
            assert_eq!(regex_test(p, s), Ok(Some(expected)), "{p:?} {s:?}");
        }
    }
    #[test]
    fn allows_safe_groups_and_lookahead() {
        for (p, s, expected) in [
            (r"^(?=.*\d).{8,}$", "password1", true),
            (r"^(?=.*\d).{8,}$", "password", false),
            (r"a(?!b)", "ac", true),
            (r"a(?!b)", "ab", false),
            ("^(?:ab|cd)+$", "abcd", true),
            ("", "", true),
        ] {
            assert_eq!(regex_test(p, s), Ok(Some(expected)), "{p}");
        }
    }
    #[test]
    fn limits_and_malformed_patterns() {
        assert_eq!(
            regex_test(&"a".repeat(257), ""),
            Err("PATTERN_TOO_LONG".into())
        );
        assert_eq!(regex_test("a", &"a".repeat(4097)), Ok(None));
        for p in [
            "[",
            "[a-",
            "(",
            "a{65536}",
            "a{9999999999999999999999999}",
            "a{2,1}",
            "*",
            "a++",
            "[]",
            r"\u{}",
        ] {
            assert!(regex_test(p, "").is_err(), "{p}");
        }
    }
}
