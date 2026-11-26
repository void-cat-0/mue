//! Model binding and modifiers of the text inputs.

use mue_components::prelude::*;
use mue_core::prelude::*;
use mue_macroquad::{math::Vector, node::TextEditHandle};

fn mount(view: impl IntoNode) -> Node {
    let node = view.into_node();
    node.prepare_layout(Vector::new(400., 200.));
    node
}

#[test]
fn model_trim_and_external_writes() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let name = signal(String::from("Ada"));
        let handle = TextEditHandle::new();
        let _view = mount(input().model(name).trim().handle(handle.clone()));
        assert_eq!(
            handle.value().unwrap().text,
            "Ada",
            "the model fills the field"
        );
        handle.set_text("  Grace  ");
        assert_eq!(
            name.get_clone(),
            "Grace",
            "trim removes surrounding whitespace"
        );
        assert_eq!(
            handle.value().unwrap().text,
            "  Grace  ",
            "the field keeps what was typed"
        );
        name.set("Linus".into());
        assert_eq!(handle.value().unwrap().text, "Linus");
    });
}

#[test]
fn lazy_models_wait_for_blur() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let note = signal(String::new());
        let handle = TextEditHandle::new();
        let _view = mount(input().model(note).lazy().handle(handle.clone()));
        assert!(handle.focus());
        handle.insert("draft");
        assert_eq!(note.get_clone(), "", "lazy models do not follow each edit");
        handle.blur();
        assert_eq!(note.get_clone(), "draft");
    });
}

#[test]
fn parsed_models_mark_invalid_text() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let age = signal(30_u32);
        let changes = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = changes.clone();
        let handle = TextEditHandle::new();
        let _view = mount(
            input()
                .model_parsed(age)
                .on_change(move |_| seen.set(seen.get() + 1))
                .handle(handle.clone()),
        );
        assert_eq!(handle.value().unwrap().text, "30");
        handle.set_text("42");
        assert_eq!(age.get(), 42);
        handle.set_text("4x");
        assert_eq!(age.get(), 42, "text that does not parse leaves the model");
        age.set(7);
        assert_eq!(handle.value().unwrap().text, "7");
        assert_eq!(changes.get(), 2);
    });
}
