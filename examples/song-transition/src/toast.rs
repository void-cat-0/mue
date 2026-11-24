//! The demo decides what to say. The reusable host owns timing and presentation.
use mue_components::{ToastContent, ToastKind, ToastSender};
use mue_i18n::{Translator, tl};

pub enum Notice {
    OpeningSong(&'static str),
    SongOffscreen,
    SettingsUpdated,
    DialogAcknowledged,
}

#[derive(Clone)]
pub struct Notices {
    sender: ToastSender,
    common: Translator,
}

impl Notices {
    pub fn new(sender: ToastSender, common: Translator) -> Self {
        Self { sender, common }
    }

    pub fn show(&self, notice: Notice) {
        let common = self.common.clone();
        let (kind, label) = match notice {
            Notice::OpeningSong(_) => (ToastKind::Info, "toast-info"),
            Notice::SongOffscreen => (ToastKind::Warning, "toast-warning"),
            Notice::SettingsUpdated | Notice::DialogAcknowledged => {
                (ToastKind::Success, "toast-success")
            }
        };
        self.sender.show_with(kind, move || {
            let message = match notice {
                Notice::OpeningSong(song) => tl!(common, "toast-opening-song", "song" => song),
                Notice::SongOffscreen => common.text("toast-song-offscreen"),
                Notice::SettingsUpdated => common.text("toast-settings-updated"),
                Notice::DialogAcknowledged => common.text("toast-dialog-acknowledged"),
            };
            ToastContent::new(message).label(common.text(label))
        });
    }
}
