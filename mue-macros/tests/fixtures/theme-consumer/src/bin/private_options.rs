fn main() {
    let _ = ui::theme::theme! { ui::widgets::Button[42 == 42 && { let options: &ui::widgets::ButtonOptions = panic!(); options.primary }] {} };
}
