use mue_components::{prelude::*, style};
use mue_core::{prelude::*, prop::Prop};
use mue_macroquad::{
    color::Color,
    math::{Matrix, Rect, Vector},
    style::{FontSize, Size, Transform, TransformOp, TransformOrigin},
};

#[test]
fn style_shorthands_follow_style_order_and_cascade() {
    let mut style = style! {
        padding: 1px 2px 3px 4px;
        padding-left: 9px;
        margin: 5px auto;
        gap: 10px 20px;
        width: 10px;
        width: 50%;
        border-width: 1px 2px 3px;
        inset: 0 auto;
    };
    let padding = style.take_padding().get();
    assert_eq!(padding.top, Size::Px(1.));
    assert_eq!(padding.right, Size::Px(2.));
    assert_eq!(padding.bottom, Size::Px(3.));
    assert_eq!(padding.left, Size::Px(9.));
    let margin = style.take_margin().get();
    assert_eq!(margin.top, Size::Px(5.));
    assert_eq!(margin.right, Size::Auto);
    assert_eq!(style.take_row_gap().get(), Size::Px(10.));
    assert_eq!(style.take_column_gap().get(), Size::Px(20.));
    assert_eq!(style.take_width().get(), Size::Percent(50.));
    let border = style.take_border().get();
    assert_eq!(border.bottom, Size::Px(3.));
    assert_eq!(border.left, Size::Px(2.));
    let inset = style.take_inset().get();
    assert_eq!(inset.right, Size::Auto);
    let mut style = style! { padding-left: 100px; padding: 3px; };
    let padding = style.take_padding().get();
    assert_eq!(padding.left, Size::Px(3.));
}

#[test]
fn transition_declarations_accept_ease_strength_and_spring_bounce() {
    use mue_macroquad::transition::Transition;

    let ease = style! { transition: 240ms ease-in-out 0.5; }
        .take_transition()
        .get();
    assert_eq!(ease, Transition::ease_in_out_with_strength(0.24, 0.5));

    let spring = style! { transition: 480ms spring 0.35; }
        .take_transition()
        .get();
    assert_eq!(spring, Transition::spring(0.48, 0.35));
    assert_eq!(Transition::ease_with_strength(1., 0.).sample(0.25), 0.25);
}

#[test]
fn expressions_remain_reactive_and_shorthands_evaluate_once() {
    let scope = Scope::new();
    scope.run(|| {
        let width = signal(24f32);
        let calls = std::cell::Cell::new(0);
        let __style = 6f32;
        let __value_0 = 12f32;
        let transform = signal(Transform::default());
        let mut style = style! {
            width: {width};
            padding: { {calls.set(calls.get() + 1); __style} } {__value_0};
            overflow: { {calls.set(calls.get() + 1); mue_macroquad::style::values::Overflow::Hidden} };
            transform: {transform};
        };
        let prop = style.take_width();
        assert_eq!(calls.get(), 2);
        let padding = style.take_padding().get();
        assert_eq!(padding.top, Size::Px(6.));
        assert_eq!(padding.left, Size::Px(12.));
        assert_eq!(prop.get(), Size::Px(24.));
        width.set(60.);
        assert_eq!(prop.get(), Size::Px(60.));
        let transform_prop = style.take_transform_list();
        let moved = Transform(vec![TransformOp::Translate(
            Size::Percent(50.),
            Size::Px(0.),
        )]);
        transform.set(moved.clone());
        assert_eq!(transform_prop.get_clone(), moved);
    });
    scope.dispose();
}

