use ui::node;

#[node]
fn stale(build: ui::node::NodeContext, #[theme] value: bool) -> ui::node::Node {
    build.finish(ui::style::Style::new(), move |_| {
        drop(value);
    })
}

fn main() {}
