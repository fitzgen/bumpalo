#![cfg(feature = "collections")]
use crate::quickcheck;
use ::quickcheck::{Arbitrary, Gen};
use bumpalo::{collections::String, format, Bump};
use std::fmt::Write;

const MAX_STRING_OPS: usize = 64;
const MAX_SEGMENT_CHARS: usize = 8;

#[derive(Clone, Debug)]
struct SmallText(std::string::String);

impl SmallText {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Arbitrary for SmallText {
    fn arbitrary(g: &mut Gen) -> Self {
        SmallText(
            std::string::String::arbitrary(g)
                .chars()
                .take(MAX_SEGMENT_CHARS)
                .collect(),
        )
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(
            self.0.shrink().map(|s: std::string::String| {
                SmallText(s.chars().take(MAX_SEGMENT_CHARS).collect())
            }),
        )
    }
}

#[derive(Clone, Debug)]
enum StringOp {
    PushChar(char),
    PushStr(SmallText),
    WriteStr(SmallText),
    Pop,
    Truncate(usize),
    ShrinkToFit,
}

impl Arbitrary for StringOp {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 6 {
            0 => StringOp::PushChar(char::arbitrary(g)),
            1 => StringOp::PushStr(SmallText::arbitrary(g)),
            2 => StringOp::WriteStr(SmallText::arbitrary(g)),
            3 => StringOp::Pop,
            4 => StringOp::Truncate(usize::arbitrary(g)),
            _ => StringOp::ShrinkToFit,
        }
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(std::iter::empty())
    }
}

#[derive(Clone, Debug)]
struct StringProgram(std::vec::Vec<StringOp>);

impl Arbitrary for StringProgram {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut ops = std::vec::Vec::<StringOp>::arbitrary(g);
        ops.truncate(MAX_STRING_OPS);
        StringProgram(ops)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        Box::new(self.0.shrink().map(|mut ops| {
            ops.truncate(MAX_STRING_OPS);
            StringProgram(ops)
        }))
    }
}

fn char_boundary_at(s: &str, char_count: usize) -> usize {
    s.char_indices()
        .nth(char_count)
        .map(|(idx, _)| idx)
        .unwrap_or_else(|| s.len())
}

fn apply_string_op(actual: &mut String<'_>, expected: &mut std::string::String, op: &StringOp) {
    match op {
        StringOp::PushChar(ch) => {
            actual.push(*ch);
            expected.push(*ch);
        }
        StringOp::PushStr(text) => {
            actual.push_str(text.as_str());
            expected.push_str(text.as_str());
        }
        StringOp::WriteStr(text) => {
            write!(actual, "{}", text.as_str()).unwrap();
            write!(expected, "{}", text.as_str()).unwrap();
        }
        StringOp::Pop => {
            assert_eq!(actual.pop(), expected.pop());
        }
        StringOp::Truncate(n) => {
            let target_chars = n % (expected.chars().count() + 1);
            let new_len = char_boundary_at(expected.as_str(), target_chars);
            actual.truncate(new_len);
            expected.truncate(new_len);
        }
        StringOp::ShrinkToFit => {
            actual.shrink_to_fit();
            expected.shrink_to_fit();
        }
    }

    assert_eq!(actual.as_str(), expected.as_str());
    assert_eq!(actual.as_bytes(), expected.as_bytes());
    assert!(actual.capacity() >= actual.len());
}

#[test]
fn format_a_bunch_of_strings() {
    let b = Bump::new();
    let mut s = String::from_str_in("hello", &b);
    for i in 0..1000 {
        write!(&mut s, " {}", i).unwrap();
    }
}

#[test]
fn trailing_comma_in_format_macro() {
    let b = Bump::new();
    let v = format![in &b, "{}{}", 1, 2, ];
    assert_eq!(v, "12");
}

#[test]
fn push_str() {
    let b = Bump::new();
    let mut s = String::new_in(&b);
    s.push_str("abc");
    assert_eq!(s, "abc");
    s.push_str("def");
    assert_eq!(s, "abcdef");
    s.push_str("");
    assert_eq!(s, "abcdef");
    s.push_str(&"x".repeat(4000));
    assert_eq!(s.len(), 4006);
    s.push_str("ghi");
    assert_eq!(s.len(), 4009);
    assert_eq!(&s[s.len() - 5..], "xxghi");
}

quickcheck! {
    fn string_operation_sequences_match_std(program: StringProgram) -> () {
        let bump = Bump::new();
        let mut actual = String::new_in(&bump);
        let mut expected = std::string::String::new();

        for op in &program.0 {
            apply_string_op(&mut actual, &mut expected, op);
        }

        assert_eq!(actual.as_str(), expected.as_str());
    }
}
