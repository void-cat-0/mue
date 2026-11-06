use ui::node;

#[node]
#[theme]
fn stale(build: ui::node::NodeContext) -> ui::node::Node {
    build.finish(ui::style::Style::new(), |_| {})
}

fn main() {}
