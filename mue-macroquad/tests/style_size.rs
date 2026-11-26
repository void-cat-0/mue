//! Builders move their `Style` by value through nested view expressions, so its
//! size directly affects stack use (notably in debug builds).

#[test]
fn style_stays_small_as_properties_are_added() {
    let size = std::mem::size_of::<mue_macroquad::style::Style>();
    assert!(
        size <= 512,
        "Style is {size} bytes; keep properties in the sparse store"
    );
}
