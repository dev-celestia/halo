//! One incremental TextView document, with Celestia code surfaces supplied by
//! the Markdown AST extension hooks. No second parser or fence splitting.
use celestia_ui::components::CodeBlock;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::text::{
    MarkdownExtensions, MarkdownNode, MarkdownParseContext, SelectionFormat, TextView,
    TextViewState, TextViewStyle,
};
use markdown::mdast::Node;

use super::TEXT_SIZE;
use super::transcript_state::preview_end;

const CODE_NODE: &str = "halo-agent-code";
/// Same rhythm as [`TextViewStyle::paragraph_gap`]. Custom fences sit outside
/// that gap inside lists (`is_last`), so the CodeBlock supplies its own inset.
const BLOCK_FLOW_GAP: Rems = rems(0.75);

#[derive(Clone, Debug)]
pub struct CodeSource {
    pub language: Option<String>,
    pub body: SharedString,
    pub preview: SharedString,
}

pub fn parse_code(node: &Node, context: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
    let Node::Code(code) = node else {
        return None;
    };
    let body: SharedString = code.value.clone().into();
    let end = preview_end(&body);
    let preview = if end == body.len() {
        body.clone()
    } else {
        body[..end].to_owned().into()
    };
    Some(
        MarkdownNode::new(
            CODE_NODE,
            CodeSource {
                language: code.lang.clone(),
                body: body.clone(),
                preview,
            },
        )
        .text(body)
        // Custom block selection is atomic; preserve the original delimiters,
        // language and whitespace rather than reconstructing a triple fence.
        .markdown(context.node_source(node).unwrap_or_default().to_owned()),
    )
}

/// Construct once per retained text entity. Cloning this registry preserves its
/// revision and parser handles across tokens, selection, resize and theme changes.
pub fn extensions(id: SharedString) -> MarkdownExtensions {
    MarkdownExtensions::default()
        .block_parser(parse_code)
        .block_renderer(CODE_NODE, move |node, _, _| render_code(&id, node))
}

fn render_code(id: &SharedString, node: &MarkdownNode) -> AnyElement {
    let Some(code) = node.data::<CodeSource>() else {
        return Empty.into_any_element();
    };
    let copy = code.body.clone();
    let offset = node.source_range().map_or(0, |range| range.start);
    let mut block = CodeBlock::new(
        SharedString::from(format!("{id}:code:{offset}")),
        code.preview.clone(),
    )
    .on_copy(move |_, _, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(copy.to_string()));
    });
    if let Some(language) = &code.language {
        block = block.language(language.clone());
    }
    let truncated = code.preview.len() < code.body.len();
    div()
        .w_full()
        .min_w_0()
        .py(BLOCK_FLOW_GAP)
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(block)
        .when(truncated, |el| {
            el.child(
                div().text_xs().child(
                    "Large code preview limited to 200 lines / 24 KiB. Copy includes the complete code.",
                ),
            )
        })
        .into_any_element()
}

pub fn render_reply(state: &Entity<TextViewState>, extensions: MarkdownExtensions) -> TextView {
    let mut style = TextViewStyle::default().paragraph_gap(BLOCK_FLOW_GAP);
    style.heading_base_font_size = TEXT_SIZE;
    style = style.heading_font_size(|level, base| match level {
        1 => base + px(4.),
        2 => base + px(2.),
        _ => base,
    });
    TextView::new(state)
        .markdown_extensions(extensions)
        .style(style)
        .selectable(true)
        .selection_format(SelectionFormat::Source)
        // No hidden stream-fade task: only the mounted current reasoning/header
        // animates, using Celestia's existing reduced-motion policy.
        .stream_fade(false)
        .w_full()
        .min_w_0()
}
