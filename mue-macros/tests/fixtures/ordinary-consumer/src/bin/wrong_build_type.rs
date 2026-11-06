use ui::node;

#[node]
fn wrong(ctx: u32) -> ui::node::Node {
    let _ = ctx;
    unreachable!()
}

fn main() {}
