use ui::theme::ClassName;
use ui::widgets::Button;

use ui::widgets::Button as ButtonAlias;
const COMPACT: ClassName = ui::theme::class!("compact-2");

fn main() {
    let local_enabled = true;
    let theme = ui::theme::theme! {
        Button.primary.!disabled[$primary && matches!($progress, 0..=4)]:hover {
            opacity: 80%;
        }
        Button[$primary], ButtonAlias.compact {
            opacity: 50%;
        }
        ui::widgets::Badge<u32, 4>[local_enabled] { opacity: 70%; }
        .selected { opacity: { if local_enabled { 0.25 } else { 0.5 } }; }
        * { opacity: 1.; }
    };
    assert_eq!(theme.rule_count(), 5);
    let _ = COMPACT;
    let _ = ui::theme::class!("ordinary-expression");
}
