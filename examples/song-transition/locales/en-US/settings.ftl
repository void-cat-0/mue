title = APPLICATION SETTINGS
description =
    Changes apply immediately to every Scene.
    Return keeps the original list position and selected song.
theme-button =
    Theme: { $theme ->
        [light] Light
       *[dark] Dark
    }  [T]
motion-button =
    Reduce artwork motion: { $reduced ->
        [on] on
       *[off] off
    }  [M]
locale-button =
    Language: { $locale ->
        [zh-CN] Simplified Chinese
       *[en-US] English
    }  [L]
motion-tip =
    With reduced motion on, the next open/return switches immediately.
    Settings navigation itself is always immediate.
back-scene = Back to previous Scene  [Esc]
shortcuts = T: theme    M: artwork motion    L: language    Esc: back    Q: exit
