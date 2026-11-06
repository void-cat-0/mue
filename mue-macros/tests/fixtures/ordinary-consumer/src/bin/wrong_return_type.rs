use ui::node;

#[node]
fn wrong_scalar(build: ui::node::NodeContext) -> u32 {
    let _ = build;
    0
}

fn main() {}
