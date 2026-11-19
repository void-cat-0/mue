#[path = "lib.rs"]
mod input_router;

use input_router::InputRouter;

fn main() {
    let mut router = InputRouter::new();
    let chart = 1u64;
    let ui = 2u64;

    println!("down chart -> {:?}", router.down(chart, false));
    println!("move chart -> {:?}", router.move_(chart));
    println!("up chart -> {:?}", router.up(chart));
    println!("down on chart, rejected -> {:?}", router.down(chart, false));
    println!("fallback to ui -> {:?}", router.fallback_to_ui(chart));
    println!("cancel ui-owned pointer -> {:?}", router.cancel(chart));
    println!("down ui -> {:?}", router.down(ui, true));
    println!("modal on -> cancel {:?}", router.set_modal(true));
    println!("down while modal -> {:?}", router.down(3, false));
    println!("back -> {}", router.back());
    println!(
        "chart input disabled -> {:?}",
        router.set_chart_enabled(false)
    );
    println!("down while chart disabled -> {:?}", router.down(4, false));
    println!(
        "chart input enabled -> {:?}",
        router.set_chart_enabled(true)
    );
    println!("down after modal -> {:?}", router.down(5, false));
    println!("chart input is enabled: {}", router.chart_enabled());
}
