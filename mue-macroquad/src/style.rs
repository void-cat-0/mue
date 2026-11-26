mod define;
mod ext;
mod rect;
mod store;
mod taffy_style;
mod transform;
use define::define_style;
pub use rect::IntoRectProp;
pub use transform::{IntoTransformProp, Transform, TransformOp, TransformOrigin, TransformProp};
pub mod units;

pub use units::{FontSize, FontSizeExt, Size, SizeExt, percent, px, vh, vw};

pub use crate::shadow::ShadowCache;
use crate::{clip::Sector, color::Color, shader::Brush, shadow::Shadow};
use cosmic_text::Align;
use cosmic_text::{FamilyOwned, Weight};
pub use ext::StyleableExt;

use mue_core::{effect::computed, prop::Prop, signal::Access};
use paste::paste;
use std::{mem, rc::Rc};

use taffy::{BoxSizing, Display, Overflow, Position};

use crate::{
    event::{pointer::PointerEvent, wheel::WheelEvent},
    gesture::{PanEvent, PinchEvent, RotateEvent, SwipeEvent, TransformEvent},
    hook::{CallbackList, NodeHooks},
    math::Matrix,
    node::{ChildrenPlan, HoverOwner, IntoChildren},
};

define_style! {
    // Taffy
    box_sizing: BoxSizing;

    display: Display;

    /// Set the preferred width; numbers are pixels and the default is auto.
    width: Size = Size::Auto;
    /// Set the preferred height; numbers are pixels and the default is auto.
    height: Size = Size::Auto;

    /// Set a lower bound on width.
    min_width: Size = Size::Auto;
    /// Set a lower bound on height.
    min_height: Size = Size::Auto;

    /// Set an upper bound on width.
    max_width: Size = Size::Auto;
    /// Set an upper bound on height.
    max_height: Size = Size::Auto;

    overflow_x: Overflow = Overflow::Visible;
    overflow_y: Overflow = Overflow::Visible;

    /// Align children along the flex cross axis.
    align_items: Option<taffy::AlignItems>;
    align_self: Option<taffy::AlignSelf>;
    justify_items: Option<taffy::AlignItems>;
    justify_self: Option<taffy::AlignSelf>;
    align_content: Option<taffy::AlignContent>;
    /// Distribute children along the flex main axis.
    justify_content: Option<taffy::JustifyContent>;

    /// Choose normal relative positioning or absolute positioning with insets.
    position: Position;

    /// Choose the main axis and ordering for flex children.
    flex_direction: taffy::FlexDirection;
    flex_wrap: taffy::FlexWrap;

    /// Horizontal space between columns and vertical space between rows.
    column_gap: Size = Size::Px(0.);
    row_gap: Size = Size::Px(0.);
    /// Preferred width divided by height, when one dimension is automatic.
    aspect_ratio: Option<f32>;

    flex_basis: Size = Size::Auto;
    /// Set how much of the parent's remaining main-axis space this node takes.
    flex_grow: f32 = 0.0;
    /// Set how much this node shrinks when flex children exceed available space.
    flex_shrink: f32 = 1.0;

    transition: crate::transition::Transition = crate::transition::Transition::Instant;

    // Paint
      /// Transform this node and its descendants without changing layout size.
    matrix_transform: Matrix = Matrix::IDENTITY;
    /// Transform functions, applied in order around the border-box origin.
    transform_list: Transform;
    transform_origin: TransformOrigin;
    /// Multiply painted alpha; 0 is transparent and 1 is opaque.
    opacity: f32 = 1.0;
    /// Paint the background with a solid color, gradient, or texture brush.
    background: Brush = Brush::default();
    /// Set the foreground brush for text and image tint.
    color: Brush = Brush::Solid(Color::WHITE);
    /// Round the top-left background, border, and outline corner (px/vw/vh).
    /// Set all four corners with [`StyleableExt::radius`].
    radius_top_left: Size = Size::Px(0.);
    /// Round the top-right corner. See `radius_top_left`.
    radius_top_right: Size = Size::Px(0.);
    /// Round the bottom-right corner. See `radius_top_left`.
    radius_bottom_right: Size = Size::Px(0.);
    /// Round the bottom-left corner. See `radius_top_left`.
    radius_bottom_left: Size = Size::Px(0.);
    /// Paint the border area set by `border` widths. Transparent by default.
    border_color: Brush = Brush::default();
    /// Draw an outline outside the border box, above the children. It does not
    /// affect layout or hit testing. Zero width draws nothing.
    outline_width: Size = Size::Px(0.);
    /// Paint the outline. Transparent by default.
    outline_color: Brush = Brush::default();
    /// Distance between the border box and the outline; negative values inset it.
    outline_offset: Size = Size::Px(0.);
    /// Draw a drop shadow behind the background.
    shadow: Shadow;
    /// Select shared mask admission (Auto by default). Exact resident masks reuse immediately;
    /// new masks require stable parameters, and Auto also requires substantial recent coverage.
    /// Currently native OpenGL only; unsupported, small or changing shadows stay analytic.
    shadow_cache: ShadowCache = ShadowCache::Auto;
    /// Clip children to a rounded rectangle with this radius (px/vw/vh;
    /// percent is not resolved).
    clip_radius: Size = Size::Px(0.);
    /// Clip children to an angular sector.
    clip_sector: Sector;

    // Text
    /// Align shaped lines within the text node's content width.
    text_align: Option<Align>;
    /// Set glyph size; percent/em use the parent computed size, rem uses the root.
    /// Unspecified sizes inherit; the root defaults to 32 logical pixels.
    font_size: FontSize = FontSize::Em(1.);
    /// Set baseline spacing in pixels.
    line_height: Size = Size::Px(44.);
    line_height_multiplier: Option<f32>;
    /// Choose a registered font family or a generic family.
    font_family: FamilyOwned = FamilyOwned::SansSerif;
    /// Choose a font weight; the matching face must be registered or installed.
    font_weight: Weight = Weight::NORMAL;
    /// Use italic font faces where available.
    font_italic: bool = false;
    /// Wrap long lines at words or glyphs. Disable for preformatted code.
    text_wrap: bool = true;
    /// Underline each shaped line with the foreground brush.
    text_underline: bool = false;
    text_strikethrough: bool = false;
    text_wrap_mode: crate::node::text::TextWrap;
    text_overflow: crate::node::text::TextOverflow;
    /// Maximum visual lines. None displays the whole document.
    text_max_lines: Option<usize>;
    text_vertical_align: crate::node::text::TextVerticalAlign;
    /// Extra space between newline-delimited paragraphs, in logical pixels.
    paragraph_spacing: Size = Size::Px(0.);
    letter_spacing: Size = Size::Px(0.);
    tab_width: u16 = 4;

    @rects {
        margin: taffy::Rect<Size> = Size::Px(0.);
        padding: taffy::Rect<Size> = Size::Px(0.);
        border: taffy::Rect<Size> = Size::Px(0.);
        inset: taffy::Rect<Size> = Size::Auto;
    }

    @hooks {
        /// Advance local frame state once per App frame, with delta time in seconds.
        /// Layout preparation and repeated drawing do not invoke this hook.
        on_update: CallbackList<f32>;
        /// Prepare geometry after publishing this node's absolute rect, before its children.
        /// May run repeatedly while layout stabilizes; do not advance time here.
        on_layout: CallbackList<()>;
        /// Draw using prepared geometry. Do not change layout or advance time here.
        on_render: CallbackList<()>;
        on_pointer_event: CallbackList<PointerEvent>;
        on_hover_event: CallbackList<PointerEvent>;
        on_wheel_event: CallbackList<WheelEvent>;

        on_tap: CallbackList<()>;
        on_long_press: CallbackList<()>;
        on_pan: CallbackList<PanEvent>;
        on_swipe: CallbackList<SwipeEvent>;
        on_pinch: CallbackList<PinchEvent>;
        on_rotate: CallbackList<RotateEvent>;
        on_transform: CallbackList<TransformEvent>;
    }
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the deferred input for mounting inside the node's scope.
    pub(crate) fn take_children_plan(&mut self) -> Option<ChildrenPlan> {
        self.children.take()
    }

    pub(crate) fn ensure_hovered(&mut self) -> Rc<HoverOwner> {
        if let Some(owner) = self.hover_owners.first() {
            return owner.clone();
        }
        let owner = HoverOwner::new();
        self.hover_owners.push(owner.clone());
        owner
    }

    pub(crate) fn build_hooks(&mut self) -> NodeHooks {
        NodeHooks {
            update: self.take_on_update(),
            layout: self.take_on_layout(),
            render: self.take_on_render(),
            after_children: CallbackList::default(),
            after_clip: CallbackList::default(),
            pointer_event: self.take_on_pointer_event(),
            hover_event: self.take_on_hover_event(),
            wheel_event: self.take_on_wheel_event(),
        }
    }
}

impl Style {
    fn append_hover_owners(&mut self, owners: Vec<Rc<HoverOwner>>) {
        for owner in owners {
            if !self
                .hover_owners
                .iter()
                .any(|existing| Rc::ptr_eq(existing, &owner))
            {
                self.hover_owners.push(owner);
            }
        }
    }
}

/// Enumerations used by style declarations without requiring direct Taffy dependencies.
pub mod values {
    pub use cosmic_text::Align as TextAlign;
    pub use taffy::{
        AlignContent, AlignItems, BoxSizing, Display, FlexDirection, FlexWrap, Overflow, Position,
    };
}
