//! Headless checks for layer hosts, teleports, z-index, and anchored content.

use mue_core::prelude::*;
use mue_macroquad::{
    anchor::{Placement, anchored},
    event::hit_test::hit_test,
    layer::{Layer, LayerHost, layer_host, teleport},
    math::{Rect, Vector},
    node::{IntoNode, NodeHandle, NodeRef, flexbox},
    style::{Styleable, StyleableExt},
};

fn rect(node: NodeRef) -> Rect {
    node.rect().unwrap().get_untracked()
}

#[test]
fn teleported_content_is_drawn_by_the_host_and_removed_with_its_placeholder() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let show = signal(true);
        let host_probe = std::rc::Rc::new(std::cell::RefCell::new(None));
        let probe = host_probe.clone();
        let root = layer_host(
            flexbox().children(mue_macroquad::node::keyed(show, move |show| {
                let probe = probe.clone();
                if *show {
                    teleport(Layer::Modal, move || {
                        *probe.borrow_mut() = LayerHost::current();
                        flexbox().width(50.).height(50.)
                    })
                    .into_node()
                } else {
                    mue_macroquad::node::Node::empty()
                }
            })),
        )
        .into_node();
        root.prepare_layout(Vector::new(300., 200.));
        let host = host_probe.borrow().clone().expect("teleport sees the host");
        assert_eq!(host.len(), 1);
        let children = root.children().get_clone_untracked();
        assert_eq!(children.len(), 2, "content plus one surface");
        let surface = children[1];
        assert_eq!(
            rect(surface),
            Rect::new(0., 0., 300., 200.),
            "surfaces cover the host"
        );
        show.set(false);
        assert_eq!(
            host.len(),
            0,
            "removing the placeholder removes the surface"
        );
        assert_eq!(root.children().get_clone_untracked().len(), 1);
    });
}

#[test]
fn surfaces_are_pointer_transparent_outside_their_content() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let root = layer_host(
            flexbox()
                .width(300.)
                .height(200.)
                .on_pointer_event(|_| {})
                .children(teleport(Layer::Popover, || {
                    flexbox().width(40.).height(40.).on_pointer_event(|_| {})
                })),
        )
        .into_node();
        root.prepare_layout(Vector::new(300., 200.));
        let children = root.children().get_clone_untracked();
        let (content, surface) = (children[0], children[1]);
        let popover = surface.children().get_clone_untracked()[0];
        let inside = hit_test(*root, Vector::new(20., 20.));
        assert_eq!(inside.first(), Some(&popover), "popover is on top");
        let outside = hit_test(*root, Vector::new(200., 150.));
        assert_eq!(
            outside.first(),
            Some(&content),
            "the empty surface passes through"
        );
    });
}

#[test]
fn z_index_reorders_siblings_for_hit_testing() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let tile = || {
            flexbox()
                .absolute()
                .width(50.)
                .height(50.)
                .on_pointer_event(|_| {})
        };
        let root = flexbox()
            .width(100.)
            .height(100.)
            .children((tile().z_index(1), tile()))
            .into_node();
        root.prepare_layout(Vector::new(100., 100.));
        let children = root.children().get_clone_untracked();
        let hit = hit_test(*root, Vector::new(10., 10.));
        assert_eq!(
            hit.first(),
            Some(&children[0]),
            "higher z_index wins over later sibling"
        );
    });
}

#[test]
fn anchored_content_converges_beside_its_anchor() {
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let trigger = NodeHandle::new();
        let root = layer_host(
            flexbox().children((
                flexbox()
                    .margin_left(100.)
                    .margin_top(60.)
                    .width(80.)
                    .height(20.)
                    .node_handle(trigger),
                teleport(Layer::Popover, move || {
                    anchored(trigger)
                        .placement(Placement::BottomStart)
                        .offset(4.)
                        .children(flexbox().width(120.).height(30.))
                }),
            )),
        )
        .into_node();
        root.prepare_layout(Vector::new(400., 300.));
        let anchor = trigger.rect().unwrap();
        assert_eq!(anchor, Rect::new(100., 60., 80., 20.));
        let surface = root.children().get_clone_untracked()[1];
        let floating = surface.children().get_clone_untracked()[0];
        assert_eq!(rect(floating), Rect::new(100., 84., 120., 30.));
    });
}

#[test]
fn layer_host_keeps_the_content_as_the_rem_root() {
    use mue_macroquad::style::FontSize;
    let owner = mue_core::scope::OwnedScope::new();
    owner.run(|| {
        let root = layer_host(
            flexbox()
                .font_size(FontSize::Px(20.))
                .children(flexbox().font_size(FontSize::Rem(0.5))),
        )
        .into_node();
        root.prepare_layout(Vector::new(100., 100.));
        let content = root.children().get_clone_untracked()[0];
        let leaf = content.children().get_clone_untracked()[0];
        assert_eq!(
            leaf.font_size(),
            Some(10.),
            "rem resolves against the content root"
        );
    });
}
