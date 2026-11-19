use input_router::InputRouter;

fn main() {
    let mut router = InputRouter::new();
    let chart = 1u64;
    let ui = 2u64;

    println!("down chart -> {:?}", router.down(chart, false));
    println!("move chart -> {:?}", router.move_(chart));
    println!("down ui -> {:?}", router.down(ui, true));
    println!("modal on -> cancel {:?}", router.set_modal(true));
    println!("down while modal -> {:?}", router.down(3, false));
    println!("back -> {}", router.back());
    println!("down after modal -> {:?}", router.down(4, false));
}
