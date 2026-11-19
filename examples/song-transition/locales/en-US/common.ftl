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
