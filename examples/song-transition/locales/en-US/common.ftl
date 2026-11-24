open-settings = Settings  [S]
settings-status =
    { $theme ->
        [light] Light theme
       *[dark] Dark theme
    } / Artwork motion: { $motion ->
        [off] off
       *[on] on
    } / Language: { $locale ->
        [zh-CN] Simplified Chinese
       *[en-US] English
    }
artist-description = { $artist } / { $description }
track-level = { $bpm } BPM / { $difficulty } { $level }
genre-progressive = melodic progressive
genre-garage = future garage
genre-drum-bass = ambient drum & bass
genre-sunset = sunset sessions
genre-night-drive = night drive
genre-quiet-currents = quiet currents

toast-info = Info
toast-warning = Warning
toast-success = Success
toast-opening-song = Opening { $song }
toast-song-offscreen = Scroll to the song before opening it.
toast-settings-updated = Session settings updated.
toast-dialog-acknowledged = Dialog acknowledged.

dialog-song-title = Opening { $song }
dialog-song-body = This dialog belongs to the app, not the song library. The artwork continues into the track details behind it, and the same dialog stays open.
    While it is open, the page underneath cannot be clicked. Press Escape or click the backdrop to dismiss it without navigating back.
dialog-dismiss = Dismiss
dialog-confirm = Got it