#[test]
fn transform_animation_interpolates_from_identity_and_checks_units() {
    use mue_macroquad::transition::Interpolatable;

    let none = Transform::default();
    let end = style! { transform: translate(50%, 20px) rotate(90deg) scale(2); }
        .take_transform_list()
        .get_clone();
    assert!(Transform::can_interpolate(&none, &end));
    let middle = Transform::interpolate(&none, &end, 0.5);
    assert_eq!(
        middle.0[0],
        TransformOp::Translate(Size::Percent(25.), Size::Px(10.))
    );
    assert_eq!(
        middle.0[1],
        TransformOp::Rotate(std::f32::consts::FRAC_PI_4)
    );
    assert_eq!(middle.0[2], TransformOp::Scale(1.5, 1.5));
    let different_units = style! { transform: translate(50px, 20px) rotate(90deg) scale(2); }
        .take_transform_list()
        .get_clone();
    assert!(!Transform::can_interpolate(&end, &different_units));
    assert_eq!(
        Transform::interpolate(&end, &different_units, 0.5),
        different_units
    );
    assert!(!Transform::can_interpolate(
        &end,
        &Transform(vec![TransformOp::Scale(2., 2.)])
    ));
}

#[test]
fn transform_supports_3d_functions_and_checks_each_length_unit() {
    use mue_macroquad::transition::Interpolatable;

    let mut style = style! {
        transform: perspective(600px) rotateX(45deg) rotateY(20deg)
            translate3d(12px, 20px, 30px) scale3d(1, 2, .5);
    };
    let transform = style.take_transform_list().get_clone();
    assert!(matches!(transform.0[0], TransformOp::Perspective(600.)));
    assert!(matches!(transform.0[1], TransformOp::RotateX(_)));
    assert!(matches!(transform.0[2], TransformOp::RotateY(_)));
    assert!(matches!(transform.0[3], TransformOp::Translate3d(..)));
    assert!(matches!(transform.0[4], TransformOp::Scale3d(1., 2., 0.5)));
    let projected = transform.resolve(
        Rect::new(10., 20., 120., 80.),
        TransformOrigin(Size::Percent(50.), Size::Percent(50.)),
    );
    assert!(projected.to_cols_array().iter().all(|v| v.is_finite()));

    let px = Transform(vec![TransformOp::Translate3d(
        Size::Px(4.),
        Size::Px(8.),
        12.,
    )]);
    let percent = Transform(vec![TransformOp::Translate3d(
        Size::Percent(4.),
        Size::Px(8.),
        12.,
    )]);
    assert!(!Transform::can_interpolate(&px, &percent));
}

#[test]
fn colors_text_and_flex_use_typed_values() {
    let mut style = style! {
        display: flex;
        box-sizing: border-box;
        flex-direction: column;
        flex-wrap: wrap;
        align-items: center;
        justify-content: space-between;
        overflow: hidden scroll;
        position: absolute;
        aspect-ratio: 16 / 9;
        flex: 1 0 auto;
        font-family: "Noto Sans";
        font-weight: 600;
        font-style: italic;
        font-size: 1.25rem;
        line-height: 1.5;
        text-align: center;
        text-decoration: underline line-through;
        white-space: pre-wrap;
        text-overflow: ellipsis;
        background-color: #1234;
        color: rgb(100% 0% 50% / 25%);
        opacity: 80%;
    };
    assert_eq!(
        style.take_background().get_clone(),
        Color::from_rgba(17, 34, 51, 68).into()
    );
    assert_eq!(
        style.take_color().get_clone(),
        Color::new(1., 0., 0.5, 0.25).into()
    );
    assert_eq!(style.take_font_size().get(), FontSize::Rem(1.25));
    assert_eq!(
        style.take_font_weight().get(),
        mue_macroquad::font::Weight(600)
    );
    assert_eq!(style.take_line_height_multiplier().get(), Some(1.5));
    assert_eq!(style.take_flex_grow().get(), 1.);
    assert_eq!(style.take_flex_shrink().get(), 0.);
    assert_eq!(style.take_flex_basis().get(), Size::Auto);
    assert_eq!(style.take_opacity().get(), 0.8);
    assert!(style.take_text_underline().get());
    assert!(style.take_text_strikethrough().get());
    assert_eq!(style.take_aspect_ratio().get(), Some(16. / 9.));
    let mut style = style! { color: rgba(255, 128, 0, 0.5); flex: 2 30px; };
    assert_eq!(
        style.take_color().get_clone(),
        Color::new(1., 128. / 255., 0., 0.5).into()
    );
    assert_eq!(style.take_flex_basis().get(), Size::Px(30.));
}

