//! Shared tab palette. Convert with `.into()` for macroquad drawing calls.
#![allow(dead_code)]

use mue_macroquad::color::Color;

pub const CANVAS: Color = Color::BLACK;
pub const INK: Color = Color::WHITE;
pub const SURFACE: Color = Color::from_rgba(25, 30, 40, 255);
pub const PRESSED: Color = Color::from_rgba(15, 20, 30, 255);
pub const HOVER: Color = Color::from_rgba(55, 65, 85, 255);
pub const ACTIVE: Color = Color::from_rgba(70, 95, 130, 255);
pub const HIGHLIGHT: Color = Color::from_rgba(100, 135, 180, 255);
pub const BLUE: Color = Color::from_rgba(37, 58, 94, 255);
pub const GREEN: Color = Color::from_rgba(46, 82, 63, 255);
pub const PLUM: Color = Color::from_rgba(88, 52, 84, 255);
