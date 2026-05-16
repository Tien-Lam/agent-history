use crate::model::ContentBlock;

/// Split text into `Text` and `CodeBlock` content blocks by detecting fenced code blocks.
pub fn parse_text_with_code_blocks(text: &str) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut code_language: Option<String> = None;
    let mut code_content = String::new();

    for line in text.lines() {
        if !in_code_block && line.starts_with("```") {
            // Start of a code block
            if !current_text.is_empty() {
                blocks.push(ContentBlock::Text(current_text.trim_end().to_string()));
                current_text.clear();
            }
            let lang = line.trim_start_matches('`').trim();
            code_language = if lang.is_empty() {
                None
            } else {
                Some(lang.to_string())
            };
            code_content.clear();
            in_code_block = true;
        } else if in_code_block && line.starts_with("```") {
            // End of a code block
            blocks.push(ContentBlock::CodeBlock {
                language: code_language.take(),
                code: code_content.trim_end().to_string(),
            });
            code_content.clear();
            in_code_block = false;
        } else if in_code_block {
            if !code_content.is_empty() {
                code_content.push('\n');
            }
            code_content.push_str(line);
        } else {
            if !current_text.is_empty() {
                current_text.push('\n');
            }
            current_text.push_str(line);
        }
    }

    // Handle unclosed code block
    if in_code_block && !code_content.is_empty() {
        blocks.push(ContentBlock::CodeBlock {
            language: code_language,
            code: code_content.trim_end().to_string(),
        });
    } else if !current_text.is_empty() {
        blocks.push(ContentBlock::Text(current_text.trim_end().to_string()));
    }

    blocks
}

#[cfg(test)]
mod tests {
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
}
