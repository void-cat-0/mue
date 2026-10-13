use super::{Prop, PropValue};

impl PropValue for cosmic_text::FamilyOwned {}

impl From<&str> for Prop<cosmic_text::FamilyOwned> {
    fn from(name: &str) -> Self {
        Prop::Static(cosmic_text::FamilyOwned::Name(name.into()))
    }
}

impl From<String> for Prop<cosmic_text::FamilyOwned> {
    fn from(name: String) -> Self {
        Prop::Static(cosmic_text::FamilyOwned::Name(name.into()))
    }
}

impl PropValue for cosmic_text::Weight {}
