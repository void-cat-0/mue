use ui::node::NodeContext;
use ui::style::Style;
use ui::{component, node};

#[node]
fn missing() {
    unreachable!()
}

#[node]
fn wrong_first_type(value: bool, ctx: NodeContext) {
    let _ = (value, ctx);
}

#[node]
fn defaulted(#[default] ctx: NodeContext) {
    let _ = ctx;
}

#[node]
fn modeled(#[model] cx: NodeContext) {
    let _ = cx;
}

#[component]
fn component_style(style: Style) {
    unreachable!()
}

#[node]
fn node_style(other: NodeContext, style: Style) {
    unreachable!()
}

#[node]
fn default_build(ctx: NodeContext, #[default(ctx)] value: bool) {
    unreachable!()
}

#[node]
fn default_context(style: NodeContext, #[default(style)] value: bool) {
    unreachable!()
}

fn main() {}