#[test]
fn transform_uses_border_box_percentages_origin_and_style_order() {
    let mut style = style! {
        transform: translate(50%, -10px) rotate(90deg) scale(2, 1);
        transform-origin: left top;
    };
    let bounds = Rect::new(100., 200., 80., 40.);
    let transform = style.take_transform_list().get_clone();
    let origin = style.take_transform_origin().get();
    let matrix = transform.resolve(bounds, origin);
    let transformed = matrix.transform_point2(Vector::new(110., 200.));
    assert!((transformed - Vector::new(140., 210.)).length() < 0.001);
    let inverse = matrix.inverse().transform_point2(transformed);
    assert!((inverse - Vector::new(110., 200.)).length() < 0.001);
    let mut center = style! { transform: rotate(0.5turn); };
    let matrix = center
        .take_transform_list()
        .get_clone()
        .resolve(bounds, center.take_transform_origin().get());
    assert!((matrix.transform_point2(bounds.center()) - bounds.center()).length() < 0.001);
    let mut reset = style! { transform: rotate(30deg); transform: none; };
    assert_eq!(
        reset.take_transform_list().get_clone(),
        Transform::default()
    );
    let mut matrix = style! { transform: matrix(1, 0, 0, 1, 12, 24); };
    assert_eq!(
        matrix.take_transform_list().get_clone(),
        Transform(vec![TransformOp::Matrix(Matrix::from_translation(
            Vector::new(12., 24.)
        ))])
    );
}

#[test]
fn applying_style_obeys_call_order_and_keeps_events() {
    let handler = std::cell::Cell::new(false);
    let mut builder = flexbox()
        .width(1.)
        .on_activate(move || handler.set(true))
        .style(style! { width: 20px; padding: 8px; })
        .height(40.);
    let style = builder.style_mut();
    assert_eq!(style.take_width().get(), Size::Px(20.));
    assert_eq!(style.take_height().get(), Size::Px(40.));
    assert!(!style.on_tap.is_empty());
    let mut style =
        style! { transform: {Prop::Static(Matrix::IDENTITY)}; transform-origin: top right; };
    assert_eq!(
        style.take_transform_origin().get(),
        TransformOrigin(Size::Percent(100.), Size::Percent(0.))
    );
    assert_eq!(style.take_matrix_transform().get(), Matrix::IDENTITY);

    let mut style = style! { line-height: 1.5; transform: rotate(30deg); }
        .line_height(24.)
        .transform(Matrix::IDENTITY);
    assert_eq!(style.take_line_height_multiplier().get(), None);
    assert_eq!(style.take_line_height().get(), Size::Px(24.));
    assert_eq!(
        style.take_transform_list().get_clone(),
        Transform::default()
    );
    let mut style = style! { transform: {Matrix::IDENTITY}; transform: translateX(20px); };
    assert_eq!(style.take_matrix_transform().get(), Matrix::IDENTITY);
    assert_eq!(
        style.take_transform_list().get_clone(),
        Transform(vec![TransformOp::Translate(Size::Px(20.), Size::Px(0.))])
    );
}

#[test]
fn all_components_are_available_in_the_component_crate() {
    fn component<T: mue_components::Component>(_: T) {}
    component(scroll_area());
    component(circle());
    component(document(Document::markdown("**Hello**")));
    assert_eq!(
        Document::heading("Title", HeadingLevel::H1).spans,
        [Span::new("Title").heading(HeadingLevel::H1)]
    );
    // Texture and geometry components are checked without creating a GPU resource.
    fn image_builder(_: ImageBuilder) {}
    fn shape_builder(_: ShapeBuilder) {}
    let _ = (image_builder, shape_builder);
}
