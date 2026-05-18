use super::*;

#[test]
fn parse_text_with_no_code_blocks() {
    let blocks = parse_text_with_code_blocks("Hello world\nSecond line");
    assert_eq!(blocks.len(), 1);
    assert!(matches!(&blocks[0], ContentBlock::Text(t) if t == "Hello world\nSecond line"));
}

#[test]
fn parse_text_with_single_code_block() {
    let input = "Before\n```rust\nfn main() {}\n```\nAfter";
    let blocks = parse_text_with_code_blocks(input);
    assert_eq!(blocks.len(), 3);
    assert!(matches!(&blocks[0], ContentBlock::Text(t) if t == "Before"));
    assert!(
        matches!(&blocks[1], ContentBlock::CodeBlock { language, code } if language.as_deref() == Some("rust") && code == "fn main() {}")
    );
    assert!(matches!(&blocks[2], ContentBlock::Text(t) if t == "After"));
}

#[test]
fn parse_text_with_no_language_code_block() {
    let input = "```\nsome code\n```";
    let blocks = parse_text_with_code_blocks(input);
    assert_eq!(blocks.len(), 1);
    assert!(
        matches!(&blocks[0], ContentBlock::CodeBlock { language, code } if language.is_none() && code == "some code")
    );
}
